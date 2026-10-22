//! REST API ingestion (`semantica/ingest/api_ingestor.py`): a generic REST client with
//! retries, pagination and batch requests. Every request goes through
//! [`crate::ssrf::request_with_ssrf_guard`].

use std::sync::Arc;

use indexmap::IndexMap;
use sem_utils::logging::Logger;
use sem_utils::progress_tracker::ProgressTracker;
use sem_utils::pycompat::time::PyDateTime;
use sem_utils::{SemanticaError, get_logger, get_progress_tracker};
use serde_json::{Map, Value};

use crate::config::processing_error;
use crate::ssrf::{GuardOptions, parse_bool, request_with_ssrf_guard};
use crate::support::http::{
    self, Body, HttpAdapter, RequestOptions, Response, Retry, Session, is_request_exception,
};
use crate::support::kw::{self, Kwargs};

/// `APIData`: REST API data representation.
#[derive(Debug, Clone, PartialEq)]
pub struct APIData {
    /// Response data (parsed JSON, or the raw text).
    pub data: Value,
    /// HTTP status code.
    pub response_status: u16,
    /// Endpoint URL.
    pub endpoint: String,
    /// Additional metadata.
    pub metadata: Map<String, Value>,
    /// Ingestion time.
    pub ingested_at: PyDateTime,
}

impl APIData {
    /// `APIData(data, response_status, endpoint, metadata={})`.
    pub fn new(data: Value, response_status: u16, endpoint: &str, metadata: Map<String, Value>) -> APIData {
        APIData {
            data,
            response_status,
            endpoint: endpoint.to_string(),
            metadata,
            ingested_at: PyDateTime::now(false),
        }
    }
}

/// The keyword arguments of `RESTIngestor.ingest_endpoint` after `endpoint`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EndpointRequest {
    /// `method` (default `"GET"`).
    pub method: Option<String>,
    /// `headers`.
    pub headers: Option<IndexMap<String, String>>,
    /// `params`.
    pub params: Option<Map<String, Value>>,
    /// `data` (a form dict or a raw string).
    pub data: Option<Value>,
    /// `json_data`.
    pub json_data: Option<Value>,
    /// `**options` forwarded to the request.
    pub options: Map<String, Value>,
}

impl EndpointRequest {
    /// Splits a `**kwargs` mapping into the named parameters and the pass-through options.
    pub fn from_kwargs(kwargs: &Kwargs) -> Result<EndpointRequest, SemanticaError> {
        let mut request = EndpointRequest::default();
        for (k, v) in kwargs {
            match k.as_str() {
                "method" => request.method = kw::get_str(kwargs, "method"),
                "headers" => {
                    request.headers = match v {
                        Value::Object(m) => Some(
                            m.iter()
                                .map(|(k, v)| (k.clone(), sem_utils::pycompat::fmt::py_str(v)))
                                .collect(),
                        ),
                        _ => None,
                    }
                }
                "params" => request.params = v.as_object().cloned(),
                "data" => {
                    request.data = if v.is_null() { None } else { Some(v.clone()) }
                }
                "json_data" => {
                    request.json_data = if v.is_null() { None } else { Some(v.clone()) }
                }
                _ => {
                    request.options.insert(k.clone(), v.clone());
                }
            }
        }
        Ok(request)
    }

    /// The method, `GET` by default.
    pub fn method(&self) -> String {
        self.method.clone().unwrap_or_else(|| "GET".to_string())
    }
}

/// `RESTIngestor`: generic REST API ingestion handler.
pub struct RESTIngestor {
    /// Merged configuration.
    pub config: Kwargs,
    /// `allow_private_ips` (parsed with [`parse_bool`]).
    pub allow_private_ips: bool,
    /// The HTTP session (retry policy mounted, default headers applied).
    pub session: Session,
    /// The progress tracker.
    pub progress_tracker: Arc<ProgressTracker>,
    pub(crate) logger: Logger,
}

impl std::fmt::Debug for RESTIngestor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RESTIngestor")
            .field("config", &self.config)
            .field("allow_private_ips", &self.allow_private_ips)
            .finish()
    }
}

/// Converts a JSON value into `data=` for the request.
pub(crate) fn body_from_value(value: &Value) -> Option<Body> {
    match value {
        Value::Null => None,
        Value::String(s) => Some(Body::Raw(s.as_bytes().to_vec())),
        Value::Object(m) => Some(Body::Form(
            m.iter()
                .map(|(k, v)| (k.clone(), sem_utils::pycompat::fmt::py_str(v)))
                .collect(),
        )),
        other => Some(Body::Raw(other.to_string().into_bytes())),
    }
}

impl RESTIngestor {
    /// `RESTIngestor(config=None, **kwargs)` (the two are merged by the caller with
    /// [`kw::merged`]).
    pub fn new(config: Kwargs) -> Result<RESTIngestor, SemanticaError> {
        let logger = get_logger("api_ingestor");
        let allow_private_ips = parse_bool(
            config.get("allow_private_ips").unwrap_or(&Value::Null),
            false,
        )?;
        let mut session = Session::new();
        let retry = Retry {
            total: Some(kw::get_i64(&config, "max_retries").unwrap_or(3)),
            backoff_factor: kw::get_f64(&config, "backoff_factor", 1.0),
            status_forcelist: vec![429, 500, 502, 503, 504],
        };
        session.mount("http://", HttpAdapter::with_retries(retry.clone()));
        session.mount("https://", HttpAdapter::with_retries(retry));
        if let Some(Value::Object(headers)) = config.get("headers")
            && !headers.is_empty()
        {
            for (k, v) in headers {
                session
                    .headers
                    .insert(k.clone(), sem_utils::pycompat::fmt::py_str(v));
            }
        }
        let ingestor = RESTIngestor {
            config,
            allow_private_ips,
            session,
            progress_tracker: get_progress_tracker(),
            logger,
        };
        ingestor.logger.debug(format!(
            "REST API ingestor initialized (allow_private_ips={})",
            if ingestor.allow_private_ips { "True" } else { "False" }
        ));
        Ok(ingestor)
    }

    fn timeout(&self) -> f64 {
        kw::get_f64(&self.config, "timeout", 30.0)
    }

    /// `ingest_endpoint(endpoint, method="GET", headers=None, params=None, data=None,
    /// json_data=None, **options)`.
    pub fn ingest_endpoint(
        &mut self,
        endpoint: &str,
        request: &EndpointRequest,
    ) -> Result<APIData, SemanticaError> {
        let method = request.method();
        let tracking_id = self.progress_tracker.start_tracking(
            Some(endpoint),
            Some("ingest"),
            Some("RESTIngestor"),
            &format!("Requesting: {method} {endpoint}"),
            None,
        );

        let mut headers: IndexMap<String, String> = self.session.headers.to_map();
        if let Some(extra) = &request.headers {
            for (k, v) in extra {
                // `CaseInsensitiveDict.update`: replace case-insensitively.
                if let Some(existing) = headers.keys().find(|h| h.eq_ignore_ascii_case(k)).cloned() {
                    headers.shift_remove(&existing);
                }
                headers.insert(k.clone(), v.clone());
            }
        }
        let mut options = RequestOptions::from_kwargs(&request.options)?;
        options.headers = Some(headers);
        options.params = match &request.params {
            Some(p) => http::json_to_params(&Value::Object(p.clone()))?,
            None => None,
        };
        options.data = request.data.as_ref().and_then(body_from_value);
        options.json = request.json_data.clone();
        options.timeout = Some(self.timeout());

        let guard = GuardOptions::allow_private_ips(self.allow_private_ips);
        let result: Result<Response, SemanticaError> =
            request_with_ssrf_guard(&method, endpoint, Some(&mut self.session), &guard, options)
                .and_then(|response| {
                    response.raise_for_status()?;
                    Ok(response)
                });
        match result {
            Ok(response) => {
                let data = match response.json() {
                    Ok(v) => v,
                    Err(e) if e.is_value_error() => Value::String(response.text()),
                    Err(e) => return Err(e),
                };
                self.progress_tracker.stop_tracking(
                    &tracking_id,
                    Some("completed"),
                    &format!("Request successful: {}", response.status_code),
                    None,
                );
                self.logger.info(format!(
                    "API request completed: {method} {endpoint} - {}",
                    response.status_code
                ));
                let mut metadata = Map::new();
                metadata.insert("method".to_string(), Value::String(method.clone()));
                metadata.insert("headers".to_string(), Value::Object(response.headers.to_json()));
                metadata.insert(
                    "content_type".to_string(),
                    kw::opt_str(response.headers.get("Content-Type")),
                );
                Ok(APIData::new(data, response.status_code, endpoint, metadata))
            }
            Err(e) if e.is_validation_error() => {
                self.progress_tracker.stop_tracking(
                    &tracking_id,
                    Some("failed"),
                    &format!("Invalid endpoint URL: {endpoint}"),
                    None,
                );
                Err(e)
            }
            Err(e) if is_request_exception(&e) => {
                self.progress_tracker
                    .stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
                self.logger
                    .error(format!("Failed to ingest endpoint {endpoint}: {e}"));
                Err(processing_error(format!("Failed to ingest endpoint: {e}")))
            }
            Err(e) => Err(e),
        }
    }

    /// `paginated_fetch(endpoint, page_size=100, page_param="page", size_param="size",
    /// limit=None, **options)`.
    pub fn paginated_fetch(
        &mut self,
        endpoint: &str,
        page_size: i64,
        page_param: &str,
        size_param: &str,
        limit: Option<i64>,
        request: &EndpointRequest,
    ) -> Result<Vec<APIData>, SemanticaError> {
        let tracking_id = self.progress_tracker.start_tracking(
            Some(endpoint),
            Some("ingest"),
            Some("RESTIngestor"),
            &format!("Fetching paginated data from: {endpoint}"),
            None,
        );
        let result = (|| -> Result<(Vec<APIData>, usize), SemanticaError> {
            let mut all_pages = Vec::new();
            let mut page: i64 = 1;
            let mut total_fetched: usize = 0;
            loop {
                let mut params = request.params.clone().unwrap_or_default();
                params.insert(page_param.to_string(), Value::from(page));
                params.insert(size_param.to_string(), Value::from(page_size));
                let mut page_request = request.clone();
                page_request.params = Some(params);
                let page_data = self.ingest_endpoint(endpoint, &page_request)?;

                let items: Vec<Value> = match &page_data.data {
                    Value::Array(items) => items.clone(),
                    Value::Object(m) => {
                        let pick = |k: &str| -> Option<Vec<Value>> {
                            let v = m.get(k)?;
                            if kw::truthy(v) {
                                Some(match v {
                                    Value::Array(a) => a.clone(),
                                    other => vec![other.clone()],
                                })
                            } else {
                                None
                            }
                        };
                        pick("items")
                            .or_else(|| pick("data"))
                            .or_else(|| pick("results"))
                            .unwrap_or_else(|| vec![page_data.data.clone()])
                    }
                    _ => Vec::new(),
                };
                if items.is_empty() {
                    break;
                }
                total_fetched += items.len();
                let data = page_data.data.clone();
                all_pages.push(page_data);
                if let Some(limit) = limit
                    && limit != 0
                    && total_fetched as i64 >= limit
                {
                    break;
                }
                if let Value::Object(m) = &data {
                    let has_more = m.get("has_more").is_some_and(kw::truthy)
                        || m.get("next").is_some_and(|v| !v.is_null());
                    if !has_more {
                        break;
                    }
                }
                page += 1;
            }
            Ok((all_pages, total_fetched))
        })();
        match result {
            Ok((pages, total)) => {
                self.progress_tracker.stop_tracking(
                    &tracking_id,
                    Some("completed"),
                    &format!("Fetched {} pages, {total} items", pages.len()),
                    None,
                );
                self.logger.info(format!(
                    "Paginated fetch completed: {} page(s), {total} item(s)",
                    pages.len()
                ));
                Ok(pages)
            }
            Err(e) => {
                self.progress_tracker
                    .stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
                self.logger
                    .error(format!("Failed to fetch paginated data: {e}"));
                Err(processing_error(format!("Failed to fetch paginated data: {e}")))
            }
        }
    }

    /// `batch_request(endpoints, method="GET", **options)`.
    pub fn batch_request(
        &mut self,
        endpoints: &[&str],
        method: &str,
        request: &EndpointRequest,
    ) -> Result<Vec<APIData>, SemanticaError> {
        let tracking_id = self.progress_tracker.start_tracking(
            Some("batch"),
            Some("ingest"),
            Some("RESTIngestor"),
            &format!("Batch request: {} endpoints", endpoints.len()),
            None,
        );
        let mut results = Vec::new();
        for endpoint in endpoints {
            let mut req = request.clone();
            req.method = Some(method.to_string());
            match self.ingest_endpoint(endpoint, &req) {
                Ok(d) => results.push(d),
                Err(e) => {
                    self.logger.warning(format!("Failed to fetch {endpoint}: {e}"));
                    if kw::get_truthy(&self.config, "fail_fast", false) {
                        self.progress_tracker
                            .stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
                        self.logger
                            .error(format!("Failed to execute batch request: {e}"));
                        return Err(processing_error(format!(
                            "Failed to execute batch request: {e}"
                        )));
                    }
                }
            }
        }
        self.progress_tracker.stop_tracking(
            &tracking_id,
            Some("completed"),
            &format!(
                "Batch request completed: {}/{} successful",
                results.len(),
                endpoints.len()
            ),
            None,
        );
        self.logger.info(format!(
            "Batch request completed: {}/{} successful",
            results.len(),
            endpoints.len()
        ));
        Ok(results)
    }
}
