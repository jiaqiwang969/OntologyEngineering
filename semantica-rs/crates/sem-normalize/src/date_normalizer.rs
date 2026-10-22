//! Date and Time Normalization Module (`semantica/normalize/date_normalizer.py`).
//!
//! Date/time normalization to ISO 8601 (or a date or `strftime` format), time zone conversion
//! (`zoneinfo`), relative dates ("yesterday", "3 days ago", "2 weeks from now") and temporal
//! expressions ("from X to Y"). Parsing is a port of `dateutil.parser.parse`.

use std::cell::RefCell;
use std::sync::Arc;

use chrono::{Duration, NaiveDateTime};
use indexmap::IndexMap;
use once_cell::sync::Lazy;
use regex::Regex;
use sem_utils::logging::Logger;
use sem_utils::progress_tracker::ProgressTracker;
use sem_utils::pycompat::fmt::type_name;
use sem_utils::{SemanticaError, ValidationError, get_logger, get_progress_tracker};
use serde_json::{Map, Value, json};

pub use crate::datetime::{DateTime, TzInfo};
use crate::dateutil;
use crate::pystr::{self, PY_SPACE};
pub use crate::zoneinfo::{ZoneInfo, reset_tzpath, tzpath};

/// `HAS_DATEUTIL`: the dateutil parser port is always available.
pub const HAS_DATEUTIL: bool = true;

thread_local! {
    static FIXED_NOW: RefCell<Option<NaiveDateTime>> = const { RefCell::new(None) };
}

/// `datetime.now()` as this module sees it: the local time, or the value installed with
/// [`patch_now`] on this thread.
pub fn now() -> NaiveDateTime {
    FIXED_NOW
        .with(|f| *f.borrow())
        .unwrap_or_else(|| chrono::Local::now().naive_local())
}

/// Restores the previous clock when dropped (see [`patch_now`]).
#[derive(Debug)]
pub struct NowGuard {
    previous: Option<NaiveDateTime>,
}

impl Drop for NowGuard {
    fn drop(&mut self) {
        FIXED_NOW.with(|f| *f.borrow_mut() = self.previous);
    }
}

/// Fix `datetime.now()` for this module on the current thread until the guard is dropped:
/// the Rust form of the original tests' `patch("semantica.normalize.date_normalizer.datetime",
/// FixedDateTime)`. The fixed time also serves as the parser's default date.
pub fn patch_now(fixed: NaiveDateTime) -> NowGuard {
    let previous = FIXED_NOW.with(|f| f.borrow_mut().replace(fixed));
    NowGuard { previous }
}

/// The input of `normalize_date` / `normalize_time`: a string, a `datetime`, or any other
/// value (Python's `Any`).
#[derive(Debug, Clone, PartialEq)]
pub enum DateInput {
    /// `str`.
    Str(String),
    /// `datetime`.
    DateTime(DateTime),
    /// Any other value.
    Other(Value),
}

impl From<&str> for DateInput {
    fn from(value: &str) -> Self {
        DateInput::Str(value.to_string())
    }
}

impl From<String> for DateInput {
    fn from(value: String) -> Self {
        DateInput::Str(value)
    }
}

impl From<DateTime> for DateInput {
    fn from(value: DateTime) -> Self {
        DateInput::DateTime(value)
    }
}

impl From<NaiveDateTime> for DateInput {
    fn from(value: NaiveDateTime) -> Self {
        DateInput::DateTime(DateTime::naive(value))
    }
}

impl From<&Value> for DateInput {
    fn from(value: &Value) -> Self {
        match value {
            Value::String(s) => DateInput::Str(s.clone()),
            other => DateInput::Other(other.clone()),
        }
    }
}

impl From<Value> for DateInput {
    fn from(value: Value) -> Self {
        DateInput::from(&value)
    }
}

impl DateInput {
    fn truthy(&self) -> bool {
        match self {
            DateInput::Str(s) => !s.is_empty(),
            DateInput::DateTime(_) => true,
            DateInput::Other(v) => sem_utils::pycompat::fmt::truthy(v),
        }
    }
}

/// Date and time normalization coordinator (`DateNormalizer(config=None, **kwargs)`).
pub struct DateNormalizer {
    /// `config` (with the keyword arguments merged in).
    pub config: Map<String, Value>,
    /// `timezone_normalizer`.
    pub timezone_normalizer: TimeZoneNormalizer,
    /// `relative_date_processor`.
    pub relative_date_processor: RelativeDateProcessor,
    /// `temporal_parser`.
    pub temporal_parser: TemporalExpressionParser,
    /// `progress_tracker` (`get_progress_tracker()`, force-enabled as in the original).
    pub progress_tracker: Arc<ProgressTracker>,
    logger: Logger,
}

impl std::fmt::Debug for DateNormalizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DateNormalizer")
            .field("config", &self.config)
            .finish()
    }
}

impl Default for DateNormalizer {
    fn default() -> Self {
        DateNormalizer::new(Map::new())
    }
}

impl DateNormalizer {
    /// `DateNormalizer(config)`.
    pub fn new(config: Map<String, Value>) -> DateNormalizer {
        let logger = get_logger("date_normalizer");
        let timezone_normalizer = TimeZoneNormalizer::new(config.clone());
        let relative_date_processor = RelativeDateProcessor::new(config.clone());
        let temporal_parser = TemporalExpressionParser::new(config.clone());
        let progress_tracker = get_progress_tracker();
        if !progress_tracker.enabled() {
            progress_tracker.set_enabled(true);
        }
        logger.debug("Date normalizer initialized");
        DateNormalizer {
            config,
            timezone_normalizer,
            relative_date_processor,
            temporal_parser,
            progress_tracker,
            logger,
        }
    }

    /// `normalize_date(date_input, format="ISO8601", timezone="UTC", **options)`: parse
    /// strings (falling back to relative expressions), convert to UTC (or to `timezone`),
    /// then format as ISO 8601, `"date"`, or a `strftime` format. Empty input gives `""`;
    /// unsupported input types raise `ValidationError`.
    pub fn normalize_date(
        &self,
        date_input: impl Into<DateInput>,
        format: Option<&str>,
        timezone: Option<&str>,
    ) -> Result<String, SemanticaError> {
        let date_input = date_input.into();
        if !date_input.truthy() {
            return Ok(String::new());
        }
        let dt = match date_input {
            DateInput::Str(s) => match dateutil::parse(&s, false, now()) {
                Ok(dt) => dt,
                Err(_) => self
                    .relative_date_processor
                    .process_relative_expression(&s, None)?,
            },
            DateInput::DateTime(dt) => dt,
            DateInput::Other(value) => {
                return Err(ValidationError::new(format!(
                    "Unsupported date input type: <class '{}'>",
                    type_name(&value)
                ))
                .into());
            }
        };
        let timezone = timezone.unwrap_or("UTC");
        let dt = if timezone != "UTC" {
            self.timezone_normalizer
                .normalize_timezone(&dt, Some(timezone))
        } else {
            self.timezone_normalizer.convert_to_utc(&dt, None)?
        };
        match format.unwrap_or("ISO8601") {
            "ISO8601" => dt.isoformat(),
            "date" => Ok(dt.date_isoformat()),
            other => dt.strftime(other),
        }
    }

    /// `normalize_time(time_input, **options)`: `HH:MM:SS[.ffffff]` of the parsed time, or
    /// `""` when parsing fails or the input is neither a string nor a datetime.
    pub fn normalize_time(&self, time_input: impl Into<DateInput>) -> String {
        match time_input.into() {
            DateInput::Str(s) => match dateutil::parse(&s, false, now()) {
                Ok(dt) => dt.time_isoformat(),
                Err(_) => String::new(),
            },
            DateInput::DateTime(dt) => dt.time_isoformat(),
            DateInput::Other(_) => String::new(),
        }
    }

    /// `process_relative_date(relative_expression, reference_date=None)`.
    pub fn process_relative_date(
        &self,
        relative_expression: &str,
        reference_date: Option<&DateTime>,
    ) -> Result<DateTime, SemanticaError> {
        self.relative_date_processor
            .process_relative_expression(relative_expression, reference_date)
    }

    /// `parse_temporal_expression(temporal_text, **context)`.
    pub fn parse_temporal_expression(&self, temporal_text: &str) -> Map<String, Value> {
        self.temporal_parser
            .parse_temporal_expression(temporal_text)
    }

    /// The logger (`semantica.date_normalizer`).
    pub fn logger(&self) -> &Logger {
        &self.logger
    }
}

/// Time zone normalization engine (`TimeZoneNormalizer(**config)`).
#[derive(Debug, Clone)]
pub struct TimeZoneNormalizer {
    /// `config` (stored, unused).
    pub config: Map<String, Value>,
    logger: Logger,
}

impl Default for TimeZoneNormalizer {
    fn default() -> Self {
        TimeZoneNormalizer::new(Map::new())
    }
}

impl TimeZoneNormalizer {
    /// The logger (`self.logger`).
    pub fn logger(&self) -> &Logger {
        &self.logger
    }

    /// `TimeZoneNormalizer(**config)`.
    pub fn new(config: Map<String, Value>) -> TimeZoneNormalizer {
        let logger = get_logger("timezone_normalizer");
        logger.debug("Time zone normalizer initialized");
        TimeZoneNormalizer { config, logger }
    }

    /// `normalize_timezone(datetime_obj, target_timezone="UTC")`: the datetime in
    /// `ZoneInfo(target_timezone)` (naive values are taken as UTC); on any failure (unknown
    /// zone, unusable offset) the input is returned unchanged.
    pub fn normalize_timezone(
        &self,
        datetime_obj: &DateTime,
        target_timezone: Option<&str>,
    ) -> DateTime {
        let target = target_timezone.unwrap_or("UTC");
        let Ok(zone) = ZoneInfo::new(target) else {
            return datetime_obj.clone();
        };
        let dt = if datetime_obj.tzinfo.is_none() {
            datetime_obj.replace_tzinfo(Some(TzInfo::Utc))
        } else {
            datetime_obj.clone()
        };
        dt.astimezone(&TzInfo::Zone(zone)).unwrap_or(dt)
    }

    /// `convert_to_utc(datetime_obj, source_timezone=None)`: naive values are taken as UTC (or
    /// put through `normalize_timezone(source_timezone)`), then converted to UTC.
    pub fn convert_to_utc(
        &self,
        datetime_obj: &DateTime,
        source_timezone: Option<&str>,
    ) -> Result<DateTime, SemanticaError> {
        let dt = if datetime_obj.tzinfo.is_none() {
            match source_timezone.filter(|s| !s.is_empty()) {
                Some(source) => self.normalize_timezone(datetime_obj, Some(source)),
                None => datetime_obj.replace_tzinfo(Some(TzInfo::Utc)),
            }
        } else {
            datetime_obj.clone()
        };
        dt.astimezone(&TzInfo::Utc)
    }

    /// `handle_dst_transitions(datetime_obj, timezone_str)`: `normalize_timezone` (the zone
    /// rules handle daylight saving time).
    pub fn handle_dst_transitions(&self, datetime_obj: &DateTime, timezone_str: &str) -> DateTime {
        self.normalize_timezone(datetime_obj, Some(timezone_str))
    }
}

static RELATIVE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(
        r"(\d+)[{PY_SPACE}]*(day|week|month|year)s?[{PY_SPACE}]*(ago|from now|later)?"
    ))
    .expect("valid regex")
});

/// Relative date processing engine (`RelativeDateProcessor(**config)`).
#[derive(Debug, Clone)]
pub struct RelativeDateProcessor {
    /// `config` (stored, unused).
    pub config: Map<String, Value>,
    /// `relative_terms`: term -> day offset (`today`, `yesterday`, `tomorrow`, `now`).
    pub relative_terms: IndexMap<String, i64>,
    logger: Logger,
}

impl Default for RelativeDateProcessor {
    fn default() -> Self {
        RelativeDateProcessor::new(Map::new())
    }
}

fn days_delta(days: i128) -> Result<Duration, SemanticaError> {
    if days.abs() > 999_999_999 {
        return Err(SemanticaError::Overflow(format!(
            "days={days}; must have magnitude <= 999999999"
        )));
    }
    Ok(Duration::days(days as i64))
}

impl RelativeDateProcessor {
    /// The logger (`self.logger`).
    pub fn logger(&self) -> &Logger {
        &self.logger
    }

    /// `RelativeDateProcessor(**config)`.
    pub fn new(config: Map<String, Value>) -> RelativeDateProcessor {
        let logger = get_logger("relative_date_processor");
        let relative_terms = [("today", 0), ("yesterday", -1), ("tomorrow", 1), ("now", 0)]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        logger.debug("Relative date processor initialized");
        RelativeDateProcessor {
            config,
            relative_terms,
            logger,
        }
    }

    /// `process_relative_expression(expression, reference_date=None)`: a relative term, an
    /// `"<n> day|week|month|year[s] [ago|from now|later]"` pattern (default direction: ago),
    /// an absolute date, or else the reference date (default: now).
    pub fn process_relative_expression(
        &self,
        expression: &str,
        reference_date: Option<&DateTime>,
    ) -> Result<DateTime, SemanticaError> {
        let reference_date = reference_date
            .cloned()
            .unwrap_or_else(|| DateTime::naive(now()));
        let expression_lower = pystr::strip(&pystr::lower(expression)).to_string();
        if let Some(days) = self.relative_terms.get(&expression_lower) {
            return reference_date.add(days_delta(i128::from(*days))?);
        }
        if let Some(caps) = RELATIVE_RE.captures(&expression_lower) {
            let amount = pystr::py_int(&caps[1])?;
            let unit = &caps[2];
            let direction = caps.get(3).map_or("ago", |m| m.as_str());
            let backwards = direction == "ago" || direction == "before";
            let sign: i128 = if backwards { -1 } else { 1 };
            return match unit {
                "week" => {
                    let days = amount.checked_mul(7).ok_or_else(|| {
                        SemanticaError::Overflow(
                            "Python int too large to convert to C int".to_string(),
                        )
                    })?;
                    reference_date.add(days_delta(days)? * sign as i32)
                }
                "month" | "year" => {
                    let amount = i64::try_from(amount).map_err(|_| {
                        SemanticaError::Overflow(
                            "Python int too large to convert to C int".to_string(),
                        )
                    })?;
                    let (years, months) = if unit == "month" {
                        (0, amount)
                    } else {
                        (amount, 0)
                    };
                    let sign = sign as i64;
                    dateutil::add_relative_months(&reference_date, years * sign, months * sign)
                }
                _ => reference_date.add(days_delta(amount)? * sign as i32),
            };
        }
        match dateutil::parse(expression, false, now()) {
            Ok(dt) => Ok(dt),
            Err(_) => Ok(reference_date),
        }
    }

    /// `calculate_date_offset(expression, reference_date)`.
    pub fn calculate_date_offset(
        &self,
        expression: &str,
        reference_date: &DateTime,
    ) -> Result<DateTime, SemanticaError> {
        self.process_relative_expression(expression, Some(reference_date))
    }

    /// `handle_relative_terms(term, reference_date)`.
    pub fn handle_relative_terms(
        &self,
        term: &str,
        reference_date: &DateTime,
    ) -> Result<DateTime, SemanticaError> {
        self.process_relative_expression(term, Some(reference_date))
    }
}

static RANGE_PATTERNS: Lazy<[Regex; 2]> = Lazy::new(|| {
    [
        Regex::new(&format!(
            r"(?i)from[{PY_SPACE}]+(.+?)[{PY_SPACE}]+to[{PY_SPACE}]+(.+?)"
        ))
        .expect("valid regex"),
        Regex::new(&format!(
            r"(?i)between[{PY_SPACE}]+(.+?)[{PY_SPACE}]+and[{PY_SPACE}]+(.+?)"
        ))
        .expect("valid regex"),
    ]
});

/// Temporal expression parsing engine (`TemporalExpressionParser(**config)`).
#[derive(Debug, Clone)]
pub struct TemporalExpressionParser {
    /// `config` (stored, unused).
    pub config: Map<String, Value>,
    logger: Logger,
}

impl Default for TemporalExpressionParser {
    fn default() -> Self {
        TemporalExpressionParser::new(Map::new())
    }
}

impl TemporalExpressionParser {
    /// The logger (`self.logger`).
    pub fn logger(&self) -> &Logger {
        &self.logger
    }

    /// `TemporalExpressionParser(**config)`.
    pub fn new(config: Map<String, Value>) -> TemporalExpressionParser {
        let logger = get_logger("temporal_expression_parser");
        logger.debug("Temporal expression parser initialized");
        TemporalExpressionParser { config, logger }
    }

    /// `parse_temporal_expression(text, **context)`: `{"date", "time", "range", "relative":
    /// False}` updated with the date and time components (fuzzy parse) and a range.
    pub fn parse_temporal_expression(&self, text: &str) -> Map<String, Value> {
        let mut result = Map::new();
        result.insert("date".to_string(), Value::Null);
        result.insert("time".to_string(), Value::Null);
        result.insert("range".to_string(), Value::Null);
        result.insert("relative".to_string(), json!(false));
        for (k, v) in self.extract_date_components(text) {
            result.insert(k, v);
        }
        for (k, v) in self.extract_time_components(text) {
            result.insert(k, v);
        }
        if let Some(range) = self.process_temporal_ranges(text) {
            result.insert("range".to_string(), Value::Object(range));
        }
        result
    }

    /// `extract_date_components(text)`: `{"date", "year", "month", "day"}` from a fuzzy parse,
    /// or `{}`.
    pub fn extract_date_components(&self, text: &str) -> Map<String, Value> {
        let mut out = Map::new();
        if let Ok(dt) = dateutil::parse(text, true, now()) {
            use chrono::Datelike;
            out.insert("date".to_string(), json!(dt.date_isoformat()));
            out.insert("year".to_string(), json!(dt.wall.year()));
            out.insert("month".to_string(), json!(dt.wall.month()));
            out.insert("day".to_string(), json!(dt.wall.day()));
        }
        out
    }

    /// `extract_time_components(text)`: `{"time", "hour", "minute", "second"}` from a fuzzy
    /// parse, or `{}`.
    pub fn extract_time_components(&self, text: &str) -> Map<String, Value> {
        let mut out = Map::new();
        if let Ok(dt) = dateutil::parse(text, true, now()) {
            use chrono::Timelike;
            out.insert("time".to_string(), json!(dt.time_isoformat()));
            out.insert("hour".to_string(), json!(dt.wall.hour()));
            out.insert("minute".to_string(), json!(dt.wall.minute()));
            out.insert("second".to_string(), json!(dt.wall.second()));
        }
        out
    }

    /// `process_temporal_ranges(text)`: `{"start", "end"}` (ISO strings) for "from X to Y" or
    /// "between X and Y". As in the original, the lazy patterns capture a single character as
    /// the end of the range.
    pub fn process_temporal_ranges(&self, text: &str) -> Option<Map<String, Value>> {
        for pattern in RANGE_PATTERNS.iter() {
            let Some(caps) = pattern.captures(text) else {
                continue;
            };
            let start = dateutil::parse(&caps[1], false, now());
            let end = dateutil::parse(&caps[2], false, now());
            if let (Ok(start), Ok(end)) = (start, end)
                && let (Ok(start), Ok(end)) = (start.isoformat(), end.isoformat())
            {
                let mut range = Map::new();
                range.insert("start".to_string(), json!(start));
                range.insert("end".to_string(), json!(end));
                return Some(range);
            }
        }
        None
    }
}
