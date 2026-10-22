//! Characterisation tests for `semantica/ingest/stream_ingestor.py` (expected values from the
//! original run in the oracle environment with `kafka.KafkaConsumer`, `pika`, `boto3.client`
//! and `pulsar.Client` patched; here the matching client hooks install in-memory sources).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use sem_ingest::stream_ingestor::{
    KAFKA_CONSUMER, KINESIS_CLIENT, KafkaProcessor, KinesisProcessor, PIKA_CONNECTION, PULSAR_CLIENT,
    PulsarProcessor, RabbitMQProcessor, SourceMessage, StreamIngestor, StreamMessage, StreamMonitor,
    StreamOptions, StreamPayload, StreamProcessor, StreamSource,
};
use sem_ingest::support::kw::Kwargs;
use sem_ingest::support::pytime;
use sem_utils::SemanticaError;
use serde_json::{Map, Value, json};

fn kwargs(v: Value) -> Kwargs {
    v.as_object().cloned().unwrap_or_default()
}

fn config(v: Value) -> Map<String, Value> {
    kwargs(v)
}

fn msg(payload: StreamPayload, token: &str) -> SourceMessage {
    SourceMessage {
        payload,
        token: Some(token.to_string()),
    }
}

type Handled = Arc<Mutex<Vec<Value>>>;

fn record_handler(p: &StreamProcessor) -> Handled {
    let handled: Handled = Arc::new(Mutex::new(Vec::new()));
    let log = handled.clone();
    p.set_message_handler(move |x| {
        log.lock()
            .unwrap()
            .push(x.map(|m| m["content"].clone()).unwrap_or(Value::Null))
    });
    handled
}

#[test]
fn stream_message_and_generic_processor() {
    let m = StreamMessage::new(json!({"a": 1}));
    assert_eq!(m.content, json!({"a": 1}));
    assert!(m.metadata.is_empty());
    assert_eq!((m.source.as_str(), m.partition, m.offset), ("", None, None));

    let p = StreamProcessor::new(config(json!({"type": "custom"})), StreamOptions::default());
    let r = p
        .process_message(&StreamPayload::Bytes(br#"{"a": 1, "metadata": {"m": 2}}"#.to_vec()))
        .unwrap()
        .unwrap();
    assert_eq!(r["content"], json!({"a": 1, "metadata": {"m": 2}}));
    assert_eq!(r["metadata"], json!({"m": 2}));
    let mut keys: Vec<&String> = r.keys().collect();
    keys.sort();
    assert_eq!(keys, vec!["content", "metadata", "processed_at"]);
    let content = |payload: StreamPayload| p.process_message(&payload).unwrap().unwrap()["content"].clone();
    assert_eq!(content(StreamPayload::Text(r#"{"b": 2}"#.to_string())), json!({"b": 2}));
    assert_eq!(content(StreamPayload::Json(json!({"c": 3}))), json!({"c": 3}));
    assert_eq!(
        p.process_message(&StreamPayload::Json(json!({"c": 3}))).unwrap().unwrap()["metadata"],
        json!({})
    );
    assert_eq!(content(StreamPayload::Json(json!(5))), json!({"raw": "5"}));
    assert_eq!(content(StreamPayload::Json(json!(["x"]))), json!({"raw": "['x']"}));
    let bad = [
        (StreamPayload::Text("not json".to_string()), "Failed to process message: Expecting value: line 1 column 1 (char 0)"),
        (
            StreamPayload::Bytes(b"\xff".to_vec()),
            "Failed to process message: 'utf-8' codec can't decode byte 0xff in position 0: invalid start byte",
        ),
        (StreamPayload::Text("[1, 2]".to_string()), "Failed to process message: 'list' object has no attribute 'get'"),
        (StreamPayload::Text("5".to_string()), "Failed to process message: 'int' object has no attribute 'get'"),
    ];
    for (payload, expected) in bad {
        let err = p.process_message(&payload).unwrap_err();
        assert_eq!(err.type_name(), "ProcessingError");
        assert_eq!(err.to_string(), expected);
    }
    assert_eq!(Value::Object(p.get_stats()), json!({"processed": 6, "errors": 4, "running": false}));

    let options = StreamOptions {
        transform: Some(Arc::new(|c: Value| {
            let mut m = c.as_object().cloned().unwrap_or_default();
            m.insert("t".to_string(), json!(true));
            Value::Object(m)
        })),
        validate: Some(Arc::new(|c: &Value| c.get("ok").is_some())),
        extra: Kwargs::new(),
    };
    let p2 = StreamProcessor::new(config(json!({"type": "custom"})), options);
    assert_eq!(
        p2.process_message(&StreamPayload::Json(json!({"ok": 1}))).unwrap().unwrap()["content"],
        json!({"ok": 1, "t": true})
    );
    let err = p2.process_message(&StreamPayload::Json(json!({"no": 1}))).unwrap_err();
    assert_eq!(err.to_string(), "Failed to process message: Message validation failed");
    let seen: Arc<Mutex<Vec<(StreamPayload, String)>>> = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    p2.set_error_handler(move |m, e| log.lock().unwrap().push((m.clone(), e.to_string())));
    assert_eq!(p2.process_message(&StreamPayload::Json(json!({"no": 1}))).unwrap(), None);
    assert_eq!(
        *seen.lock().unwrap(),
        vec![(StreamPayload::Json(json!({"no": 1})), "Message validation failed".to_string())]
    );
    assert_eq!(Value::Object(p2.get_stats()), json!({"processed": 1, "errors": 2, "running": false}));

    let p3 = StreamProcessor::new(config(json!({"type": "custom"})), StreamOptions::default());
    p3.start_consuming();
    assert!(p3.running());
    assert_eq!(p3.get_stats()["running"], json!(true));
    p3.start_consuming();
    p3.stop_consuming();
    assert!(!p3.running());
}

/// A source answering scripted batches; `stop_after` clears the processor's `running` flag
/// once the script is exhausted (as the original tests' `poll` side effect does).
struct ScriptedSource {
    batches: VecDeque<Result<Vec<SourceMessage>, SemanticaError>>,
    running: Arc<Mutex<Option<Arc<AtomicBool>>>>,
    stop_on_last: bool,
    acks: Arc<Mutex<Vec<String>>>,
    nacks: Arc<Mutex<Vec<String>>>,
    closed: Arc<AtomicBool>,
    commits: Arc<Mutex<usize>>,
}

impl ScriptedSource {
    fn stop(&self) {
        if let Some(flag) = self.running.lock().unwrap().as_ref() {
            flag.store(false, Ordering::SeqCst);
        }
    }
}

impl StreamSource for ScriptedSource {
    fn receive(&mut self) -> Result<Vec<SourceMessage>, SemanticaError> {
        let next = self.batches.pop_front().unwrap_or_else(|| Ok(Vec::new()));
        if self.batches.is_empty() && self.stop_on_last {
            self.stop();
        }
        next
    }

    fn ack(&mut self, message: &SourceMessage) -> Result<(), SemanticaError> {
        self.acks.lock().unwrap().push(message.token.clone().unwrap_or_default());
        Ok(())
    }

    fn nack(&mut self, message: &SourceMessage) -> Result<(), SemanticaError> {
        self.nacks.lock().unwrap().push(message.token.clone().unwrap_or_default());
        Ok(())
    }

    fn commit(&mut self) -> Result<(), SemanticaError> {
        *self.commits.lock().unwrap() += 1;
        Ok(())
    }

    fn close(&mut self) {
        self.closed.store(true, Ordering::SeqCst);
    }
}

#[derive(Clone, Default)]
struct Probe {
    running: Arc<Mutex<Option<Arc<AtomicBool>>>>,
    acks: Arc<Mutex<Vec<String>>>,
    nacks: Arc<Mutex<Vec<String>>>,
    closed: Arc<AtomicBool>,
    commits: Arc<Mutex<usize>>,
}

impl Probe {
    fn source(&self, batches: Vec<Result<Vec<SourceMessage>, SemanticaError>>, stop_on_last: bool) -> ScriptedSource {
        ScriptedSource {
            batches: batches.into(),
            running: self.running.clone(),
            stop_on_last,
            acks: self.acks.clone(),
            nacks: self.nacks.clone(),
            closed: self.closed.clone(),
            commits: self.commits.clone(),
        }
    }

    fn attach(&self, p: &StreamProcessor) {
        *self.running.lock().unwrap() = Some(p.running_handle());
    }
}

#[test]
fn kafka_processor_consumes_batches() {
    let probe = Probe::default();
    let seen_config: Arc<Mutex<Option<(String, Vec<String>, Kwargs)>>> = Arc::new(Mutex::new(None));
    let (probe_c, seen_c) = (probe.clone(), seen_config.clone());
    let _consumer = KAFKA_CONSUMER.set(Box::new(move |topic: &str, servers: &[String], cfg: &Kwargs| {
        *seen_c.lock().unwrap() = Some((topic.to_string(), servers.to_vec(), cfg.clone()));
        let batches = vec![
            Ok(vec![
                msg(StreamPayload::Json(json!({"a": 1})), "0"),
                msg(StreamPayload::Json(json!([1])), "1"),
                msg(StreamPayload::Json(json!({"b": 2})), "2"),
            ]),
            Ok(vec![msg(StreamPayload::Json(json!({"c": 3})), "3")]),
        ];
        Ok(Box::new(probe_c.source(batches, true)) as Box<dyn StreamSource>)
    }));
    let kp = KafkaProcessor::new(
        "events",
        &["localhost:9092".to_string()],
        StreamOptions::from_kwargs(&kwargs(json!({"consumer_config": {"group_id": "g"}}))),
    )
    .unwrap();
    assert_eq!(
        Value::Object(kp.source_config.clone()),
        json!({"type": "kafka", "topic": "events", "bootstrap_servers": ["localhost:9092"]})
    );
    let (topic, servers, cfg) = seen_config.lock().unwrap().clone().unwrap();
    assert_eq!((topic.as_str(), servers), ("events", vec!["localhost:9092".to_string()]));
    assert_eq!(cfg["group_id"], "g");
    let handled = record_handler(&kp);
    probe.attach(&kp);
    kp.set_running(true);
    kp._consume_loop();
    assert_eq!(
        *handled.lock().unwrap(),
        vec![json!({"a": 1}), json!({"raw": "[1]"}), json!({"b": 2}), json!({"c": 3})]
    );
    assert_eq!(Value::Object(kp.get_stats()), json!({"processed": 4, "errors": 0, "running": false}));
}

#[test]
fn rabbitmq_processor_acks_and_requeues() {
    let probe = Probe::default();
    let declared: Arc<Mutex<Option<(String, String, bool)>>> = Arc::new(Mutex::new(None));
    let (probe_c, declared_c) = (probe.clone(), declared.clone());
    let _pika = PIKA_CONNECTION.set(Box::new(move |url: &str, queue: &str, durable: bool| {
        *declared_c.lock().unwrap() = Some((url.to_string(), queue.to_string(), durable));
        let batches = vec![Ok(vec![
            msg(StreamPayload::Bytes(br#"{"x": 1}"#.to_vec()), "7"),
            msg(StreamPayload::Bytes(b"oops".to_vec()), "8"),
        ])];
        Ok(Box::new(probe_c.source(batches, true)) as Box<dyn StreamSource>)
    }));
    let rp = RabbitMQProcessor::new(
        "jobs",
        "amqp://guest:guest@localhost:5672/",
        StreamOptions::from_kwargs(&kwargs(json!({"durable": false}))),
    )
    .unwrap();
    assert_eq!(
        Value::Object(rp.source_config.clone()),
        json!({"type": "rabbitmq", "queue": "jobs", "connection_url": "amqp://guest:guest@localhost:5672/"})
    );
    assert_eq!(
        declared.lock().unwrap().clone().unwrap(),
        ("amqp://guest:guest@localhost:5672/".to_string(), "jobs".to_string(), false)
    );
    let handled = record_handler(&rp);
    probe.attach(&rp);
    rp.set_running(true);
    rp._consume_loop();
    assert_eq!(*handled.lock().unwrap(), vec![json!({"x": 1})]);
    assert_eq!(*probe.acks.lock().unwrap(), vec!["7"]);
    assert_eq!(*probe.nacks.lock().unwrap(), vec!["8"]);
    assert!(probe.closed.load(Ordering::SeqCst));
}

#[test]
fn kinesis_processor_retries_failed_batches() {
    let probe = Probe::default();
    let opened: Arc<Mutex<Option<Vec<String>>>> = Arc::new(Mutex::new(None));
    let (probe_c, opened_c) = (probe.clone(), opened.clone());
    let _client = KINESIS_CLIENT.set(Box::new(move |stream: &str, region: &str, shard: &str, kind: &str| {
        *opened_c.lock().unwrap() = Some(vec![stream.into(), region.into(), shard.into(), kind.into()]);
        let batches = vec![
            Ok(vec![
                msg(StreamPayload::Bytes(br#"{"k": 1}"#.to_vec()), "1"),
                msg(StreamPayload::Bytes(b"bad".to_vec()), "2"),
            ]),
            Ok(vec![msg(StreamPayload::Bytes(br#"{"k": 2}"#.to_vec()), "3")]),
        ];
        Ok(Box::new(probe_c.source(batches, false)) as Box<dyn StreamSource>)
    }));
    let kp = KinesisProcessor::new(
        "stream",
        "us-west-2",
        StreamOptions::from_kwargs(&kwargs(json!({"shard_id": "shardId-1"}))),
    )
    .unwrap();
    assert_eq!(
        Value::Object(kp.source_config.clone()),
        json!({"type": "kinesis", "stream_name": "stream", "region": "us-west-2"})
    );
    assert_eq!(
        opened.lock().unwrap().clone().unwrap(),
        vec!["stream", "us-west-2", "shardId-1", "LATEST"]
    );
    let handled = record_handler(&kp);
    let sleeps = Arc::new(Mutex::new(Vec::new()));
    let (sleep_log, flag) = (sleeps.clone(), kp.running_handle());
    let _sleep = pytime::SLEEP.set(Box::new(move |s| {
        let mut log = sleep_log.lock().unwrap();
        log.push(s);
        if log.len() >= 2 {
            flag.store(false, Ordering::SeqCst);
        }
    }));
    kp.set_running(true);
    kp._consume_loop();
    assert_eq!(*handled.lock().unwrap(), vec![json!({"k": 1}), json!({"k": 2})]);
    assert_eq!(*sleeps.lock().unwrap(), vec![5.0, 1.0]);
    // Only the second, fully processed batch advanced the shard iterator.
    assert_eq!(*probe.commits.lock().unwrap(), 1);
}

#[test]
fn pulsar_processor_acks_nacks_and_ignores_timeouts() {
    let probe = Probe::default();
    let subscribed: Arc<Mutex<Option<Vec<String>>>> = Arc::new(Mutex::new(None));
    let (probe_c, subscribed_c) = (probe.clone(), subscribed.clone());
    let _client = PULSAR_CLIENT.set(Box::new(move |url: &str, topic: &str, subscription: &str| {
        *subscribed_c.lock().unwrap() = Some(vec![url.into(), topic.into(), subscription.into()]);
        let batches = vec![
            Ok(vec![msg(StreamPayload::Bytes(br#"{"p": 1}"#.to_vec()), "a")]),
            Ok(vec![msg(StreamPayload::Bytes(b"nope".to_vec()), "b")]),
            Err(SemanticaError::Runtime("Pulsar error: TimeOut".to_string())),
        ];
        Ok(Box::new(probe_c.source(batches, true)) as Box<dyn StreamSource>)
    }));
    let pp = PulsarProcessor::new(
        "persistent://public/default/t",
        "pulsar://localhost:6650",
        StreamOptions::default(),
    )
    .unwrap();
    assert_eq!(
        Value::Object(pp.source_config.clone()),
        json!({"type": "pulsar", "topic": "persistent://public/default/t", "service_url": "pulsar://localhost:6650"})
    );
    assert_eq!(
        subscribed.lock().unwrap().clone().unwrap(),
        vec!["pulsar://localhost:6650", "persistent://public/default/t", "semantica-consumer"]
    );
    let handled = record_handler(&pp);
    probe.attach(&pp);
    pp.set_running(true);
    pp._consume_loop();
    assert_eq!(*handled.lock().unwrap(), vec![json!({"p": 1})]);
    assert_eq!(probe.acks.lock().unwrap().len(), 1);
    assert_eq!(probe.nacks.lock().unwrap().len(), 1);
}

#[test]
fn stream_monitor_metrics_and_health() {
    let mut mon = StreamMonitor::new(Kwargs::new());
    let a = Arc::new(StreamProcessor::new(config(json!({"type": "a"})), StreamOptions::default()));
    let b = Arc::new(StreamProcessor::new(Map::new(), StreamOptions::default()));
    mon.monitor_processor(a.clone(), None);
    mon.monitor_processor(b.clone(), None);
    mon.monitor_processor(a.clone(), Some("named"));
    assert_eq!(mon.processors.keys().collect::<Vec<_>>(), vec!["a", "unknown", "named"]);
    for metrics in mon.metrics.values() {
        let mut keys: Vec<&String> = metrics.keys().collect();
        keys.sort();
        assert_eq!(keys, vec!["errors", "healthy", "last_check", "processed"]);
    }
    assert_eq!(Value::Object(mon.alert_thresholds.clone()), json!({"error_rate": 0.1, "max_lag": 1000}));
    a.process_message(&StreamPayload::Json(json!({"x": 1}))).unwrap();
    let _ = a.process_message(&StreamPayload::Text("bad".to_string()));
    let strip = |m: &Value| {
        let mut m = m.as_object().cloned().unwrap();
        m.remove("last_check");
        Value::Object(m)
    };
    let one = mon.get_metrics(Some("a"));
    assert_eq!(strip(&one["a"]), json!({"processed": 1, "errors": 1, "healthy": true, "running": false}));
    assert!(mon.get_metrics(Some("zzz")).is_empty());
    let all = mon.get_metrics(None);
    let all: Map<String, Value> = all.iter().map(|(k, v)| (k.clone(), strip(v))).collect();
    assert_eq!(
        Value::Object(all),
        json!({
            "a": {"processed": 1, "errors": 1, "healthy": true, "running": false},
            "unknown": {"processed": 0, "errors": 0, "healthy": true, "running": false},
            "named": {"processed": 1, "errors": 1, "healthy": true, "running": false},
        })
    );
    a.set_running(true);
    let mut health = mon.check_health().unwrap();
    health.remove("timestamp");
    assert_eq!(
        Value::Object(health),
        json!({"overall": "unhealthy", "processors": {
            "a": {"healthy": false, "running": true, "error_rate": 0.5, "processed": 1, "errors": 1},
            "unknown": {"healthy": false, "running": false, "error_rate": 0, "processed": 0, "errors": 0},
            "named": {"healthy": false, "running": true, "error_rate": 0.5, "processed": 1, "errors": 1},
        }})
    );
    let mut mon2 = StreamMonitor::new(kwargs(json!({"alert_thresholds": {"max_lag": 1}})));
    mon2.monitor_processor(a.clone(), None);
    let err = mon2.check_health().unwrap_err();
    assert_eq!(err.type_name(), "KeyError");
    assert_eq!(err.to_string(), "'error_rate'");
    let mut empty = StreamMonitor::new(Kwargs::new()).check_health().unwrap();
    empty.remove("timestamp");
    assert_eq!(Value::Object(empty), json!({"overall": "healthy", "processors": {}}));
    a.set_running(false);
}

#[test]
fn stream_ingestor_registers_and_starts_processors() {
    let probe = Probe::default();
    let p1 = probe.clone();
    let _kafka = KAFKA_CONSUMER.set(Box::new(move |_t: &str, _s: &[String], _c: &Kwargs| {
        Ok(Box::new(p1.source(vec![], false)) as Box<dyn StreamSource>)
    }));
    let p2 = probe.clone();
    let _pulsar = PULSAR_CLIENT.set(Box::new(move |_u: &str, _t: &str, _s: &str| {
        Ok(Box::new(p2.source(vec![], false)) as Box<dyn StreamSource>)
    }));
    let p3 = probe.clone();
    let _kinesis = KINESIS_CLIENT.set(Box::new(move |_s: &str, _r: &str, _i: &str, _k: &str| {
        Ok(Box::new(p3.source(vec![], false)) as Box<dyn StreamSource>)
    }));
    let p4 = probe.clone();
    let _pika = PIKA_CONNECTION.set(Box::new(move |_u: &str, _q: &str, _d: bool| {
        Ok(Box::new(p4.source(vec![], false)) as Box<dyn StreamSource>)
    }));
    let mut ing = StreamIngestor::new(Kwargs::new());
    let k = ing.ingest_kafka("t1", &["h:1".to_string()], StreamOptions::default()).unwrap();
    ing.ingest_pulsar("t2", "pulsar://h:6650", StreamOptions::default()).unwrap();
    ing.ingest_kinesis("s1", "eu-west-1", StreamOptions::default()).unwrap();
    ing.ingest_rabbitmq("q1", "amqp://h", StreamOptions::default()).unwrap();
    assert_eq!(
        ing.processors.keys().collect::<Vec<_>>(),
        vec!["kafka_t1", "pulsar_t2", "kinesis_s1", "rabbitmq_q1"]
    );
    assert_eq!(
        ing.monitor.processors.keys().collect::<Vec<_>>(),
        vec!["kafka_t1", "pulsar_t2", "kinesis_s1", "rabbitmq_q1"]
    );
    ing.start_streaming(Some(std::slice::from_ref(&k))).unwrap();
    assert!(k.running());
    ing.start_streaming(None).unwrap();
    assert!(ing.processors.values().all(|p| p.running()));
    ing.stop_streaming();
    assert!(ing.processors.values().all(|p| !p.running()));
}
