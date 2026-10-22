//! Real-time stream ingestion (`semantica/ingest/stream_ingestor.py`): a generic processor with
//! transformation/validation, error handling and statistics, consuming from Kafka (Confluent
//! REST Proxy), RabbitMQ (management HTTP API), AWS Kinesis (JSON API + SigV4) or Apache Pulsar
//! (WebSocket consumer API) through the [`StreamSource`] trait, plus health monitoring.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use base64::Engine;
use indexmap::IndexMap;
use sem_utils::logging::Logger;
use sem_utils::pycompat::time::PyDateTime;
use sem_utils::{SemanticaError, get_logger};
use serde_json::{Map, Value, json};

use crate::config::{processing_error, validation_error};
use crate::support::aws::{self, AwsCredentials};
use crate::support::hooks::Hook;
use crate::support::kw::{self, Kwargs};
use crate::support::{pycodec, pyjson, pytime};

/// `StreamMessage`.
#[derive(Debug, Clone, PartialEq)]
pub struct StreamMessage {
    /// `content`.
    pub content: Value,
    /// `metadata`.
    pub metadata: Map<String, Value>,
    /// `timestamp`.
    pub timestamp: PyDateTime,
    /// `source`.
    pub source: String,
    /// `partition`.
    pub partition: Option<i64>,
    /// `offset`.
    pub offset: Option<i64>,
}

impl StreamMessage {
    /// `StreamMessage(content)` with the other defaults.
    pub fn new(content: Value) -> StreamMessage {
        StreamMessage {
            content,
            metadata: Map::new(),
            timestamp: PyDateTime::now(false),
            source: String::new(),
            partition: None,
            offset: None,
        }
    }
}

/// A raw message as a source delivers it (Python's `bytes` / `str` / `dict` / other).
#[derive(Debug, Clone, PartialEq)]
pub enum StreamPayload {
    /// `bytes` (JSON text).
    Bytes(Vec<u8>),
    /// `str` (JSON text).
    Text(String),
    /// An already-decoded object.
    Json(Value),
}

/// A delivered message with its acknowledgement token.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceMessage {
    /// The payload.
    pub payload: StreamPayload,
    /// Delivery tag / message id used to (n)ack.
    pub token: Option<String>,
}

/// One streaming backend.
pub trait StreamSource: Send {
    /// Waits (about a second at most) for the next messages.
    fn receive(&mut self) -> Result<Vec<SourceMessage>, SemanticaError>;
    /// Acknowledges a processed message.
    fn ack(&mut self, _message: &SourceMessage) -> Result<(), SemanticaError> {
        Ok(())
    }
    /// Negatively acknowledges (requeues) a message.
    fn nack(&mut self, _message: &SourceMessage) -> Result<(), SemanticaError> {
        Ok(())
    }
    /// Marks the last received batch as fully processed (Kinesis advances its shard
    /// iterator only then).
    fn commit(&mut self) -> Result<(), SemanticaError> {
        Ok(())
    }
    /// Releases the connection.
    fn close(&mut self) {}
}

/// Which consumption loop a processor runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamKind {
    /// The base `StreamProcessor` (sleeps).
    Generic,
    /// `KafkaProcessor`.
    Kafka,
    /// `RabbitMQProcessor`.
    RabbitMQ,
    /// `KinesisProcessor`.
    Kinesis,
    /// `PulsarProcessor`.
    Pulsar,
}

type Transform = Arc<dyn Fn(Value) -> Value + Send + Sync>;
type Validate = Arc<dyn Fn(&Value) -> bool + Send + Sync>;
type MessageHandler = Arc<dyn Fn(Option<&Map<String, Value>>) + Send + Sync>;
type ErrorHandler = Arc<dyn Fn(&StreamPayload, &SemanticaError) + Send + Sync>;

/// `**options` of a processor.
#[derive(Clone, Default)]
pub struct StreamOptions {
    /// `transform`.
    pub transform: Option<Transform>,
    /// `validate`.
    pub validate: Option<Validate>,
    /// Remaining options (`consumer_config`, `durable`, `shard_id`, ...).
    pub extra: Kwargs,
}

impl std::fmt::Debug for StreamOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamOptions")
            .field("transform", &self.transform.is_some())
            .field("validate", &self.validate.is_some())
            .field("extra", &self.extra)
            .finish()
    }
}

impl StreamOptions {
    /// Options from plain keyword values.
    pub fn from_kwargs(extra: &Kwargs) -> StreamOptions {
        StreamOptions {
            transform: None,
            validate: None,
            extra: extra.clone(),
        }
    }
}

struct Shared {
    message_handler: Mutex<Option<MessageHandler>>,
    error_handler: Mutex<Option<ErrorHandler>>,
    processed: AtomicU64,
    errors: AtomicU64,
    running: Arc<AtomicBool>,
}

/// `StreamProcessor` (and, through [`StreamKind`], its Kafka/RabbitMQ/Kinesis/Pulsar
/// subclasses).
pub struct StreamProcessor {
    /// `source_config`.
    pub source_config: Map<String, Value>,
    /// `options`.
    pub options: StreamOptions,
    /// The consumption loop kind.
    pub kind: StreamKind,
    shared: Arc<Shared>,
    source: Arc<Mutex<Option<Box<dyn StreamSource>>>>,
    thread: Mutex<Option<JoinHandle<()>>>,
    logger: Logger,
}

impl std::fmt::Debug for StreamProcessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamProcessor")
            .field("source_config", &self.source_config)
            .field("kind", &self.kind)
            .field("running", &self.running())
            .finish()
    }
}

fn process_payload(
    shared: &Shared,
    options: &StreamOptions,
    logger: &Logger,
    message: &StreamPayload,
) -> Result<Option<Map<String, Value>>, SemanticaError> {
    let result = (|| -> Result<Map<String, Value>, SemanticaError> {
        let content = match message {
            StreamPayload::Bytes(b) => pyjson::loads(&pycodec::decode_utf8(b)?)?,
            StreamPayload::Text(t) => pyjson::loads(t)?,
            StreamPayload::Json(Value::Object(m)) => Value::Object(m.clone()),
            StreamPayload::Json(other) => json!({"raw": sem_utils::pycompat::fmt::py_str(other)}),
        };
        let metadata = match &content {
            Value::Object(m) => m.get("metadata").cloned().unwrap_or(json!({})),
            other => {
                return Err(SemanticaError::Attribute(format!(
                    "'{}' object has no attribute 'get'",
                    sem_utils::pycompat::fmt::type_name(other)
                )));
            }
        };
        let mut content = content;
        if let Some(t) = &options.transform {
            content = t(content);
        }
        if let Some(v) = &options.validate
            && !v(&content)
        {
            return Err(validation_error("Message validation failed"));
        }
        shared.processed.fetch_add(1, Ordering::SeqCst);
        Ok(kw::obj([
            ("content", content),
            ("metadata", metadata),
            ("processed_at", Value::String(PyDateTime::now(false).isoformat('T'))),
        ]))
    })();
    match result {
        Ok(m) => Ok(Some(m)),
        Err(e) => {
            shared.errors.fetch_add(1, Ordering::SeqCst);
            logger.error(format!("Failed to process message: {e}"));
            let handler = shared.error_handler.lock().ok().and_then(|h| h.clone());
            match handler {
                Some(h) => {
                    h(message, &e);
                    Ok(None)
                }
                None => Err(processing_error(format!("Failed to process message: {e}"))),
            }
        }
    }
}

fn with_source<T>(
    source: &Mutex<Option<Box<dyn StreamSource>>>,
    f: impl FnOnce(&mut dyn StreamSource) -> Result<T, SemanticaError>,
    default: T,
) -> Result<T, SemanticaError> {
    let mut guard = match source.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    match guard.as_mut() {
        Some(s) => f(s.as_mut()),
        None => Ok(default),
    }
}

fn message_handler(shared: &Shared) -> Option<MessageHandler> {
    shared.message_handler.lock().ok().and_then(|h| h.clone())
}

/// `process_message(...)` then `message_handler(processed)` (the handler also receives
/// `None` when an error handler swallowed a failure, as in the original).
fn process_and_handle(
    shared: &Shared,
    options: &StreamOptions,
    logger: &Logger,
    payload: &StreamPayload,
) -> Result<(), SemanticaError> {
    let processed = process_payload(shared, options, logger, payload)?;
    if let Some(h) = message_handler(shared) {
        h(processed.as_ref());
    }
    Ok(())
}

fn consume_loop(
    kind: StreamKind,
    shared: Arc<Shared>,
    source: Arc<Mutex<Option<Box<dyn StreamSource>>>>,
    options: StreamOptions,
    logger: Logger,
) {
    let running = || shared.running.load(Ordering::SeqCst);
    match kind {
        StreamKind::Generic => {
            while running() {
                pytime::sleep(0.1);
            }
        }
        StreamKind::Kafka => {
            // `consumer.poll()`; a failure anywhere in a batch abandons the rest of it.
            while running() {
                let result = with_source(&source, |s| s.receive(), Vec::new()).and_then(|messages| {
                    for message in &messages {
                        process_and_handle(&shared, &options, &logger, &message.payload)?;
                    }
                    Ok(())
                });
                if let Err(e) = result {
                    logger.error(format!("Error consuming from Kafka: {e}"));
                    if !running() {
                        break;
                    }
                }
            }
        }
        StreamKind::RabbitMQ => {
            // `basic_consume` callback: ack on success, nack (requeue) on failure; a
            // connection failure ends the loop, which closes the channel and connection.
            while running() {
                let messages = match with_source(&source, |s| s.receive(), Vec::new()) {
                    Ok(m) => m,
                    Err(e) => {
                        logger.error(format!("Error in RabbitMQ consumption: {e}"));
                        break;
                    }
                };
                for message in messages {
                    match process_and_handle(&shared, &options, &logger, &message.payload) {
                        Ok(()) => {
                            let _ = with_source(&source, |s| s.ack(&message), ());
                        }
                        Err(e) => {
                            logger.error(format!("Error processing RabbitMQ message: {e}"));
                            let _ = with_source(&source, |s| s.nack(&message), ());
                        }
                    }
                }
            }
            let _ = with_source(
                &source,
                |s| {
                    s.close();
                    Ok(())
                },
                (),
            );
        }
        StreamKind::Kinesis => {
            // `get_records`; the shard iterator only advances once the whole batch was
            // processed, otherwise the batch is read again after 5 seconds.
            while running() {
                let result = with_source(&source, |s| s.receive(), Vec::new()).and_then(|records| {
                    for record in &records {
                        process_and_handle(&shared, &options, &logger, &record.payload)?;
                    }
                    with_source(&source, |s| s.commit(), ())
                });
                match result {
                    Ok(()) => pytime::sleep(1.0),
                    Err(e) => {
                        logger.error(format!("Error consuming from Kinesis: {e}"));
                        if !running() {
                            break;
                        }
                        pytime::sleep(5.0);
                    }
                }
            }
        }
        StreamKind::Pulsar => {
            while running() {
                match with_source(&source, |s| s.receive(), Vec::new()) {
                    Ok(messages) => {
                        for message in messages {
                            match process_and_handle(&shared, &options, &logger, &message.payload) {
                                Ok(()) => {
                                    let _ = with_source(&source, |s| s.ack(&message), ());
                                }
                                Err(e) => {
                                    logger.error(format!("Error processing Pulsar message: {e}"));
                                    let _ = with_source(&source, |s| s.nack(&message), ());
                                }
                            }
                        }
                    }
                    Err(e) => {
                        if !e.to_string().to_lowercase().contains("timeout") {
                            logger.error(format!("Error consuming from Pulsar: {e}"));
                        }
                        if !running() {
                            break;
                        }
                    }
                }
            }
        }
    }
}

impl StreamProcessor {
    /// `StreamProcessor(source_config, **options)`.
    pub fn new(source_config: Map<String, Value>, options: StreamOptions) -> StreamProcessor {
        StreamProcessor::with_source(StreamKind::Generic, source_config, options, None)
    }

    /// A processor of `kind` consuming from `source`.
    pub fn with_source(
        kind: StreamKind,
        source_config: Map<String, Value>,
        options: StreamOptions,
        source: Option<Box<dyn StreamSource>>,
    ) -> StreamProcessor {
        StreamProcessor {
            source_config,
            options,
            kind,
            shared: Arc::new(Shared {
                message_handler: Mutex::new(None),
                error_handler: Mutex::new(None),
                processed: AtomicU64::new(0),
                errors: AtomicU64::new(0),
                running: Arc::new(AtomicBool::new(false)),
            }),
            source: Arc::new(Mutex::new(source)),
            thread: Mutex::new(None),
            logger: get_logger("stream_processor"),
        }
    }

    /// `running`.
    pub fn running(&self) -> bool {
        self.shared.running.load(Ordering::SeqCst)
    }

    /// `processor.running = value`.
    pub fn set_running(&self, value: bool) {
        self.shared.running.store(value, Ordering::SeqCst);
    }

    /// The shared `running` flag (what a consumption loop observes).
    pub fn running_handle(&self) -> Arc<AtomicBool> {
        self.shared.running.clone()
    }

    /// `process_message(message)`: decodes, transforms and validates a message.
    pub fn process_message(&self, message: &StreamPayload) -> Result<Option<Map<String, Value>>, SemanticaError> {
        process_payload(&self.shared, &self.options, &self.logger, message)
    }

    /// `start_consuming()`.
    pub fn start_consuming(&self) {
        if self.running() {
            self.logger.warning("Processor already running");
            return;
        }
        self.shared.running.store(true, Ordering::SeqCst);
        let kind = self.kind;
        let shared = self.shared.clone();
        let source = self.source.clone();
        let options = self.options.clone();
        let logger = self.logger.clone();
        let handle = std::thread::spawn(move || consume_loop(kind, shared, source, options, logger));
        if let Ok(mut t) = self.thread.lock() {
            *t = Some(handle);
        }
        self.logger.info("Stream processor started");
    }

    /// `stop_consuming()` (waits up to 5 seconds for the loop).
    pub fn stop_consuming(&self) {
        self.shared.running.store(false, Ordering::SeqCst);
        if let Ok(mut t) = self.thread.lock()
            && let Some(handle) = t.take()
        {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !handle.is_finished() && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            if handle.is_finished() {
                let _ = handle.join();
            }
        }
        self.logger.info("Stream processor stopped");
    }

    /// `set_message_handler(handler)`: called with each processed message (`None` when an
    /// error handler swallowed the failure).
    pub fn set_message_handler(&self, handler: impl Fn(Option<&Map<String, Value>>) + Send + Sync + 'static) {
        if let Ok(mut h) = self.shared.message_handler.lock() {
            *h = Some(Arc::new(handler));
        }
    }

    /// `set_error_handler(handler)`.
    pub fn set_error_handler(&self, handler: impl Fn(&StreamPayload, &SemanticaError) + Send + Sync + 'static) {
        if let Ok(mut h) = self.shared.error_handler.lock() {
            *h = Some(Arc::new(handler));
        }
    }

    /// `_consume_loop()` on the calling thread.
    pub fn _consume_loop(&self) {
        consume_loop(
            self.kind,
            self.shared.clone(),
            self.source.clone(),
            self.options.clone(),
            self.logger.clone(),
        );
    }

    /// `get_stats()`.
    pub fn get_stats(&self) -> Map<String, Value> {
        kw::obj([
            ("processed", json!(self.shared.processed.load(Ordering::SeqCst))),
            ("errors", json!(self.shared.errors.load(Ordering::SeqCst))),
            ("running", Value::Bool(self.running())),
        ])
    }
}

fn http_client() -> Result<reqwest::blocking::Client, SemanticaError> {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| SemanticaError::Runtime(e.to_string()))
}

fn http_error(e: impl std::fmt::Display) -> SemanticaError {
    SemanticaError::Runtime(e.to_string())
}

/// Kafka through the Confluent REST Proxy (v2 consumer API).
pub struct KafkaRestSource {
    base_uri: String,
    client: reqwest::blocking::Client,
}

impl KafkaRestSource {
    /// Creates a consumer instance in `group` and subscribes it to `topic`.
    pub fn connect(proxy_url: &str, group: &str, topic: &str, config: &Kwargs) -> Result<KafkaRestSource, SemanticaError> {
        let client = http_client()?;
        let mut body = json!({"format": "json", "auto.offset.reset": config.get("auto_offset_reset").and_then(Value::as_str).unwrap_or("latest")});
        if let Some(name) = config.get("client_id").and_then(Value::as_str) {
            body["name"] = json!(name);
        }
        let created: Value = client
            .post(format!("{}/consumers/{group}", proxy_url.trim_end_matches('/')))
            .header("Content-Type", "application/vnd.kafka.v2+json")
            .body(body.to_string())
            .send()
            .and_then(|r| r.error_for_status())
            .map_err(http_error)?
            .json()
            .map_err(http_error)?;
        let base_uri = created
            .get("base_uri")
            .and_then(Value::as_str)
            .ok_or_else(|| SemanticaError::Runtime("REST proxy returned no base_uri".to_string()))?
            .to_string();
        client
            .post(format!("{base_uri}/subscription"))
            .header("Content-Type", "application/vnd.kafka.v2+json")
            .body(json!({"topics": [topic]}).to_string())
            .send()
            .and_then(|r| r.error_for_status())
            .map_err(http_error)?;
        Ok(KafkaRestSource { base_uri, client })
    }
}

impl StreamSource for KafkaRestSource {
    fn receive(&mut self) -> Result<Vec<SourceMessage>, SemanticaError> {
        let records: Value = self
            .client
            .get(format!("{}/records?timeout=1000", self.base_uri))
            .header("Accept", "application/vnd.kafka.json.v2+json")
            .send()
            .and_then(|r| r.error_for_status())
            .map_err(http_error)?
            .json()
            .map_err(http_error)?;
        Ok(records
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|r| SourceMessage {
                payload: StreamPayload::Json(r.get("value").cloned().unwrap_or(Value::Null)),
                token: r.get("offset").map(|o| o.to_string()),
            })
            .collect())
    }

    fn close(&mut self) {
        let _ = self.client.delete(&self.base_uri).send();
    }
}

/// RabbitMQ through the management HTTP API (`/api/queues/<vhost>/<queue>/get`).
pub struct RabbitMqHttpSource {
    api: String,
    vhost: String,
    queue: String,
    user: String,
    password: String,
    client: reqwest::blocking::Client,
}

impl RabbitMqHttpSource {
    /// Derives the management endpoint from an `amqp://user:pass@host:port/vhost` URL and
    /// declares the queue.
    pub fn connect(connection_url: &str, queue: &str, durable: bool, management_url: Option<&str>) -> Result<RabbitMqHttpSource, SemanticaError> {
        let parsed = url::Url::parse(connection_url).map_err(|e| SemanticaError::Value(e.to_string()))?;
        let host = parsed.host_str().unwrap_or("localhost").to_string();
        let user = if parsed.username().is_empty() { "guest".to_string() } else { parsed.username().to_string() };
        let password = parsed.password().unwrap_or("guest").to_string();
        let vhost = match parsed.path().trim_start_matches('/') {
            "" => "/".to_string(),
            v => crate::support::pyurl::unquote(v),
        };
        let api = match management_url {
            Some(u) => u.trim_end_matches('/').to_string(),
            None => format!("http://{host}:15672/api"),
        };
        let source = RabbitMqHttpSource {
            api,
            vhost,
            queue: queue.to_string(),
            user,
            password,
            client: http_client()?,
        };
        source
            .client
            .put(format!(
                "{}/queues/{}/{}",
                source.api,
                aws::uri_encode(&source.vhost, false),
                aws::uri_encode(queue, false)
            ))
            .basic_auth(&source.user, Some(&source.password))
            .json(&json!({"durable": durable}))
            .send()
            .and_then(|r| r.error_for_status())
            .map_err(http_error)?;
        Ok(source)
    }
}

impl StreamSource for RabbitMqHttpSource {
    fn receive(&mut self) -> Result<Vec<SourceMessage>, SemanticaError> {
        let messages: Value = self
            .client
            .post(format!(
                "{}/queues/{}/{}/get",
                self.api,
                aws::uri_encode(&self.vhost, false),
                aws::uri_encode(&self.queue, false)
            ))
            .basic_auth(&self.user, Some(&self.password))
            .json(&json!({"count": 10, "ackmode": "ack_requeue_false", "encoding": "auto"}))
            .send()
            .and_then(|r| r.error_for_status())
            .map_err(http_error)?
            .json()
            .map_err(http_error)?;
        let out: Vec<SourceMessage> = messages
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|m| {
                let payload = m.get("payload").and_then(Value::as_str).unwrap_or("");
                let bytes = if m.get("payload_encoding").and_then(Value::as_str) == Some("base64") {
                    base64::engine::general_purpose::STANDARD.decode(payload).unwrap_or_default()
                } else {
                    payload.as_bytes().to_vec()
                };
                SourceMessage {
                    payload: StreamPayload::Bytes(bytes),
                    token: m.get("delivery_tag").map(|t| t.to_string()),
                }
            })
            .collect();
        if out.is_empty() {
            pytime::sleep(1.0);
        }
        Ok(out)
    }

    /// The management API removes messages when they are fetched, so a negative
    /// acknowledgement re-publishes the message to the queue (requeue at the tail).
    fn nack(&mut self, message: &SourceMessage) -> Result<(), SemanticaError> {
        let bytes = match &message.payload {
            StreamPayload::Bytes(b) => b.clone(),
            StreamPayload::Text(t) => t.as_bytes().to_vec(),
            StreamPayload::Json(v) => v.to_string().into_bytes(),
        };
        self.client
            .post(format!(
                "{}/exchanges/{}/amq.default/publish",
                self.api,
                aws::uri_encode(&self.vhost, false)
            ))
            .basic_auth(&self.user, Some(&self.password))
            .json(&json!({
                "properties": {},
                "routing_key": self.queue,
                "payload": base64::engine::general_purpose::STANDARD.encode(bytes),
                "payload_encoding": "base64",
            }))
            .send()
            .and_then(|r| r.error_for_status())
            .map_err(http_error)?;
        Ok(())
    }
}

/// AWS Kinesis through its JSON API (SigV4).
pub struct KinesisHttpSource {
    region: String,
    credentials: Option<AwsCredentials>,
    shard_iterator: Option<String>,
    pending_iterator: Option<Option<String>>,
    client: reqwest::blocking::Client,
}

impl KinesisHttpSource {
    fn call(&self, target: &str, body: &Value) -> Result<Value, SemanticaError> {
        let host = format!("kinesis.{}.amazonaws.com", self.region);
        let payload = body.to_string();
        let extra = vec![
            ("content-type".to_string(), "application/x-amz-json-1.1".to_string()),
            ("x-amz-target".to_string(), format!("Kinesis_20131202.{target}")),
        ];
        let mut request = self.client.post(format!("https://{host}/"));
        for (k, v) in &extra {
            request = request.header(k.as_str(), v.as_str());
        }
        if let Some(c) = &self.credentials {
            for (k, v) in aws::sign_v4(c, &self.region, "kinesis", "POST", &host, "/", &[], &extra, payload.as_bytes(), &aws::amz_now()) {
                if k.starts_with("content-type") || k.starts_with("x-amz-target") {
                    continue;
                }
                request = request.header(k.as_str(), v.as_str());
            }
        }
        request
            .body(payload)
            .send()
            .and_then(|r| r.error_for_status())
            .map_err(http_error)?
            .json()
            .map_err(http_error)
    }

    /// `boto3.client("kinesis", region_name=region)` + `get_shard_iterator(...)`.
    pub fn connect(stream_name: &str, region: &str, shard_id: &str, iterator_type: &str) -> Result<KinesisHttpSource, SemanticaError> {
        let mut source = KinesisHttpSource {
            region: region.to_string(),
            credentials: AwsCredentials::resolve(None, None),
            shard_iterator: None,
            pending_iterator: None,
            client: http_client()?,
        };
        let response = source.call(
            "GetShardIterator",
            &json!({"StreamName": stream_name, "ShardId": shard_id, "ShardIteratorType": iterator_type}),
        )?;
        source.shard_iterator = Some(
            response
                .get("ShardIterator")
                .map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()))
                .ok_or_else(|| SemanticaError::Key("ShardIterator".to_string()))?,
        );
        Ok(source)
    }
}

impl StreamSource for KinesisHttpSource {
    fn receive(&mut self) -> Result<Vec<SourceMessage>, SemanticaError> {
        let iterator = self
            .shard_iterator
            .clone()
            .ok_or_else(|| SemanticaError::Key("ShardIterator".to_string()))?;
        let response = self.call("GetRecords", &json!({"ShardIterator": iterator}))?;
        let records = response
            .get("Records")
            .and_then(Value::as_array)
            .cloned()
            .ok_or_else(|| SemanticaError::Key("Records".to_string()))?;
        self.pending_iterator = Some(
            response
                .get("NextShardIterator")
                .and_then(Value::as_str)
                .map(str::to_string),
        );
        Ok(records
            .into_iter()
            .map(|r| SourceMessage {
                payload: StreamPayload::Bytes(
                    base64::engine::general_purpose::STANDARD
                        .decode(r.get("Data").and_then(Value::as_str).unwrap_or(""))
                        .unwrap_or_default(),
                ),
                token: r.get("SequenceNumber").and_then(Value::as_str).map(str::to_string),
            })
            .collect())
    }

    fn commit(&mut self) -> Result<(), SemanticaError> {
        match self.pending_iterator.take() {
            Some(Some(next)) => {
                self.shard_iterator = Some(next);
                Ok(())
            }
            _ => Err(SemanticaError::Key("NextShardIterator".to_string())),
        }
    }
}

/// Apache Pulsar through the WebSocket consumer API (`ws://<host>:8080/ws/v2/consumer/...`).
pub struct PulsarWebSocketSource {
    stream: TcpStream,
    reader: BufReader<TcpStream>,
}

fn ws_frame(opcode: u8, payload: &[u8]) -> Vec<u8> {
    let mut frame = vec![0x80 | opcode];
    let len = payload.len();
    if len < 126 {
        frame.push(0x80 | len as u8);
    } else if len < 65536 {
        frame.push(0x80 | 126);
        frame.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        frame.push(0x80 | 127);
        frame.extend_from_slice(&(len as u64).to_be_bytes());
    }
    let mask: [u8; 4] = uuid::Uuid::new_v4().as_bytes()[..4].try_into().unwrap_or([1, 2, 3, 4]);
    frame.extend_from_slice(&mask);
    frame.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
    frame
}

impl PulsarWebSocketSource {
    /// Connects a shared subscription to `topic` (`pulsar://host:6650` maps to
    /// `ws://host:8080`; `websocket_url` overrides it).
    pub fn connect(service_url: &str, topic: &str, subscription: &str, websocket_url: Option<&str>) -> Result<PulsarWebSocketSource, SemanticaError> {
        let base = match websocket_url {
            Some(u) => u.to_string(),
            None => {
                let parsed = url::Url::parse(service_url).map_err(|e| SemanticaError::Value(e.to_string()))?;
                format!("ws://{}:8080", parsed.host_str().unwrap_or("localhost"))
            }
        };
        let parsed = url::Url::parse(&base).map_err(|e| SemanticaError::Value(e.to_string()))?;
        if parsed.scheme() != "ws" {
            return Err(SemanticaError::Value(format!(
                "Only ws:// Pulsar WebSocket endpoints are supported, got {base}"
            )));
        }
        let host = parsed.host_str().unwrap_or("localhost").to_string();
        let port = parsed.port().unwrap_or(8080);
        let full_topic = if topic.contains("://") {
            topic.replace("://", "/")
        } else {
            format!("persistent/public/default/{topic}")
        };
        let path = format!("/ws/v2/consumer/{full_topic}/{subscription}?subscriptionType=Shared");
        let stream = TcpStream::connect((host.as_str(), port)).map_err(SemanticaError::from)?;
        stream
            .set_read_timeout(Some(std::time::Duration::from_millis(1000)))
            .map_err(SemanticaError::from)?;
        let key = base64::engine::general_purpose::STANDARD.encode(uuid::Uuid::new_v4().as_bytes());
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        );
        let mut writer = stream.try_clone().map_err(SemanticaError::from)?;
        writer.write_all(request.as_bytes()).map_err(SemanticaError::from)?;
        let mut reader = BufReader::new(stream.try_clone().map_err(SemanticaError::from)?);
        let mut status = String::new();
        reader.read_line(&mut status).map_err(SemanticaError::from)?;
        if !status.contains(" 101 ") {
            return Err(SemanticaError::Runtime(format!("WebSocket upgrade failed: {}", status.trim())));
        }
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).map_err(SemanticaError::from)?;
            if line.trim().is_empty() {
                break;
            }
        }
        Ok(PulsarWebSocketSource { stream, reader })
    }

    fn read_frame(&mut self) -> Result<(u8, Vec<u8>), SemanticaError> {
        let mut head = [0u8; 2];
        self.reader.read_exact(&mut head).map_err(|e| {
            if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) {
                SemanticaError::Runtime("Pulsar receive timeout".to_string())
            } else {
                SemanticaError::from(e)
            }
        })?;
        let opcode = head[0] & 0x0f;
        let mut len = u64::from(head[1] & 0x7f);
        if len == 126 {
            let mut b = [0u8; 2];
            self.reader.read_exact(&mut b).map_err(SemanticaError::from)?;
            len = u64::from(u16::from_be_bytes(b));
        } else if len == 127 {
            let mut b = [0u8; 8];
            self.reader.read_exact(&mut b).map_err(SemanticaError::from)?;
            len = u64::from_be_bytes(b);
        }
        let mut payload = vec![0u8; len as usize];
        self.reader.read_exact(&mut payload).map_err(SemanticaError::from)?;
        Ok((opcode, payload))
    }

    fn send_json(&mut self, value: &Value) -> Result<(), SemanticaError> {
        self.stream
            .write_all(&ws_frame(0x1, value.to_string().as_bytes()))
            .map_err(SemanticaError::from)
    }
}

impl StreamSource for PulsarWebSocketSource {
    fn receive(&mut self) -> Result<Vec<SourceMessage>, SemanticaError> {
        loop {
            let (opcode, payload) = self.read_frame()?;
            match opcode {
                0x1 | 0x2 => {
                    let message: Value = serde_json::from_slice(&payload).map_err(|e| SemanticaError::JsonDecode(e.to_string()))?;
                    let data = base64::engine::general_purpose::STANDARD
                        .decode(message.get("payload").and_then(Value::as_str).unwrap_or(""))
                        .unwrap_or_default();
                    return Ok(vec![SourceMessage {
                        payload: StreamPayload::Bytes(data),
                        token: message.get("messageId").and_then(Value::as_str).map(str::to_string),
                    }]);
                }
                0x9 => {
                    let _ = self.stream.write_all(&ws_frame(0xA, &payload));
                }
                0x8 => return Err(SemanticaError::Runtime("Pulsar WebSocket closed".to_string())),
                _ => {}
            }
        }
    }

    fn ack(&mut self, message: &SourceMessage) -> Result<(), SemanticaError> {
        match &message.token {
            Some(id) => self.send_json(&json!({"messageId": id})),
            None => Ok(()),
        }
    }

    fn nack(&mut self, message: &SourceMessage) -> Result<(), SemanticaError> {
        match &message.token {
            Some(id) => self.send_json(&json!({"type": "negativeAcknowledge", "messageId": id})),
            None => Ok(()),
        }
    }

    fn close(&mut self) {
        let _ = self.stream.write_all(&ws_frame(0x8, &[]));
    }
}

/// Replacement for the `KafkaProcessor` class (`patch("...stream_ingestor.KafkaProcessor")`).
pub static KAFKA_PROCESSOR: Hook<Box<dyn Fn(&str, &[String], &StreamOptions) -> Result<StreamProcessor, SemanticaError>>> =
    Hook::new("semantica.ingest.stream_ingestor.KafkaProcessor");
/// Replacement for `StreamIngestor.ingest_kafka`.
pub static INGEST_KAFKA: Hook<Box<dyn Fn(&str, &[String]) -> Result<Arc<StreamProcessor>, SemanticaError>>> =
    Hook::new("semantica.ingest.stream_ingestor.StreamIngestor.ingest_kafka");
/// Replacement for `StreamIngestor.ingest_rabbitmq`.
pub static INGEST_RABBITMQ: Hook<Box<dyn Fn(&str, &str) -> Result<Arc<StreamProcessor>, SemanticaError>>> =
    Hook::new("semantica.ingest.stream_ingestor.StreamIngestor.ingest_rabbitmq");

type SourceResult = Result<Box<dyn StreamSource>, SemanticaError>;

/// Replacement for `kafka.KafkaConsumer(topic, bootstrap_servers=..., **consumer_config)`.
pub static KAFKA_CONSUMER: Hook<Box<dyn Fn(&str, &[String], &Kwargs) -> SourceResult>> =
    Hook::new("kafka.KafkaConsumer");
/// Replacement for `pika.BlockingConnection(pika.URLParameters(url))` plus the
/// `queue_declare(queue, durable)` the processor performs (`(connection_url, queue, durable)`).
pub static PIKA_CONNECTION: Hook<Box<dyn Fn(&str, &str, bool) -> SourceResult>> =
    Hook::new("pika.BlockingConnection");
/// Replacement for `boto3.client("kinesis", region_name=region)` plus
/// `get_shard_iterator(...)` (`(stream_name, region, shard_id, shard_iterator_type)`).
pub static KINESIS_CLIENT: Hook<Box<dyn Fn(&str, &str, &str, &str) -> SourceResult>> =
    Hook::new("boto3.client");
/// Replacement for `pulsar.Client(service_url).subscribe(topic, subscription_name, Shared)`
/// (`(service_url, topic, subscription_name)`).
pub static PULSAR_CLIENT: Hook<Box<dyn Fn(&str, &str, &str) -> SourceResult>> =
    Hook::new("pulsar.Client");

fn source_config(pairs: &[(&str, Value)]) -> Map<String, Value> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
}

/// `KafkaProcessor`.
#[derive(Debug, Clone, Copy)]
pub struct KafkaProcessor;

impl KafkaProcessor {
    /// `KafkaProcessor(topic, bootstrap_servers, **options)`: subscribes through the Kafka
    /// REST Proxy (`consumer_config["rest_proxy_url"]`, default `http://<first server host>:8082`).
    #[allow(clippy::new_ret_no_self)]
    pub fn new(topic: &str, bootstrap_servers: &[String], options: StreamOptions) -> Result<StreamProcessor, SemanticaError> {
        if let Some(f) = KAFKA_PROCESSOR.get() {
            return f(topic, bootstrap_servers, &options);
        }
        let consumer_config = options
            .extra
            .get("consumer_config")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let proxy = kw::get_str(&consumer_config, "rest_proxy_url").unwrap_or_else(|| {
            let host = bootstrap_servers
                .first()
                .map(|s| s.split(':').next().unwrap_or("localhost").to_string())
                .unwrap_or_else(|| "localhost".to_string());
            format!("http://{host}:8082")
        });
        let group = kw::get_str(&consumer_config, "group_id").unwrap_or_else(|| "semantica".to_string());
        let source: Box<dyn StreamSource> = match KAFKA_CONSUMER.get() {
            Some(f) => f(topic, bootstrap_servers, &consumer_config)?,
            None => Box::new(KafkaRestSource::connect(&proxy, &group, topic, &consumer_config)?),
        };
        Ok(StreamProcessor::with_source(
            StreamKind::Kafka,
            source_config(&[
                ("type", json!("kafka")),
                ("topic", json!(topic)),
                ("bootstrap_servers", json!(bootstrap_servers)),
            ]),
            options,
            Some(source),
        ))
    }
}

/// `RabbitMQProcessor`.
#[derive(Debug, Clone, Copy)]
pub struct RabbitMQProcessor;

impl RabbitMQProcessor {
    /// `RabbitMQProcessor(queue, connection_url, **options)` (declares the queue, `durable`
    /// default true).
    #[allow(clippy::new_ret_no_self)]
    pub fn new(queue: &str, connection_url: &str, options: StreamOptions) -> Result<StreamProcessor, SemanticaError> {
        let durable = options.extra.get("durable").map(kw::truthy).unwrap_or(true);
        let source: Box<dyn StreamSource> = match PIKA_CONNECTION.get() {
            Some(f) => f(connection_url, queue, durable)?,
            None => Box::new(RabbitMqHttpSource::connect(
                connection_url,
                queue,
                durable,
                kw::get_str(&options.extra, "management_url").as_deref(),
            )?),
        };
        Ok(StreamProcessor::with_source(
            StreamKind::RabbitMQ,
            source_config(&[
                ("type", json!("rabbitmq")),
                ("queue", json!(queue)),
                ("connection_url", json!(connection_url)),
            ]),
            options,
            Some(source),
        ))
    }
}

/// `KinesisProcessor`.
#[derive(Debug, Clone, Copy)]
pub struct KinesisProcessor;

impl KinesisProcessor {
    /// `KinesisProcessor(stream_name, region, **options)` (`shard_id` default `"0"`,
    /// `shard_iterator_type` default `"LATEST"`).
    #[allow(clippy::new_ret_no_self)]
    pub fn new(stream_name: &str, region: &str, options: StreamOptions) -> Result<StreamProcessor, SemanticaError> {
        let shard_id = kw::get_str(&options.extra, "shard_id").unwrap_or_else(|| "0".to_string());
        let iterator_type = kw::get_str(&options.extra, "shard_iterator_type").unwrap_or_else(|| "LATEST".to_string());
        let source: Box<dyn StreamSource> = match KINESIS_CLIENT.get() {
            Some(f) => f(stream_name, region, &shard_id, &iterator_type)?,
            None => Box::new(KinesisHttpSource::connect(stream_name, region, &shard_id, &iterator_type)?),
        };
        Ok(StreamProcessor::with_source(
            StreamKind::Kinesis,
            source_config(&[
                ("type", json!("kinesis")),
                ("stream_name", json!(stream_name)),
                ("region", json!(region)),
            ]),
            options,
            Some(source),
        ))
    }
}

/// `PulsarProcessor`.
#[derive(Debug, Clone, Copy)]
pub struct PulsarProcessor;

impl PulsarProcessor {
    /// `PulsarProcessor(topic, service_url, **options)` (`subscription_name` default
    /// `"semantica-consumer"`, shared subscription).
    #[allow(clippy::new_ret_no_self)]
    pub fn new(topic: &str, service_url: &str, options: StreamOptions) -> Result<StreamProcessor, SemanticaError> {
        let subscription = kw::get_str(&options.extra, "subscription_name").unwrap_or_else(|| "semantica-consumer".to_string());
        let source: Box<dyn StreamSource> = match PULSAR_CLIENT.get() {
            Some(f) => f(service_url, topic, &subscription)?,
            None => Box::new(PulsarWebSocketSource::connect(
                service_url,
                topic,
                &subscription,
                kw::get_str(&options.extra, "websocket_url").as_deref(),
            )?),
        };
        Ok(StreamProcessor::with_source(
            StreamKind::Pulsar,
            source_config(&[
                ("type", json!("pulsar")),
                ("topic", json!(topic)),
                ("service_url", json!(service_url)),
            ]),
            options,
            Some(source),
        ))
    }
}

/// `StreamMonitor`: health and metrics of stream processors.
pub struct StreamMonitor {
    /// Configuration.
    pub config: Kwargs,
    /// Monitored processors.
    pub processors: IndexMap<String, Arc<StreamProcessor>>,
    /// Per-processor metrics.
    pub metrics: IndexMap<String, Map<String, Value>>,
    /// `alert_thresholds` (`error_rate`, `max_lag`).
    pub alert_thresholds: Map<String, Value>,
    logger: Logger,
}

impl std::fmt::Debug for StreamMonitor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamMonitor")
            .field("processors", &self.processors.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl StreamMonitor {
    /// `StreamMonitor(**config)`.
    pub fn new(config: Kwargs) -> StreamMonitor {
        let alert_thresholds = config
            .get("alert_thresholds")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_else(|| kw::obj([("error_rate", json!(0.1)), ("max_lag", json!(1000))]));
        StreamMonitor {
            config,
            processors: IndexMap::new(),
            metrics: IndexMap::new(),
            alert_thresholds,
            logger: get_logger("stream_monitor"),
        }
    }

    /// `monitor_processor(processor, name=None)`.
    pub fn monitor_processor(&mut self, processor: Arc<StreamProcessor>, name: Option<&str>) {
        let name = match name {
            Some(n) => n.to_string(),
            None => processor
                .source_config
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string(),
        };
        self.processors.insert(name.clone(), processor);
        self.metrics.insert(
            name.clone(),
            kw::obj([
                ("last_check", Value::String(PyDateTime::now(false).isoformat('T'))),
                ("processed", json!(0)),
                ("errors", json!(0)),
                ("healthy", json!(true)),
            ]),
        );
        self.logger.info(format!("Started monitoring processor: {name}"));
    }

    /// `get_metrics(processor_name=None)`.
    pub fn get_metrics(&self, processor_name: Option<&str>) -> Map<String, Value> {
        let merged = |name: &str, p: &StreamProcessor| {
            let mut m = self.metrics.get(name).cloned().unwrap_or_default();
            for (k, v) in p.get_stats() {
                m.insert(k, v);
            }
            Value::Object(m)
        };
        match processor_name.filter(|n| !n.is_empty()) {
            Some(name) => match self.processors.get(name) {
                Some(p) => kw::obj([(name, merged(name, p))]),
                None => Map::new(),
            },
            None => self
                .processors
                .iter()
                .map(|(name, p)| (name.clone(), merged(name, p)))
                .collect(),
        }
    }

    /// `check_health()` (`KeyError` when `alert_thresholds` lacks `error_rate`).
    pub fn check_health(&self) -> Result<Map<String, Value>, SemanticaError> {
        let mut processors = Map::new();
        let mut unhealthy = 0usize;
        for (name, p) in &self.processors {
            let threshold = self
                .alert_thresholds
                .get("error_rate")
                .ok_or_else(|| SemanticaError::Key("error_rate".to_string()))?
                .as_f64()
                .unwrap_or(f64::NAN);
            let stats = p.get_stats();
            let processed = stats.get("processed").and_then(Value::as_u64).unwrap_or(0);
            let errors = stats.get("errors").and_then(Value::as_u64).unwrap_or(0);
            let total = processed + errors;
            let error_rate = if total > 0 { errors as f64 / total as f64 } else { 0.0 };
            let healthy = p.running() && error_rate < threshold;
            if !healthy {
                unhealthy += 1;
            }
            let rate_value = if total > 0 { json!(error_rate) } else { json!(0) };
            processors.insert(
                name.clone(),
                json!({
                    "healthy": healthy,
                    "running": p.running(),
                    "error_rate": rate_value,
                    "processed": processed,
                    "errors": errors,
                }),
            );
        }
        let overall = if unhealthy == 0 {
            "healthy"
        } else if unhealthy == self.processors.len() {
            "unhealthy"
        } else {
            "degraded"
        };
        Ok(kw::obj([
            ("overall", json!(overall)),
            ("processors", Value::Object(processors)),
            ("timestamp", Value::String(PyDateTime::now(false).isoformat('T'))),
        ]))
    }
}

/// `StreamIngestor`.
pub struct StreamIngestor {
    /// Merged configuration.
    pub config: Kwargs,
    /// Processors by name (`kafka_<topic>`, ...).
    pub processors: IndexMap<String, Arc<StreamProcessor>>,
    /// The monitor.
    pub monitor: StreamMonitor,
    logger: Logger,
}

impl std::fmt::Debug for StreamIngestor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamIngestor")
            .field("processors", &self.processors.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl Default for StreamIngestor {
    fn default() -> Self {
        StreamIngestor::new(Kwargs::new())
    }
}

impl StreamIngestor {
    /// `StreamIngestor(config=None, **kwargs)`.
    pub fn new(config: Kwargs) -> StreamIngestor {
        let ingestor = StreamIngestor {
            monitor: StreamMonitor::new(config.clone()),
            config,
            processors: IndexMap::new(),
            logger: get_logger("stream_ingestor"),
        };
        ingestor.logger.debug("Stream ingestor initialized");
        ingestor
    }

    fn register(&mut self, name: String, processor: StreamProcessor) -> Arc<StreamProcessor> {
        let processor = Arc::new(processor);
        self.processors.insert(name.clone(), processor.clone());
        self.monitor.monitor_processor(processor.clone(), Some(&name));
        processor
    }

    /// `ingest_kafka(topic, bootstrap_servers, **options)`.
    pub fn ingest_kafka(&mut self, topic: &str, bootstrap_servers: &[String], options: StreamOptions) -> Result<Arc<StreamProcessor>, SemanticaError> {
        if let Some(f) = INGEST_KAFKA.get() {
            return f(topic, bootstrap_servers);
        }
        let processor = KafkaProcessor::new(topic, bootstrap_servers, options)?;
        Ok(self.register(format!("kafka_{topic}"), processor))
    }

    /// `ingest_pulsar(topic, service_url, **options)`.
    pub fn ingest_pulsar(&mut self, topic: &str, service_url: &str, options: StreamOptions) -> Result<Arc<StreamProcessor>, SemanticaError> {
        let processor = PulsarProcessor::new(topic, service_url, options)?;
        Ok(self.register(format!("pulsar_{topic}"), processor))
    }

    /// `ingest_rabbitmq(queue, connection_url, **options)`.
    pub fn ingest_rabbitmq(&mut self, queue: &str, connection_url: &str, options: StreamOptions) -> Result<Arc<StreamProcessor>, SemanticaError> {
        if let Some(f) = INGEST_RABBITMQ.get() {
            return f(queue, connection_url);
        }
        let processor = RabbitMQProcessor::new(queue, connection_url, options)?;
        Ok(self.register(format!("rabbitmq_{queue}"), processor))
    }

    /// `ingest_kinesis(stream_name, region, **options)`.
    pub fn ingest_kinesis(&mut self, stream_name: &str, region: &str, options: StreamOptions) -> Result<Arc<StreamProcessor>, SemanticaError> {
        let processor = KinesisProcessor::new(stream_name, region, options)?;
        Ok(self.register(format!("kinesis_{stream_name}"), processor))
    }

    /// `start_streaming(processors=None)`.
    pub fn start_streaming(&self, processors: Option<&[Arc<StreamProcessor>]>) -> Result<(), SemanticaError> {
        let list: Vec<Arc<StreamProcessor>> = match processors {
            Some(p) => p.to_vec(),
            None => self.processors.values().cloned().collect(),
        };
        for p in list {
            p.start_consuming();
            self.logger.info(format!(
                "Started stream processor: {}",
                p.source_config
                    .get("type")
                    .map(sem_utils::pycompat::fmt::py_str)
                    .unwrap_or_else(|| "None".to_string())
            ));
        }
        Ok(())
    }

    /// `stop_streaming()`.
    pub fn stop_streaming(&self) {
        for (name, p) in &self.processors {
            p.stop_consuming();
            self.logger.info(format!("Stopped stream processor: {name}"));
        }
    }
}
