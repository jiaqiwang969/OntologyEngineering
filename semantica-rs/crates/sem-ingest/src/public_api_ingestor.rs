//! Public, no-auth API ingestion (`semantica/ingest/public_api_ingestor.py`), built on
//! [`RESTIngestor`]: no-auth validation, endpoint-level public detection, polite rate
//! limiting and JSON/CSV/XML response normalisation, plus a catalog of example endpoints.

use std::ops::{Deref, DerefMut};

use indexmap::IndexMap;
use once_cell::sync::Lazy;
use sem_utils::pycompat::fmt::{py_repr, type_name};
use sem_utils::pycompat::time::PyDateTime;
use sem_utils::{SemanticaError, get_logger};
use serde_json::{Map, Value, json};

use crate::api_ingestor::{APIData, RESTIngestor, body_from_value};
use crate::config::{processing_error, validation_error};
use crate::ssrf::{GuardOptions, request_with_ssrf_guard};
use crate::support::etree;
use crate::support::http::{self, RequestOptions, Response, is_request_exception};
use crate::support::kw::{self, Kwargs};
use crate::support::optional;
use crate::support::pycsv;
use crate::support::pytime;
use crate::support::pyurl;
use crate::support::xmldom::XmlErrorKind;

/// `AUTH_HEADER_NAMES`.
pub const AUTH_HEADER_NAMES: [&str; 9] = [
    "authorization",
    "proxy-authorization",
    "x-api-key",
    "api-key",
    "apikey",
    "ocp-apim-subscription-key",
    "x-rapidapi-key",
    "x-auth-token",
    "x-access-token",
];

/// `AUTH_PARAM_NAMES`.
pub const AUTH_PARAM_NAMES: [&str; 9] = [
    "api_key",
    "apikey",
    "access_token",
    "auth_token",
    "bearer_token",
    "client_secret",
    "token",
    "subscription_key",
    "subscription-key",
];

/// `PublicAPIExample`: a pre-configured public API endpoint definition.
#[derive(Debug, Clone, PartialEq)]
pub struct PublicAPIExample {
    /// `name`.
    pub name: String,
    /// `endpoint`.
    pub endpoint: String,
    /// `description`.
    pub description: String,
    /// `method`.
    pub method: String,
    /// `params`.
    pub params: Map<String, Value>,
    /// `headers`.
    pub headers: IndexMap<String, String>,
    /// `response_format`.
    pub response_format: String,
    /// `record_path`.
    pub record_path: Option<String>,
    /// `tags`.
    pub tags: Vec<String>,
    /// `rate_limit_delay`.
    pub rate_limit_delay: Option<f64>,
}

/// `PublicAPIDetection`: whether an endpoint can be reached without auth.
#[derive(Debug, Clone, PartialEq)]
pub struct PublicAPIDetection {
    /// `endpoint`.
    pub endpoint: String,
    /// `is_public`.
    pub is_public: bool,
    /// `requires_auth`.
    pub requires_auth: bool,
    /// `response_status`.
    pub response_status: Option<u16>,
    /// `reason`.
    pub reason: String,
    /// `metadata`.
    pub metadata: Map<String, Value>,
    /// `checked_at`.
    pub checked_at: PyDateTime,
}

fn example(
    name: &str,
    endpoint: &str,
    description: &str,
    params: Value,
    record_path: Option<&str>,
    tags: &[&str],
    delay: f64,
) -> PublicAPIExample {
    PublicAPIExample {
        name: name.to_string(),
        endpoint: endpoint.to_string(),
        description: description.to_string(),
        method: "GET".to_string(),
        params: params.as_object().cloned().unwrap_or_default(),
        headers: IndexMap::new(),
        response_format: "json".to_string(),
        record_path: record_path.map(str::to_string),
        tags: tags.iter().map(|s| s.to_string()).collect(),
        rate_limit_delay: Some(delay),
    }
}

static EXAMPLES: Lazy<IndexMap<String, PublicAPIExample>> = Lazy::new(|| {
    let list = vec![
        example(
            "jsonplaceholder_posts",
            "https://jsonplaceholder.typicode.com/posts",
            "Fake blog post records for REST API testing.",
            json!({}),
            None,
            &["json", "testing", "placeholder"],
            0.5,
        ),
        example(
            "jsonplaceholder_users",
            "https://jsonplaceholder.typicode.com/users",
            "Fake user records for REST API testing.",
            json!({}),
            None,
            &["json", "testing", "placeholder"],
            0.5,
        ),
        example(
            "jsonplaceholder_todos",
            "https://jsonplaceholder.typicode.com/todos",
            "Fake todo records for REST API testing.",
            json!({}),
            None,
            &["json", "testing", "placeholder"],
            0.5,
        ),
        example(
            "rest_countries_all",
            "https://restcountries.com/v3.1/all",
            "Country reference data from REST Countries.",
            json!({"fields": "name,capital,region,population,cca2,cca3"}),
            None,
            &["json", "countries", "reference"],
            1.0,
        ),
        example(
            "data_gov_datasets",
            "https://catalog.data.gov/api/3/action/package_search",
            "Data.gov catalog package search results.",
            json!({"q": "climate", "rows": 10}),
            Some("result.results"),
            &["json", "government", "datasets"],
            1.0,
        ),
        example(
            "open_meteo_forecast",
            "https://api.open-meteo.com/v1/forecast",
            "Open-Meteo forecast sample for Berlin.",
            json!({"latitude": 52.52, "longitude": 13.41, "current": "temperature_2m,wind_speed_10m"}),
            None,
            &["json", "weather", "forecast"],
            1.0,
        ),
    ];
    list.into_iter().map(|e| (e.name.clone(), e)).collect()
});

static SAMPLE_RESPONSES: Lazy<IndexMap<String, Value>> = Lazy::new(|| {
    let mut m = IndexMap::new();
    m.insert(
        "jsonplaceholder_posts".to_string(),
        json!([{"userId": 1, "id": 1, "title": "sample post", "body": "body text"}]),
    );
    m.insert(
        "jsonplaceholder_users".to_string(),
        json!([{"id": 1, "name": "Leanne Graham", "email": "leanne@example.com"}]),
    );
    m.insert(
        "jsonplaceholder_todos".to_string(),
        json!([{"userId": 1, "id": 1, "title": "sample todo", "completed": false}]),
    );
    m.insert(
        "rest_countries_all".to_string(),
        json!([{
            "name": {"common": "India", "official": "Republic of India"},
            "capital": ["New Delhi"],
            "region": "Asia",
            "population": 1407563842i64,
            "cca2": "IN",
            "cca3": "IND"
        }]),
    );
    m.insert(
        "data_gov_datasets".to_string(),
        json!({
            "success": true,
            "result": {
                "count": 1,
                "results": [{
                    "id": "sample-dataset",
                    "title": "Sample Dataset",
                    "metadata_created": "2026-01-01T00:00:00"
                }]
            }
        }),
    );
    m.insert(
        "open_meteo_forecast".to_string(),
        json!({
            "latitude": 52.52,
            "longitude": 13.41,
            "current": {"temperature_2m": 18.2, "wind_speed_10m": 9.1}
        }),
    );
    m
});

/// `PublicAPIExamples`: catalog of no-auth public API examples.
#[derive(Debug, Clone, Copy, Default)]
pub struct PublicAPIExamples;

impl PublicAPIExamples {
    /// `list_examples(tag=None)` (deep copies).
    pub fn list_examples(tag: Option<&str>) -> Vec<PublicAPIExample> {
        match tag.filter(|t| !t.is_empty()) {
            Some(t) => {
                let lower = t.to_lowercase();
                EXAMPLES
                    .values()
                    .filter(|e| e.tags.contains(&lower))
                    .cloned()
                    .collect()
            }
            None => EXAMPLES.values().cloned().collect(),
        }
    }

    /// `names(tag=None)`.
    pub fn names(tag: Option<&str>) -> Vec<String> {
        Self::list_examples(tag).into_iter().map(|e| e.name).collect()
    }

    /// `endpoints()`.
    pub fn endpoints() -> IndexMap<String, String> {
        EXAMPLES
            .iter()
            .map(|(k, e)| (k.clone(), e.endpoint.clone()))
            .collect()
    }

    /// `get(name)` (hyphens normalised to underscores).
    pub fn get(name: &str) -> Result<PublicAPIExample, SemanticaError> {
        let normalized = name.to_lowercase().replace('-', "_");
        match EXAMPLES.get(&normalized) {
            Some(e) => Ok(e.clone()),
            None => {
                let mut available: Vec<&String> = EXAMPLES.keys().collect();
                available.sort();
                Err(validation_error(format!(
                    "Unknown public API example: {name}. Available examples: {}",
                    available.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                )))
            }
        }
    }

    /// `sample_response(name)`: a small mock payload for an example.
    pub fn sample_response(name: &str) -> Result<Value, SemanticaError> {
        let normalized = name.to_lowercase().replace('-', "_");
        match SAMPLE_RESPONSES.get(&normalized) {
            Some(v) => Ok(v.clone()),
            None => {
                let mut available: Vec<&String> = SAMPLE_RESPONSES.keys().collect();
                available.sort();
                Err(validation_error(format!(
                    "No sample response for public API example: {name}. Available samples: {}",
                    available.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                )))
            }
        }
    }
}

/// The keyword arguments of `ingest_public_api` / `detect_public_api` after `endpoint`.
#[derive(Debug, Clone, PartialEq)]
pub struct PublicApiRequest {
    /// `method`.
    pub method: String,
    /// `headers`.
    pub headers: Option<IndexMap<String, String>>,
    /// `params`.
    pub params: Option<Map<String, Value>>,
    /// `data`.
    pub data: Option<Value>,
    /// `json_data`.
    pub json_data: Option<Value>,
    /// `response_format`.
    pub response_format: String,
    /// `record_path`.
    pub record_path: Option<String>,
    /// `normalize_records`.
    pub normalize_records: bool,
    /// `require_public`.
    pub require_public: bool,
    /// `rate_limit_delay`.
    pub rate_limit_delay: Option<f64>,
    /// `**options`.
    pub options: Map<String, Value>,
}

impl Default for PublicApiRequest {
    fn default() -> Self {
        PublicApiRequest {
            method: "GET".to_string(),
            headers: None,
            params: None,
            data: None,
            json_data: None,
            response_format: "auto".to_string(),
            record_path: None,
            normalize_records: true,
            require_public: true,
            rate_limit_delay: None,
            options: Map::new(),
        }
    }
}

impl PublicApiRequest {
    /// Binds a `**kwargs` call to the named parameters.
    pub fn from_kwargs(kwargs: &Kwargs) -> PublicApiRequest {
        let mut r = PublicApiRequest::default();
        for (k, v) in kwargs {
            match k.as_str() {
                "method" => {
                    if let Some(m) = v.as_str() {
                        r.method = m.to_string();
                    }
                }
                "headers" => {
                    r.headers = v.as_object().map(|m| {
                        m.iter()
                            .map(|(k, v)| (k.clone(), sem_utils::pycompat::fmt::py_str(v)))
                            .collect()
                    })
                }
                "params" => r.params = v.as_object().cloned(),
                "data" => r.data = if v.is_null() { None } else { Some(v.clone()) },
                "json_data" => r.json_data = if v.is_null() { None } else { Some(v.clone()) },
                "response_format" => {
                    if let Some(f) = v.as_str() {
                        r.response_format = f.to_string();
                    }
                }
                "record_path" => r.record_path = v.as_str().map(str::to_string),
                "normalize_records" => r.normalize_records = kw::truthy(v),
                "require_public" => r.require_public = kw::truthy(v),
                "rate_limit_delay" => r.rate_limit_delay = v.as_f64(),
                _ => {
                    r.options.insert(k.clone(), v.clone());
                }
            }
        }
        r
    }
}

/// `PublicAPIIngestor`: public, no-auth API ingestion handler (a [`RESTIngestor`]).
pub struct PublicAPIIngestor {
    /// The underlying REST ingestor (session, config, progress tracker).
    pub rest: RESTIngestor,
    /// Minimum seconds between requests.
    pub rate_limit_delay: f64,
    /// Reject auth headers/params before requests.
    pub validate_no_auth: bool,
    last_request_time: f64,
}

impl std::fmt::Debug for PublicAPIIngestor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PublicAPIIngestor")
            .field("rest", &self.rest)
            .field("rate_limit_delay", &self.rate_limit_delay)
            .field("validate_no_auth", &self.validate_no_auth)
            .finish()
    }
}

impl Deref for PublicAPIIngestor {
    type Target = RESTIngestor;

    fn deref(&self) -> &RESTIngestor {
        &self.rest
    }
}

impl DerefMut for PublicAPIIngestor {
    fn deref_mut(&mut self) -> &mut RESTIngestor {
        &mut self.rest
    }
}

fn float_of(value: &Value) -> Result<f64, SemanticaError> {
    match value {
        Value::Number(n) => Ok(n.as_f64().unwrap_or(0.0)),
        Value::Bool(b) => Ok(f64::from(u8::from(*b))),
        Value::Null => Ok(0.0),
        Value::String(s) => crate::config::py_float(s).ok_or_else(|| {
            SemanticaError::Value(format!("could not convert string to float: {}", py_repr(value)))
        }),
        other => Err(SemanticaError::Type(format!(
            "float() argument must be a string or a real number, not '{}'",
            type_name(other)
        ))),
    }
}

/// The public API ingestor's `_coerce_bool(value, default=True)`.
pub fn _coerce_bool(value: &Value, default: bool) -> Result<bool, SemanticaError> {
    match value {
        Value::Null => Ok(default),
        Value::Bool(b) => Ok(*b),
        Value::String(s) => {
            let normalized = sem_utils::pycompat::fmt::strip(s).to_lowercase();
            match normalized.as_str() {
                "1" | "true" | "yes" | "on" => Ok(true),
                "0" | "false" | "no" | "off" => Ok(false),
                _ => Err(validation_error(format!(
                    "Invalid boolean value for validate_no_auth: {}",
                    py_repr(value)
                ))),
            }
        }
        other => Ok(kw::truthy(other)),
    }
}

/// `_strip_namespace(value)`.
fn strip_namespace(value: &str) -> String {
    if value.starts_with('{')
        && let Some(i) = value.find('}')
    {
        return value[i + 1..].to_string();
    }
    value.to_string()
}

fn element_to_dict(element: &etree::Element) -> Value {
    let children: Vec<Value> = element.children.iter().map(element_to_dict).collect();
    let attributes: Map<String, Value> = element
        .attrib
        .iter()
        .map(|(k, v)| (strip_namespace(k), Value::String(v.clone())))
        .collect();
    json!({
        "tag": strip_namespace(&element.tag),
        "attributes": attributes,
        "text": sem_utils::pycompat::fmt::strip(element.text.as_deref().unwrap_or("")),
        "children": children,
    })
}

enum ParseFailure {
    /// `ValueError` (JSON) / defusedxml or lxml rejections: "Failed to parse ...".
    Format(String),
    /// Anything else (propagates to the generic handler).
    Other(SemanticaError),
}

impl PublicAPIIngestor {
    /// `PublicAPIIngestor(config=None, rate_limit_delay=None, validate_no_auth=None, **kwargs)`.
    pub fn new(
        config: Option<Kwargs>,
        rate_limit_delay: Option<f64>,
        validate_no_auth: Option<Value>,
        kwargs: Kwargs,
    ) -> Result<PublicAPIIngestor, SemanticaError> {
        let mut merged = config.unwrap_or_default();
        for (k, v) in kwargs {
            merged.insert(k, v);
        }
        if let Some(d) = rate_limit_delay {
            merged.insert("rate_limit_delay".to_string(), json!(d));
        }
        if let Some(v) = validate_no_auth {
            merged.insert("validate_no_auth".to_string(), v);
        }
        let mut rest = RESTIngestor::new(merged)?;
        rest.logger = get_logger("public_api_ingestor");
        let delay_value = rest
            .config
            .get("rate_limit_delay")
            .or_else(|| rest.config.get("delay"))
            .cloned()
            .unwrap_or(json!(1.0));
        let delay = if kw::truthy(&delay_value) {
            float_of(&delay_value)?
        } else {
            0.0
        };
        let validate = _coerce_bool(
            rest.config.get("validate_no_auth").unwrap_or(&Value::Bool(true)),
            true,
        )?;
        let ingestor = PublicAPIIngestor {
            rest,
            rate_limit_delay: delay,
            validate_no_auth: validate,
            last_request_time: 0.0,
        };
        ingestor.rest.logger.debug("Public API ingestor initialized");
        Ok(ingestor)
    }

    /// `PublicAPIIngestor(**kwargs)` (as the convenience functions call it).
    pub fn from_kwargs(kwargs: &Kwargs) -> Result<PublicAPIIngestor, SemanticaError> {
        let mut rest = kwargs.clone();
        let config = rest.remove("config").and_then(|v| v.as_object().cloned());
        let delay = rest.remove("rate_limit_delay").and_then(|v| v.as_f64());
        let validate = rest.remove("validate_no_auth");
        PublicAPIIngestor::new(config, delay, validate, rest)
    }

    fn validate_endpoint(&self, endpoint: &str) -> Result<(), SemanticaError> {
        let parsed = pyurl::urlparse(endpoint)?;
        if !(parsed.scheme == "http" || parsed.scheme == "https") || parsed.netloc.is_empty() {
            return Err(validation_error(format!(
                "Public API endpoint must be an absolute HTTP(S) URL: {endpoint}"
            )));
        }
        Ok(())
    }

    /// `_merged_headers(headers=None)`: session headers updated with `headers`.
    pub fn _merged_headers(&self, headers: Option<&IndexMap<String, String>>) -> IndexMap<String, String> {
        let mut base = self.rest.session.headers.to_map();
        if let Some(h) = headers {
            for (k, v) in h {
                base.insert(k.clone(), v.clone());
            }
        }
        base
    }

    /// `_auth_indicators(headers, params, options, endpoint)`.
    pub fn _auth_indicators(
        &self,
        headers: Option<&IndexMap<String, String>>,
        params: Option<&Map<String, Value>>,
        options: Option<&Map<String, Value>>,
        endpoint: Option<&str>,
    ) -> Result<Vec<String>, SemanticaError> {
        let mut indicators = Vec::new();
        for name in self._merged_headers(headers).keys() {
            if AUTH_HEADER_NAMES.contains(&name.to_lowercase().as_str()) {
                indicators.push(format!("header:{name}"));
            }
        }
        if let Some(params) = params {
            for name in params.keys() {
                if AUTH_PARAM_NAMES.contains(&name.to_lowercase().as_str()) {
                    indicators.push(format!("param:{name}"));
                }
            }
        }
        if let Some(options) = options
            && options.get("auth").is_some_and(|a| !a.is_null())
        {
            indicators.push("request:auth".to_string());
        }
        if let Some(endpoint) = endpoint.filter(|e| !e.is_empty()) {
            let query = pyurl::urlparse(endpoint)?.query;
            for name in pyurl::parse_qs(&query, false).keys() {
                if AUTH_PARAM_NAMES.contains(&name.to_lowercase().as_str()) {
                    indicators.push(format!("url_param:{name}"));
                }
            }
        }
        Ok(indicators)
    }

    fn validate_no_auth_request(&self, request: &PublicApiRequest, endpoint: &str) -> Result<(), SemanticaError> {
        if !self.validate_no_auth {
            return Ok(());
        }
        let indicators = self._auth_indicators(
            request.headers.as_ref(),
            request.params.as_ref(),
            Some(&request.options),
            Some(endpoint),
        )?;
        if !indicators.is_empty() {
            return Err(validation_error(format!(
                "Public API ingestion only supports no-auth endpoints. Authentication indicators found: {}. Use RESTIngestor for authenticated APIs.",
                indicators.join(", ")
            )));
        }
        Ok(())
    }

    fn wait_if_needed(&mut self, rate_limit_delay: Option<f64>) {
        let delay = rate_limit_delay.unwrap_or(self.rate_limit_delay);
        let now = pytime::time();
        if delay > 0.0 && self.last_request_time != 0.0 {
            let elapsed = now - self.last_request_time;
            if elapsed < delay {
                pytime::sleep(delay - elapsed);
            }
        }
        self.last_request_time = pytime::time();
    }

    fn classify(response: &Response) -> (bool, bool, String) {
        let status = response.status_code;
        if (200..300).contains(&status) {
            return (true, false, "Endpoint responded successfully without auth.".to_string());
        }
        if status == 401 || status == 403 {
            return (
                false,
                true,
                "Endpoint returned an authentication or authorization status.".to_string(),
            );
        }
        if status == 429 {
            return (false, false, "Endpoint is rate limited; public access is unclear.".to_string());
        }
        (false, false, format!("Endpoint returned status {status}."))
    }

    fn request_options(&self, request: &PublicApiRequest) -> Result<(RequestOptions, f64, Option<f64>), SemanticaError> {
        let mut extra = request.options.clone();
        let timeout = match extra.remove("timeout") {
            Some(v) => v.as_f64().unwrap_or(30.0),
            None => kw::get_f64(&self.rest.config, "timeout", 30.0),
        };
        let rate_limit_delay = extra.remove("rate_limit_delay").and_then(|v| v.as_f64());
        extra.remove("session");
        extra.remove("allow_private_ips");
        let mut options = RequestOptions::from_kwargs(&extra)?;
        options.headers = Some(self._merged_headers(request.headers.as_ref()));
        options.params = match &request.params {
            Some(p) => http::json_to_params(&Value::Object(p.clone()))?,
            None => None,
        };
        options.timeout = Some(timeout);
        Ok((options, timeout, rate_limit_delay))
    }

    /// `detect_public_api(endpoint, method="GET", headers=None, params=None, **options)`.
    pub fn detect_public_api(
        &mut self,
        endpoint: &str,
        request: &PublicApiRequest,
    ) -> Result<PublicAPIDetection, SemanticaError> {
        self.validate_endpoint(endpoint)?;
        let indicators = self._auth_indicators(
            request.headers.as_ref(),
            request.params.as_ref(),
            Some(&request.options),
            Some(endpoint),
        )?;
        if !indicators.is_empty() {
            let mut metadata = Map::new();
            metadata.insert("auth_indicators".to_string(), json!(indicators));
            return Ok(PublicAPIDetection {
                endpoint: endpoint.to_string(),
                is_public: false,
                requires_auth: true,
                response_status: None,
                reason: "Authentication credentials were provided; no-auth access was not tested."
                    .to_string(),
                metadata,
                checked_at: PyDateTime::now(false),
            });
        }
        let (options, _, delay) = self.request_options(request)?;
        self.wait_if_needed(request.rate_limit_delay.or(delay));
        let guard = GuardOptions::allow_private_ips(self.rest.allow_private_ips);
        let response = match request_with_ssrf_guard(
            &request.method,
            endpoint,
            Some(&mut self.rest.session),
            &guard,
            options,
        ) {
            Ok(r) => r,
            Err(e) if is_request_exception(&e) => {
                self.rest
                    .logger
                    .error(format!("Failed to detect public API {endpoint}: {e}"));
                return Err(processing_error(format!("Failed to detect public API: {e}")));
            }
            Err(e) => return Err(e),
        };
        let (is_public, requires_auth, reason) = Self::classify(&response);
        let mut metadata = Map::new();
        metadata.insert("method".to_string(), Value::String(request.method.clone()));
        metadata.insert(
            "content_type".to_string(),
            kw::opt_str(response.headers.get("Content-Type")),
        );
        metadata.insert(
            "www_authenticate".to_string(),
            kw::opt_str(response.headers.get("WWW-Authenticate")),
        );
        Ok(PublicAPIDetection {
            endpoint: endpoint.to_string(),
            is_public,
            requires_auth,
            response_status: Some(response.status_code),
            reason,
            metadata,
            checked_at: PyDateTime::now(false),
        })
    }

    /// `is_public_api(endpoint, **options)`.
    pub fn is_public_api(&mut self, endpoint: &str, request: &PublicApiRequest) -> Result<bool, SemanticaError> {
        Ok(self.detect_public_api(endpoint, request)?.is_public)
    }

    /// `ingest_public_api(endpoint, ...)`.
    pub fn ingest_public_api(
        &mut self,
        endpoint: &str,
        request: &PublicApiRequest,
    ) -> Result<APIData, SemanticaError> {
        self.validate_endpoint(endpoint)?;
        self.validate_no_auth_request(request, endpoint)?;
        let tracking_id = self.rest.progress_tracker.start_tracking(
            Some(endpoint),
            Some("ingest"),
            Some("PublicAPIIngestor"),
            &format!("Requesting public API: {} {endpoint}", request.method),
            None,
        );
        let result = self.ingest_public_api_inner(endpoint, request, &tracking_id);
        match result {
            Ok(data) => Ok(data),
            Err(e) if e.is_validation_error() || e.type_name() == "ProcessingError" => {
                self.rest.progress_tracker.stop_tracking(
                    &tracking_id,
                    Some("failed"),
                    "Public API ingestion failed",
                    None,
                );
                Err(e)
            }
            Err(e) => {
                self.rest
                    .progress_tracker
                    .stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
                self.rest
                    .logger
                    .error(format!("Failed to ingest public API {endpoint}: {e}"));
                Err(processing_error(format!("Failed to ingest public API: {e}")))
            }
        }
    }

    fn ingest_public_api_inner(
        &mut self,
        endpoint: &str,
        request: &PublicApiRequest,
        tracking_id: &str,
    ) -> Result<APIData, SemanticaError> {
        let (mut options, _, delay) = self.request_options(request)?;
        options.data = request.data.as_ref().and_then(body_from_value);
        options.json = request.json_data.clone();
        self.wait_if_needed(request.rate_limit_delay.or(delay));
        let guard = GuardOptions::allow_private_ips(self.rest.allow_private_ips);
        let response = request_with_ssrf_guard(
            &request.method,
            endpoint,
            Some(&mut self.rest.session),
            &guard,
            options,
        )?;
        let (is_public, requires_auth, reason) = Self::classify(&response);
        if request.require_public && requires_auth {
            return Err(validation_error(format!(
                "Endpoint appears to require authentication: {endpoint} ({}). Use RESTIngestor for authenticated APIs.",
                response.status_code
            )));
        }
        response.raise_for_status()?;
        let (parsed, detected_format) = self.parse_response(&response, &request.response_format, endpoint)?;
        let result_data = if request.normalize_records {
            Value::Array(self._to_records(&parsed, request.record_path.as_deref())?)
        } else if let Some(path) = request.record_path.as_deref().filter(|p| !p.is_empty()) {
            self._extract_record_path(&parsed, path)?
        } else {
            parsed.clone()
        };
        let record_count = match &result_data {
            Value::Array(a) => a.len(),
            _ => 1,
        };
        self.rest.progress_tracker.stop_tracking(
            tracking_id,
            Some("completed"),
            &format!("Public API request successful: {}", response.status_code),
            None,
        );
        self.rest.logger.info(format!(
            "Public API request completed: {} {endpoint} - {}",
            request.method, response.status_code
        ));
        let mut metadata = Map::new();
        metadata.insert("method".to_string(), Value::String(request.method.clone()));
        metadata.insert("headers".to_string(), Value::Object(response.headers.to_json()));
        metadata.insert(
            "content_type".to_string(),
            kw::opt_str(response.headers.get("Content-Type")),
        );
        metadata.insert("public_api".to_string(), Value::Bool(is_public));
        metadata.insert("authentication".to_string(), json!("none"));
        metadata.insert("public_detection_reason".to_string(), Value::String(reason));
        metadata.insert("requires_auth".to_string(), Value::Bool(requires_auth));
        metadata.insert("response_format".to_string(), Value::String(detected_format));
        metadata.insert("record_path".to_string(), kw::opt_str(request.record_path.as_deref()));
        metadata.insert("normalized_records".to_string(), Value::Bool(request.normalize_records));
        metadata.insert("record_count".to_string(), json!(record_count));
        metadata.insert("source_data_type".to_string(), json!(type_name(&parsed)));
        Ok(APIData::new(result_data, response.status_code, endpoint, metadata))
    }

    /// `ingest_example(name, **overrides)`.
    pub fn ingest_example(&mut self, name: &str, overrides: &Kwargs) -> Result<APIData, SemanticaError> {
        let example = PublicAPIExamples::get(name)?;
        let mut overrides = overrides.clone();
        let mut params = example.params.clone();
        if let Some(Value::Object(p)) = overrides.remove("params") {
            for (k, v) in p {
                params.insert(k, v);
            }
        }
        let mut headers = example.headers.clone();
        if let Some(Value::Object(h)) = overrides.remove("headers") {
            for (k, v) in h {
                headers.insert(k, sem_utils::pycompat::fmt::py_str(&v));
            }
        }
        let endpoint = overrides
            .remove("endpoint")
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or(example.endpoint.clone());
        let mut request = PublicApiRequest::from_kwargs(&overrides);
        request.method = match overrides.get("method").and_then(Value::as_str) {
            Some(m) => m.to_string(),
            None => example.method.clone(),
        };
        if !overrides.contains_key("response_format") {
            request.response_format = example.response_format.clone();
        }
        if !overrides.contains_key("record_path") {
            request.record_path = example.record_path.clone();
        }
        if !overrides.contains_key("rate_limit_delay") {
            request.rate_limit_delay = example.rate_limit_delay;
        }
        request.headers = if headers.is_empty() { None } else { Some(headers) };
        request.params = if params.is_empty() { None } else { Some(params) };
        let mut result = self.ingest_public_api(&endpoint, &request)?;
        result
            .metadata
            .insert("example_name".to_string(), Value::String(example.name.clone()));
        result.metadata.insert(
            "example_description".to_string(),
            Value::String(example.description.clone()),
        );
        result
            .metadata
            .insert("example_tags".to_string(), json!(example.tags));
        Ok(result)
    }

    /// `ingest_examples(names, **options)`.
    pub fn ingest_examples(&mut self, names: &[&str], options: &Kwargs) -> Result<Vec<APIData>, SemanticaError> {
        names.iter().map(|n| self.ingest_example(n, options)).collect()
    }

    /// `batch_public_apis(endpoints, method="GET", **options)`.
    pub fn batch_public_apis(
        &mut self,
        endpoints: &[&str],
        request: &PublicApiRequest,
    ) -> Result<Vec<APIData>, SemanticaError> {
        let mut results = Vec::new();
        for endpoint in endpoints {
            match self.ingest_public_api(endpoint, request) {
                Ok(d) => results.push(d),
                Err(e) => {
                    self.rest
                        .logger
                        .warning(format!("Failed to fetch public API {endpoint}: {e}"));
                    if kw::get_truthy(&self.rest.config, "fail_fast", false) {
                        return Err(e);
                    }
                }
            }
        }
        Ok(results)
    }

    fn detect_response_format(
        &self,
        response: &Response,
        requested: &str,
        endpoint: &str,
    ) -> Result<String, SemanticaError> {
        let requested = requested.to_lowercase();
        if requested != "auto" {
            if !["json", "csv", "xml", "text"].contains(&requested.as_str()) {
                return Err(validation_error(
                    "response_format must be one of: auto, json, csv, xml, text",
                ));
            }
            return Ok(requested);
        }
        let content_type = response.headers.get("Content-Type").unwrap_or("").to_lowercase();
        let endpoint_lower = endpoint.to_lowercase();
        let text = response.text();
        let text = text.trim_start();
        Ok(if content_type.contains("json") || endpoint_lower.ends_with(".json") {
            "json"
        } else if content_type.contains("csv") || endpoint_lower.ends_with(".csv") {
            "csv"
        } else if content_type.contains("xml") || endpoint_lower.ends_with(".xml") {
            "xml"
        } else if text.starts_with('{') || text.starts_with('[') {
            "json"
        } else if text.starts_with('<') && !content_type.contains("text/html") {
            "xml"
        } else {
            "text"
        }
        .to_string())
    }

    fn parse_response(
        &self,
        response: &Response,
        response_format: &str,
        endpoint: &str,
    ) -> Result<(Value, String), SemanticaError> {
        let detected = self.detect_response_format(response, response_format, endpoint)?;
        let outcome: Result<Value, ParseFailure> = match detected.as_str() {
            "json" => response.json().map_err(|e| {
                if e.is_value_error() {
                    ParseFailure::Format(format!("Failed to parse {} public API response", detected.to_uppercase()))
                } else {
                    ParseFailure::Other(e)
                }
            }),
            "csv" => self._parse_csv(&response.text()).map_err(ParseFailure::Other),
            "xml" => self.parse_xml(&response.text()),
            _ => Ok(Value::String(response.text())),
        };
        match outcome {
            Ok(v) => Ok((v, detected)),
            Err(ParseFailure::Format(message)) => Err(processing_error(message)),
            Err(ParseFailure::Other(e)) => Err(e),
        }
    }

    /// `_parse_csv(csv_text)`: `csv.DictReader` rows.
    pub fn _parse_csv(&self, csv_text: &str) -> Result<Value, SemanticaError> {
        let rows = pycsv::dict_reader(csv_text).map_err(|e| SemanticaError::Other {
            module: "_csv".to_string(),
            type_name: "Error".to_string(),
            message: e.message,
        })?;
        Ok(Value::Array(
            rows.into_iter()
                .map(|row| {
                    Value::Object(
                        row.into_iter()
                            .map(|(k, v)| (k.unwrap_or_else(|| "null".to_string()), v))
                            .collect(),
                    )
                })
                .collect(),
        ))
    }

    fn parse_xml(&self, xml_text: &str) -> Result<Value, ParseFailure> {
        if optional::import_module("defusedxml").is_ok() {
            match etree::defused_fromstring(xml_text) {
                Ok(root) => Ok(element_to_dict(&root)),
                Err(e) if matches!(e.kind, XmlErrorKind::EntitiesForbidden(_) | XmlErrorKind::DtdForbidden) => {
                    Err(ParseFailure::Format("Failed to parse XML public API response".to_string()))
                }
                Err(e) => Err(ParseFailure::Other(etree::parse_error(&e))),
            }
        } else {
            match etree::lxml_safe_fromstring(xml_text) {
                Ok(root) => Ok(element_to_dict(&root)),
                Err(_) => Err(ParseFailure::Format("Failed to parse XML public API response".to_string())),
            }
        }
    }

    /// `_extract_record_path(data, record_path)`.
    pub fn _extract_record_path(&self, data: &Value, record_path: &str) -> Result<Value, SemanticaError> {
        let mut current = data.clone();
        for part in record_path.split('.') {
            current = match current {
                Value::Object(m) => match m.get(part) {
                    Some(v) => v.clone(),
                    None => {
                        return Err(validation_error(format!(
                            "Record path '{record_path}' not found at '{part}'"
                        )));
                    }
                },
                Value::Array(items) => {
                    if !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()) {
                        let index: usize = part.parse().unwrap_or(usize::MAX);
                        match items.get(index) {
                            Some(v) => v.clone(),
                            None => {
                                return Err(validation_error(format!(
                                    "Record path '{record_path}' index out of range: {part}"
                                )));
                            }
                        }
                    } else {
                        let mut values = Vec::new();
                        for item in &items {
                            match item.as_object().and_then(|m| m.get(part)) {
                                Some(Value::Array(a)) => values.extend(a.iter().cloned()),
                                Some(v) => values.push(v.clone()),
                                None => {
                                    return Err(validation_error(format!(
                                        "Record path '{record_path}' not found at '{part}'"
                                    )));
                                }
                            }
                        }
                        Value::Array(values)
                    }
                }
                other => {
                    return Err(validation_error(format!(
                        "Record path '{record_path}' cannot traverse {}",
                        type_name(&other)
                    )));
                }
            };
        }
        Ok(current)
    }

    /// `_to_records(data, record_path=None)`.
    pub fn _to_records(&self, data: &Value, record_path: Option<&str>) -> Result<Vec<Value>, SemanticaError> {
        let selected = match record_path.filter(|p| !p.is_empty()) {
            Some(p) => self._extract_record_path(data, p)?,
            None => data.clone(),
        };
        let selected: Vec<Value> = match selected {
            Value::Object(m) => {
                let mut picked = None;
                for key in ["items", "data", "results", "records"] {
                    if let Some(Value::Array(a)) = m.get(key) {
                        picked = Some(a.clone());
                        break;
                    }
                }
                picked.unwrap_or_else(|| vec![Value::Object(m)])
            }
            Value::Array(a) => a,
            other => vec![other],
        };
        Ok(selected
            .into_iter()
            .map(|item| match item {
                Value::Object(_) => item,
                other => json!({"value": other}),
            })
            .collect())
    }
}
