//! Python `datetime` semantics: naive vs aware values, `strftime` (CPython 3.11 on top of the
//! BSD/macOS C library, C locale), `strptime` (a port of `_strptime`), `fromtimestamp` and
//! `isoformat`.

use std::fmt;

use chrono::{
    DateTime, Datelike, FixedOffset, Local, NaiveDate, NaiveDateTime, NaiveTime, Offset, TimeZone,
    Timelike, Utc,
};
use once_cell::sync::Lazy;
use regex::Regex;

use super::fmt::{SPACE_CLASS, str_repr};
use crate::exceptions::SemanticaError;

/// A Python `datetime`: naive (no UTC offset) or aware (fixed UTC offset).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PyDateTime {
    /// A naive datetime (`tzinfo is None`).
    Naive(NaiveDateTime),
    /// An aware datetime with a fixed UTC offset.
    Aware(DateTime<FixedOffset>),
}

impl From<NaiveDateTime> for PyDateTime {
    fn from(value: NaiveDateTime) -> Self {
        PyDateTime::Naive(value)
    }
}

impl From<DateTime<FixedOffset>> for PyDateTime {
    fn from(value: DateTime<FixedOffset>) -> Self {
        PyDateTime::Aware(value)
    }
}

impl From<DateTime<Utc>> for PyDateTime {
    fn from(value: DateTime<Utc>) -> Self {
        PyDateTime::Aware(value.fixed_offset())
    }
}

const WEEKDAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

fn value_error(message: impl Into<String>) -> SemanticaError {
    SemanticaError::Value(message.into())
}

impl PyDateTime {
    /// `datetime.now(timezone.utc)` when `utc`, else the naive local `datetime.now()`.
    pub fn now(utc: bool) -> PyDateTime {
        if utc {
            PyDateTime::Aware(Utc::now().fixed_offset())
        } else {
            PyDateTime::Naive(Local::now().naive_local())
        }
    }

    /// `datetime.fromtimestamp(ts, timezone.utc)` when `utc`, else `datetime.fromtimestamp(ts)`
    /// (naive local time). Microseconds are rounded half-to-even, as CPython does.
    pub fn from_timestamp(ts: f64, utc: bool) -> Result<PyDateTime, SemanticaError> {
        if ts.is_nan() {
            return Err(value_error("Invalid value NaN (not a number)"));
        }
        let mut intpart = ts.trunc();
        let mut floatpart = round_half_even((ts - intpart) * 1e6);
        if floatpart >= 1e6 {
            floatpart -= 1e6;
            intpart += 1.0;
        } else if floatpart < 0.0 {
            floatpart += 1e6;
            intpart -= 1.0;
        }
        if !(-9.223_372_036_854_776e18..9.223_372_036_854_776e18).contains(&intpart) {
            return Err(SemanticaError::Overflow(
                "timestamp out of range for platform time_t".to_string(),
            ));
        }
        let secs = intpart as i64;
        let micros = floatpart as u32;
        // gmtime()/localtime() must fit the year in a C int; datetime then needs 1..=9999.
        let (approx_year, _, _) = civil_from_days(secs.div_euclid(86_400));
        if approx_year - 1900 > i64::from(i32::MAX) || approx_year - 1900 < i64::from(i32::MIN) {
            return Err(SemanticaError::Os(crate::exceptions::OsError {
                type_name: "OSError".to_string(),
                errno: Some(84),
                text: "Value too large to be stored in data type".to_string(),
                filename: None,
            }));
        }
        let utc_dt = match DateTime::<Utc>::from_timestamp(secs, micros * 1000) {
            Some(dt) => dt,
            None => return Err(value_error(format!("year {approx_year} is out of range"))),
        };
        let result = if utc {
            PyDateTime::Aware(utc_dt.fixed_offset())
        } else {
            PyDateTime::Naive(utc_dt.with_timezone(&Local).naive_local())
        };
        let year = result.wall().year();
        if !(1..=9999).contains(&year) {
            return Err(value_error(format!("year {year} is out of range")));
        }
        Ok(result)
    }

    /// The wall-clock date and time (what the fields `year`..`microsecond` hold).
    pub fn wall(&self) -> NaiveDateTime {
        match self {
            PyDateTime::Naive(dt) => *dt,
            PyDateTime::Aware(dt) => dt.naive_local(),
        }
    }

    /// `utcoffset()`, `None` for naive values.
    pub fn offset(&self) -> Option<FixedOffset> {
        match self {
            PyDateTime::Naive(_) => None,
            PyDateTime::Aware(dt) => Some(dt.offset().fix()),
        }
    }

    /// `datetime.isoformat(sep)`.
    pub fn isoformat(&self, sep: char) -> String {
        let wall = self.wall();
        let mut out = format!(
            "{:04}-{:02}-{:02}{}{:02}:{:02}:{:02}",
            wall.year(),
            wall.month(),
            wall.day(),
            sep,
            wall.hour(),
            wall.minute(),
            wall.second()
        );
        let micros = wall.nanosecond() / 1000;
        if micros != 0 {
            out.push_str(&format!(".{micros:06}"));
        }
        if let Some(offset) = self.offset() {
            out.push_str(&format_offset(offset.local_minus_utc(), true));
        }
        out
    }

    /// `datetime.strftime(format)`.
    pub fn strftime(&self, format: &str) -> String {
        strftime(self, format)
    }
}

impl fmt::Display for PyDateTime {
    /// `str(datetime)`, i.e. `isoformat(' ')`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.isoformat(' '))
    }
}

/// Proleptic Gregorian `(year, month, day)` of a day count since 1970-01-01.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

fn round_half_even(x: f64) -> f64 {
    let rounded = x.round();
    if (x - rounded).abs() == 0.5 {
        2.0 * (x / 2.0).round()
    } else {
        rounded
    }
}

/// `+HHMM[SS]` (`colons = false`, `%z`) or `+HH:MM[:SS]` (`isoformat`).
fn format_offset(seconds: i32, colons: bool) -> String {
    let sign = if seconds < 0 { '-' } else { '+' };
    let total = seconds.unsigned_abs();
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    let sep = if colons { ":" } else { "" };
    let mut out = format!("{sign}{h:02}{sep}{m:02}");
    if s != 0 {
        out.push_str(&format!("{sep}{s:02}"));
    }
    out
}

/// `tzinfo.tzname()` of a `datetime.timezone` without an explicit name.
fn offset_name(seconds: i32) -> String {
    if seconds == 0 {
        "UTC".to_string()
    } else {
        format!("UTC{}", format_offset(seconds, true))
    }
}

fn pad(value: i64, width: usize, flag: Option<char>, default_pad: char) -> String {
    let pad_char = match flag {
        Some('-') => return value.to_string(),
        Some('_') => ' ',
        Some('0') => '0',
        _ => default_pad,
    };
    let digits = value.unsigned_abs().to_string();
    let sign = if value < 0 { "-" } else { "" };
    if digits.len() + sign.len() >= width {
        return format!("{sign}{digits}");
    }
    let fill: String = std::iter::repeat_n(pad_char, width - digits.len() - sign.len()).collect();
    if pad_char == '0' {
        format!("{sign}{fill}{digits}")
    } else {
        format!("{fill}{sign}{digits}")
    }
}

/// Week number with weeks starting on `first_day` (0 = Sunday for `%U`, 1 = Monday for `%W`).
fn week_number(date: NaiveDate, monday_first: bool) -> i64 {
    let yday = i64::from(date.ordinal0());
    let wday = if monday_first {
        i64::from(date.weekday().num_days_from_monday())
    } else {
        i64::from(date.weekday().num_days_from_sunday())
    };
    (yday + 7 - wday) / 7
}

/// `datetime.strftime`: Python expands `%z`, `%Z` and `%f`; the C library (BSD semantics, C
/// locale) handles everything else, including the `-`, `_` and `0` padding flags and the
/// ignored `E`/`O` modifiers. Unknown conversions print the conversion character.
pub fn strftime(dt: &PyDateTime, format: &str) -> String {
    let wall = dt.wall();
    let date = wall.date();
    let chars: Vec<char> = format.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c != '%' {
            out.push(c);
            i += 1;
            continue;
        }
        if i + 1 >= chars.len() {
            out.push('%');
            break;
        }
        // Python's own directives (only when they follow the '%' directly).
        match chars[i + 1] {
            'z' => {
                if let Some(offset) = dt.offset() {
                    out.push_str(&format_offset(offset.local_minus_utc(), false));
                }
                i += 2;
                continue;
            }
            'Z' => {
                if let Some(offset) = dt.offset() {
                    out.push_str(&offset_name(offset.local_minus_utc()));
                }
                i += 2;
                continue;
            }
            'f' => {
                out.push_str(&format!("{:06}", wall.nanosecond() / 1000));
                i += 2;
                continue;
            }
            _ => {}
        }
        let mut j = i + 1;
        let mut flag = None;
        while j < chars.len() && matches!(chars[j], '-' | '_' | '0' | 'E' | 'O') {
            if matches!(chars[j], '-' | '_' | '0') {
                flag = Some(chars[j]);
            }
            j += 1;
        }
        if j >= chars.len() {
            // A dangling conversion prefix copies through.
            out.extend(&chars[i..]);
            break;
        }
        let conv = chars[j];
        i = j + 1;
        let year = i64::from(wall.year());
        let hour = i64::from(wall.hour());
        let hour12 = if hour % 12 == 0 { 12 } else { hour % 12 };
        let piece = match conv {
            'a' => WEEKDAYS[date.weekday().num_days_from_monday() as usize][..3].to_string(),
            'A' => WEEKDAYS[date.weekday().num_days_from_monday() as usize].to_string(),
            'b' | 'h' => MONTHS[date.month0() as usize][..3].to_string(),
            'B' => MONTHS[date.month0() as usize].to_string(),
            'c' => strftime(dt, "%a %b %e %H:%M:%S %Y"),
            'C' => pad(year.div_euclid(100), 2, flag, '0'),
            'd' => pad(i64::from(wall.day()), 2, flag, '0'),
            'D' => strftime(dt, "%m/%d/%y"),
            'e' => pad(i64::from(wall.day()), 2, flag, ' '),
            'F' => strftime(dt, "%Y-%m-%d"),
            'G' => pad(i64::from(date.iso_week().year()), 4, flag, '0'),
            'g' => pad(
                i64::from(date.iso_week().year()).rem_euclid(100),
                2,
                flag,
                '0',
            ),
            'H' => pad(hour, 2, flag, '0'),
            'I' => pad(hour12, 2, flag, '0'),
            'j' => pad(i64::from(date.ordinal()), 3, flag, '0'),
            'k' => pad(hour, 2, flag, ' '),
            'l' => pad(hour12, 2, flag, ' '),
            'm' => pad(i64::from(wall.month()), 2, flag, '0'),
            'M' => pad(i64::from(wall.minute()), 2, flag, '0'),
            'n' => "\n".to_string(),
            'p' => if hour < 12 { "AM" } else { "PM" }.to_string(),
            'r' => strftime(dt, "%I:%M:%S %p"),
            'R' => strftime(dt, "%H:%M"),
            's' => {
                let secs = match dt {
                    PyDateTime::Aware(aware) => aware.timestamp(),
                    PyDateTime::Naive(naive) => Local
                        .from_local_datetime(naive)
                        .earliest()
                        .map(|local| local.timestamp())
                        .unwrap_or_else(|| naive.and_utc().timestamp()),
                };
                secs.to_string()
            }
            'S' => pad(i64::from(wall.second()), 2, flag, '0'),
            't' => "\t".to_string(),
            'T' => strftime(dt, "%H:%M:%S"),
            'u' => i64::from(date.weekday().number_from_monday()).to_string(),
            'U' => pad(week_number(date, false), 2, flag, '0'),
            'V' => pad(i64::from(date.iso_week().week()), 2, flag, '0'),
            'v' => strftime(dt, "%e-%b-%Y"),
            'w' => i64::from(date.weekday().num_days_from_sunday()).to_string(),
            'W' => pad(week_number(date, true), 2, flag, '0'),
            'x' => strftime(dt, "%m/%d/%y"),
            'X' => strftime(dt, "%H:%M:%S"),
            'y' => pad(year.rem_euclid(100), 2, flag, '0'),
            'Y' => pad(year, 4, flag, '0'),
            // Reached only with flags (`%-z`): the C library sees Python's `timetuple()`,
            // which carries no zone, so these print an empty name and a zero offset.
            'z' => "+0000".to_string(),
            'Z' => String::new(),
            '+' => strftime(dt, "%a %b %e %H:%M:%S %-Z %Y"),
            '%' => "%".to_string(),
            other => other.to_string(),
        };
        out.push_str(&piece);
    }
    out
}

const A_WEEKDAY: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];
const F_WEEKDAY: [&str; 7] = [
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
];
const A_MONTH: [&str; 13] = [
    "", "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];
const F_MONTH: [&str; 13] = [
    "",
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];
const AM_PM: [&str; 2] = ["am", "pm"];
const TZ_NAMES: [&str; 2] = ["utc", "gmt"];

fn seq_to_re(values: &[&str], directive: char) -> String {
    let mut sorted: Vec<&str> = values.to_vec();
    sorted.sort_by_key(|v| std::cmp::Reverse(v.chars().count()));
    if sorted.iter().all(|v| v.is_empty()) {
        return String::new();
    }
    let alternatives: Vec<String> = sorted.iter().map(|v| regex::escape(v)).collect();
    format!("(?P<{directive}>{})", alternatives.join("|"))
}

/// `TimeRE[directive]` for the C locale.
fn directive_regex(directive: char) -> Option<String> {
    let fixed = match directive {
        'd' => r"(?P<d>3[0-1]|[1-2]\d|0[1-9]|[1-9]| [1-9])",
        'f' => r"(?P<f>[0-9]{1,6})",
        'H' => r"(?P<H>2[0-3]|[0-1]\d|\d)",
        'I' => r"(?P<I>1[0-2]|0[1-9]|[1-9])",
        'G' => r"(?P<G>\d\d\d\d)",
        'j' => r"(?P<j>36[0-6]|3[0-5]\d|[1-2]\d\d|0[1-9]\d|00[1-9]|[1-9]\d|0[1-9]|[1-9])",
        'm' => r"(?P<m>1[0-2]|0[1-9]|[1-9])",
        'M' => r"(?P<M>[0-5]\d|\d)",
        'S' => r"(?P<S>6[0-1]|[0-5]\d|\d)",
        'U' => r"(?P<U>5[0-3]|[0-4]\d|\d)",
        'W' => r"(?P<W>5[0-3]|[0-4]\d|\d)",
        'w' => r"(?P<w>[0-6])",
        'u' => r"(?P<u>[1-7])",
        'V' => r"(?P<V>5[0-3]|0[1-9]|[1-4]\d|\d)",
        'y' => r"(?P<y>\d\d)",
        'Y' => r"(?P<Y>\d\d\d\d)",
        'z' => r"(?P<z>[+-]\d\d:?[0-5]\d(:?[0-5]\d(\.\d{1,6})?)?|(?-i:Z))",
        '%' => "%",
        'A' => return Some(seq_to_re(&F_WEEKDAY, 'A')),
        'a' => return Some(seq_to_re(&A_WEEKDAY, 'a')),
        'B' => return Some(seq_to_re(&F_MONTH[1..], 'B')),
        'b' => return Some(seq_to_re(&A_MONTH[1..], 'b')),
        'p' => return Some(seq_to_re(&AM_PM, 'p')),
        'Z' => return Some(seq_to_re(&TZ_NAMES, 'Z')),
        'c' => return time_re_pattern("%a %b %d %H:%M:%S %Y").ok(),
        'x' => return time_re_pattern("%m/%d/%y").ok(),
        'X' => return time_re_pattern("%H:%M:%S").ok(),
        _ => return None,
    };
    Some(fixed.to_string())
}

static REGEX_CHARS: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"([\\.^$*+?(){}\[\]|])").expect("regex chars"));
static WHITESPACE_RUN: Lazy<Regex> =
    Lazy::new(|| Regex::new(&format!("[{SPACE_CLASS}]+")).expect("whitespace run"));

enum PatternError {
    BadDirective(char),
    Stray,
}

/// `TimeRE.pattern(format)`.
fn time_re_pattern(format: &str) -> Result<String, PatternError> {
    let escaped = REGEX_CHARS.replace_all(format, r"\$1");
    let processed = WHITESPACE_RUN.replace_all(&escaped, r"\s+");
    let chars: Vec<char> = processed.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '%' {
            let directive = *chars.get(i + 1).ok_or(PatternError::Stray)?;
            let re = directive_regex(directive).ok_or(PatternError::BadDirective(directive))?;
            out.push_str(&re);
            i += 2;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    // Python's `\s` also covers U+001C..U+001F.
    Ok(out.replace(r"\s+", &format!("[{SPACE_CLASS}]+")))
}

/// The value of a (possibly non-ASCII) decimal digit string, as `int()` reads it.
fn parse_digits(s: &str) -> i64 {
    static DIGIT: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\p{Nd}$").expect("digit class"));
    let mut value: i64 = 0;
    for c in s.trim().chars() {
        let digit = c.to_digit(10).map(i64::from).unwrap_or_else(|| {
            // Unicode decimal digits come in contiguous runs of ten starting at zero.
            let mut start = c as u32;
            let mut buf = [0u8; 4];
            while start > 0 {
                match char::from_u32(start - 1) {
                    Some(prev) if DIGIT.is_match(prev.encode_utf8(&mut buf)) => start -= 1,
                    _ => break,
                }
            }
            i64::from((c as u32 - start) % 10)
        });
        value = value * 10 + digit;
    }
    value
}

fn date_or_error(year: i64, month: i64, day: i64) -> Result<NaiveDate, SemanticaError> {
    if !(1..=9999).contains(&year) {
        return Err(value_error(format!("year {year} is out of range")));
    }
    if !(1..=12).contains(&month) {
        return Err(value_error("month must be in 1..12"));
    }
    NaiveDate::from_ymd_opt(year as i32, month as u32, day as u32)
        .ok_or_else(|| value_error("day is out of range for month"))
}

fn ordinal_of(date: NaiveDate) -> i64 {
    i64::from(date.num_days_from_ce())
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// `repr()` of a `datetime.timedelta` built from seconds and microseconds.
fn timedelta_repr(seconds: i64, microseconds: i64) -> String {
    let total_us = seconds * 1_000_000 + microseconds;
    let days = total_us.div_euclid(86_400_000_000);
    let rem = total_us.rem_euclid(86_400_000_000);
    let secs = rem / 1_000_000;
    let micros = rem % 1_000_000;
    let mut parts = Vec::new();
    if days != 0 {
        parts.push(format!("days={days}"));
    }
    if secs != 0 {
        parts.push(format!("seconds={secs}"));
    }
    if micros != 0 {
        parts.push(format!("microseconds={micros}"));
    }
    if parts.is_empty() {
        "datetime.timedelta(0)".to_string()
    } else {
        format!("datetime.timedelta({})", parts.join(", "))
    }
}

/// `datetime.strptime(data, format)` (CPython's `_strptime`, C locale).
pub fn strptime(data: &str, format: &str) -> Result<PyDateTime, SemanticaError> {
    let pattern = match time_re_pattern(format) {
        Ok(p) => p,
        Err(PatternError::BadDirective(d)) => {
            let shown = if d == '\\' { '%' } else { d };
            return Err(value_error(format!(
                "'{shown}' is a bad directive in format '{format}'"
            )));
        }
        Err(PatternError::Stray) => {
            return Err(value_error(format!("stray % in format '{format}'")));
        }
    };
    let re = Regex::new(&format!("(?i)^(?:{pattern})")).map_err(|e| SemanticaError::Other {
        module: "re".to_string(),
        type_name: "error".to_string(),
        message: e.to_string(),
    })?;
    let caps = re.captures(data).ok_or_else(|| {
        value_error(format!(
            "time data {} does not match format {}",
            str_repr(data),
            str_repr(format)
        ))
    })?;
    let end = caps.get(0).map(|m| m.end()).unwrap_or(0);
    if end != data.len() {
        return Err(value_error(format!(
            "unconverted data remains: {}",
            &data[end..]
        )));
    }

    let mut iso_year: Option<i64> = None;
    let mut year: Option<i64> = None;
    let (mut month, mut day) = (1i64, 1i64);
    let (mut hour, mut minute, mut second, mut fraction) = (0i64, 0i64, 0i64, 0i64);
    let mut gmtoff: Option<i64> = None;
    let mut iso_week: Option<i64> = None;
    let mut week_of_year: Option<i64> = None;
    let mut week_starts_monday = false;
    let mut weekday: Option<i64> = None;
    let mut julian: Option<i64> = None;

    let group = |name: &str| caps.name(name).map(|m| m.as_str());
    for name in re.capture_names().flatten() {
        let Some(found) = group(name) else { continue };
        match name {
            "y" => {
                let y = parse_digits(found);
                year = Some(if y <= 68 { y + 2000 } else { y + 1900 });
            }
            "Y" => year = Some(parse_digits(found)),
            "G" => iso_year = Some(parse_digits(found)),
            "m" => month = parse_digits(found),
            "B" => {
                let lower = found.to_lowercase();
                month = F_MONTH.iter().position(|m| *m == lower).unwrap_or(0) as i64;
            }
            "b" => {
                let lower = found.to_lowercase();
                month = A_MONTH.iter().position(|m| *m == lower).unwrap_or(0) as i64;
            }
            "d" => day = parse_digits(found),
            "H" => hour = parse_digits(found),
            "I" => {
                hour = parse_digits(found);
                let ampm = group("p").unwrap_or("").to_lowercase();
                if ampm.is_empty() || ampm == AM_PM[0] {
                    if hour == 12 {
                        hour = 0;
                    }
                } else if ampm == AM_PM[1] && hour != 12 {
                    hour += 12;
                }
            }
            "M" => minute = parse_digits(found),
            "S" => second = parse_digits(found),
            "f" => {
                let padded = format!("{found:0<6}");
                fraction = parse_digits(&padded);
            }
            "A" => {
                let lower = found.to_lowercase();
                weekday = F_WEEKDAY.iter().position(|d| *d == lower).map(|p| p as i64);
            }
            "a" => {
                let lower = found.to_lowercase();
                weekday = A_WEEKDAY.iter().position(|d| *d == lower).map(|p| p as i64);
            }
            "w" => {
                let w = parse_digits(found);
                weekday = Some(if w == 0 { 6 } else { w - 1 });
            }
            "u" => weekday = Some(parse_digits(found) - 1),
            "j" => julian = Some(parse_digits(found)),
            "U" | "W" => {
                week_of_year = Some(parse_digits(found));
                week_starts_monday = name == "W";
            }
            "V" => iso_week = Some(parse_digits(found)),
            "z" => {
                if found == "Z" {
                    gmtoff = Some(0);
                } else {
                    let mut z: Vec<char> = found.chars().collect();
                    if z[3] == ':' {
                        z.remove(3);
                        if z.len() > 5 {
                            if z[5] != ':' {
                                return Err(value_error(format!(
                                    "Inconsistent use of : in {found}"
                                )));
                            }
                            z.remove(5);
                        }
                    }
                    let text: String = z.iter().collect();
                    let hours = parse_digits(&text[1..3]);
                    let minutes = parse_digits(&text[3..5]);
                    let seconds = if text.len() >= 7 {
                        parse_digits(&text[5..7])
                    } else {
                        0
                    };
                    let mut off = hours * 3600 + minutes * 60 + seconds;
                    if text.starts_with('-') {
                        off = -off;
                    }
                    gmtoff = Some(off);
                }
            }
            _ => {}
        }
    }

    if iso_year.is_some() {
        if julian.is_some() {
            return Err(value_error(
                "Day of the year directive '%j' is not compatible with ISO year directive \
                 '%G'. Use '%Y' instead.",
            ));
        } else if iso_week.is_none() || weekday.is_none() {
            return Err(value_error(
                "ISO year directive '%G' must be used with the ISO week directive '%V' and a \
                 weekday directive ('%A', '%a', '%w', or '%u').",
            ));
        }
    } else if iso_week.is_some() {
        if year.is_none() || weekday.is_none() {
            return Err(value_error(
                "ISO week directive '%V' must be used with the ISO year directive '%G' and a \
                 weekday directive ('%A', '%a', '%w', or '%u').",
            ));
        }
        return Err(value_error(
            "ISO week directive '%V' is incompatible with the year directive '%Y'. Use the ISO \
             year '%G' instead.",
        ));
    }

    let mut leap_year_fix = false;
    let mut year = match year {
        Some(y) => y,
        None if month == 2 && day == 29 => {
            leap_year_fix = true;
            1904
        }
        None => 1900,
    };

    if julian.is_none()
        && let Some(wd) = weekday
    {
        if let Some(week) = week_of_year {
            let jan1 = date_or_error(year, 1, 1)?;
            let mut first_weekday = i64::from(jan1.weekday().num_days_from_monday());
            let mut day_of_week = wd;
            if !week_starts_monday {
                first_weekday = (first_weekday + 1) % 7;
                day_of_week = (day_of_week + 1) % 7;
            }
            let week_0_length = (7 - first_weekday) % 7;
            julian = Some(if week == 0 {
                1 + day_of_week - first_weekday
            } else {
                1 + week_0_length + 7 * (week - 1) + day_of_week
            });
        } else if let (Some(iy), Some(iw)) = (iso_year, iso_week) {
            let jan4 = date_or_error(iy, 1, 4)?;
            let correction = i64::from(jan4.weekday().number_from_monday()) + 3;
            let mut ordinal = iw * 7 + (wd + 1) - correction;
            let mut y = iy;
            if ordinal < 1 {
                ordinal += ordinal_of(date_or_error(y, 1, 1)?);
                y -= 1;
                ordinal -= ordinal_of(date_or_error(y, 1, 1)?);
            }
            year = y;
            julian = Some(ordinal);
        }
        if let Some(j) = julian
            && j <= 0
        {
            year -= 1;
            julian = Some(j + if is_leap(year) { 366 } else { 365 });
        }
    }

    match julian {
        None => {
            date_or_error(year, month, day)?;
        }
        Some(j) => {
            let base = ordinal_of(date_or_error(year, 1, 1)?);
            let date = NaiveDate::from_num_days_from_ce_opt((j - 1 + base) as i32)
                .ok_or_else(|| value_error("year is out of range"))?;
            year = i64::from(date.year());
            month = i64::from(date.month());
            day = i64::from(date.day());
        }
    }
    if leap_year_fix {
        year = 1900;
    }

    let date = date_or_error(year, month, day)?;
    if !(0..=23).contains(&hour) {
        return Err(value_error("hour must be in 0..23"));
    }
    if !(0..=59).contains(&minute) {
        return Err(value_error("minute must be in 0..59"));
    }
    if !(0..=59).contains(&second) {
        return Err(value_error("second must be in 0..59"));
    }
    let time =
        NaiveTime::from_hms_micro_opt(hour as u32, minute as u32, second as u32, fraction as u32)
            .ok_or_else(|| value_error("microsecond must be in 0..999999"))?;
    let naive = NaiveDateTime::new(date, time);
    match gmtoff {
        None => Ok(PyDateTime::Naive(naive)),
        Some(off) => {
            let offset = FixedOffset::east_opt(off as i32)
                .filter(|_| off.abs() < 86_400)
                .ok_or_else(|| {
                    value_error(format!(
                        "offset must be a timedelta strictly between -timedelta(hours=24) and \
                         timedelta(hours=24), not {}.",
                        timedelta_repr(off, 0)
                    ))
                })?;
            let aware = offset
                .from_local_datetime(&naive)
                .single()
                .ok_or_else(|| value_error("invalid local time"))?;
            Ok(PyDateTime::Aware(aware))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strftime_matches_python_on_macos() {
        let dt = PyDateTime::Naive(
            NaiveDate::from_ymd_opt(2024, 3, 5)
                .unwrap()
                .and_hms_micro_opt(7, 8, 9, 12345)
                .unwrap(),
        );
        assert_eq!(strftime(&dt, "%c"), "Tue Mar  5 07:08:09 2024");
        assert_eq!(
            strftime(
                &dt,
                "%C|%D|%F|%G|%g|%h|%n|%r|%R|%t|%T|%u|%U|%V|%w|%W|%x|%X|%y|%j"
            ),
            "20|03/05/24|2024-03-05|2024|24|Mar|\n|07:08:09 AM|07:08|\t|07:08:09|2|09|10|2|10|03/05/24|07:08:09|24|065"
        );
        assert_eq!(strftime(&dt, "%-d|%_d|%0d|%Q|%Ey|%Od"), "5| 5|05|Q|24|05");
        assert_eq!(strftime(&dt, "%f|%z|%Z"), "012345||");
    }

    #[test]
    fn strptime_parses_offsets_and_reports_errors() {
        let aware = strptime("2024-01-15T10:30:00+0530", "%Y-%m-%dT%H:%M:%S%z").unwrap();
        assert_eq!(aware.to_string(), "2024-01-15 10:30:00+05:30");
        let err = strptime("2024-01-01x", "%Y-%m-%d").unwrap_err();
        assert_eq!(err.to_string(), "unconverted data remains: x");
        let err = strptime("2024-13-01", "%Y-%m-%d").unwrap_err();
        assert_eq!(
            err.to_string(),
            "time data '2024-13-01' does not match format '%Y-%m-%d'"
        );
        let err = strptime("x", "%Y").unwrap_err();
        assert_eq!(err.to_string(), "time data 'x' does not match format '%Y'");
    }
}
