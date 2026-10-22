//! `datetime.fromisoformat` exactly as CPython 3.11's C implementation (`_datetimemodule.c`)
//! parses it: the formats it accepts, the ones it rejects, and its error messages.
//!
//! The provenance module needs it twice: `SourceReference.from_dict` turns an ISO string back
//! into a `datetime`, and `ProvenanceManager.export_prov` writes `xsd:dateTime` literals whose
//! lexical form rdflib normalises through `datetime.fromisoformat(...).isoformat()`.

use chrono::{FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Weekday};
use sem_utils::SemanticaError;
use sem_utils::pycompat::fmt::str_repr;
use sem_utils::pycompat::time::PyDateTime;

/// The bytes of the string with C's implicit NUL terminator past the end.
struct Bytes<'a>(&'a [u8]);

impl Bytes<'_> {
    fn at(&self, index: usize) -> u8 {
        self.0.get(index).copied().unwrap_or(0)
    }

    fn len(&self) -> usize {
        self.0.len()
    }

    /// `parse_digits`: `count` ASCII digits starting at `pos`.
    fn digits(&self, pos: usize, count: usize) -> Option<(usize, i64)> {
        let mut value = 0i64;
        for i in 0..count {
            let c = self.at(pos + i);
            if !c.is_ascii_digit() {
                return None;
            }
            value = value * 10 + i64::from(c - b'0');
        }
        Some((pos + count, value))
    }
}

/// `datetime.datetime.fromisoformat(date_string)`.
///
/// Offsets that are not a whole number of seconds keep only their seconds (`PyDateTime` holds a
/// `chrono::FixedOffset`); Python keeps the microseconds of such an offset.
pub fn datetime_fromisoformat(date_string: &str) -> Result<PyDateTime, SemanticaError> {
    let invalid = || {
        SemanticaError::Value(format!(
            "Invalid isoformat string: {}",
            str_repr(date_string)
        ))
    };
    // `_sanitize_isoformat_str`: every valid string has at least 7 characters.
    if date_string.chars().count() < 7 {
        return Err(invalid());
    }
    let bytes = Bytes(date_string.as_bytes());
    let len = bytes.len();
    let separator = find_isoformat_datetime_separator(&bytes);
    if separator < 0 {
        return Err(invalid());
    }
    let separator = separator as usize;

    let (year, month, day) = match parse_isoformat_date(&bytes, separator) {
        Ok(date) => date,
        Err(()) => return Err(invalid()),
    };
    let mut time = TimeParts::default();
    let mut has_offset = false;
    if len > separator {
        match parse_isoformat_time(&bytes, separator + 1, len) {
            Ok((parts, offset)) => {
                time = parts;
                has_offset = offset;
            }
            Err(()) => return Err(invalid()),
        }
    }

    // `tzinfo_from_isoformat_results` runs before the field range checks.
    let offset = if has_offset {
        if time.tz_offset == 0 {
            Some(0)
        } else {
            check_offset_range(time.tz_offset, time.tz_microsecond)?;
            Some(time.tz_offset as i32)
        }
    } else {
        None
    };
    let naive = new_datetime(
        year,
        month,
        day,
        time.hour,
        time.minute,
        time.second,
        time.microsecond,
    )?;
    Ok(match offset {
        None => PyDateTime::Naive(naive),
        Some(seconds) => {
            let fixed = FixedOffset::east_opt(seconds).expect("offset validated to be under 24h");
            PyDateTime::Aware(
                fixed
                    .from_local_datetime(&naive)
                    .single()
                    .expect("fixed offsets are unambiguous"),
            )
        }
    })
}

/// `_find_isoformat_datetime_separator`; `-1` for strings that cannot be valid.
fn find_isoformat_datetime_separator(bytes: &Bytes<'_>) -> isize {
    let len = bytes.len();
    if len == 7 {
        return 7;
    }
    if bytes.at(4) == b'-' {
        if bytes.at(5) == b'W' {
            if len < 8 {
                return -1;
            }
            if len > 8 && bytes.at(8) == b'-' {
                if len == 9 {
                    return -1;
                }
                if len > 10 && bytes.at(10).is_ascii_digit() {
                    return 8;
                }
                return 10;
            }
            8
        } else {
            10
        }
    } else if bytes.at(4) == b'W' {
        let mut idx = 7;
        while idx < len {
            if !bytes.at(idx).is_ascii_digit() {
                break;
            }
            idx += 1;
        }
        if idx < 9 {
            return idx as isize;
        }
        if idx % 2 == 0 { 7 } else { 8 }
    } else {
        8
    }
}

/// `parse_isoformat_date` over the first `len` bytes (`len` is the separator position).
fn parse_isoformat_date(bytes: &Bytes<'_>, len: usize) -> Result<(i64, i64, i64), ()> {
    let (mut p, year) = bytes.digits(0, 4).ok_or(())?;
    let uses_separator = bytes.at(p) == b'-';
    if uses_separator {
        p += 1;
    }
    if bytes.at(p) == b'W' {
        p += 1;
        let (next, iso_week) = bytes.digits(p, 2).ok_or(())?;
        p = next;
        let iso_day = if p < len {
            if uses_separator {
                let c = bytes.at(p);
                p += 1;
                if c != b'-' {
                    return Err(());
                }
            }
            bytes.digits(p, 1).ok_or(())?.1
        } else {
            1
        };
        return iso_to_ymd(year, iso_week, iso_day);
    }
    let (next, month) = bytes.digits(p, 2).ok_or(())?;
    p = next;
    if uses_separator {
        let c = bytes.at(p);
        p += 1;
        if c != b'-' {
            return Err(());
        }
    }
    let (_, day) = bytes.digits(p, 2).ok_or(())?;
    Ok((year, month, day))
}

/// `iso_to_ymd`: ISO year/week/weekday to a proleptic Gregorian date.
fn iso_to_ymd(iso_year: i64, iso_week: i64, iso_day: i64) -> Result<(i64, i64, i64), ()> {
    if !(1..=53).contains(&iso_week) || !(1..=7).contains(&iso_day) {
        return Err(());
    }
    let weekday = Weekday::try_from((iso_day - 1) as u8).map_err(|_| ())?;
    let date = NaiveDate::from_isoywd_opt(iso_year as i32, iso_week as u32, weekday).ok_or(())?;
    use chrono::Datelike;
    Ok((
        i64::from(date.year()),
        i64::from(date.month()),
        i64::from(date.day()),
    ))
}

#[derive(Debug, Default, Clone, Copy)]
struct TimeParts {
    hour: i64,
    minute: i64,
    second: i64,
    microsecond: i64,
    tz_offset: i64,
    tz_microsecond: i64,
}

/// `parse_hh_mm_ss_ff` between `start` and `end`; returns `(rv, hour, minute, second, us)` with
/// C's return codes (`0` end of string, `1` more text follows, negative on failure).
fn parse_hh_mm_ss_ff(bytes: &Bytes<'_>, start: usize, end: usize) -> (i32, [i64; 4]) {
    let mut vals = [0i64; 4];
    let mut p = start;
    let mut has_separator = true;
    let mut i = 0;
    while i < 3 {
        match bytes.digits(p, 2) {
            Some((next, value)) => {
                vals[i] = value;
                p = next;
            }
            None => return (-3, vals),
        }
        let c = bytes.at(p);
        p += 1;
        if i == 0 {
            has_separator = c == b':';
        }
        if p >= end {
            return (i32::from(c != 0), vals);
        } else if has_separator && c == b':' {
            i += 1;
            continue;
        } else if c == b'.' || c == b',' {
            break;
        } else if !has_separator {
            p -= 1;
        } else {
            return (-4, vals);
        }
        i += 1;
    }
    let len_remains = end.saturating_sub(p);
    let to_parse = len_remains.min(6);
    match bytes.digits(p, to_parse) {
        Some((next, value)) => {
            p = next;
            vals[3] = value;
        }
        None => return (-3, vals),
    }
    const CORRECTION: [i64; 5] = [100_000, 10_000, 1_000, 100, 10];
    if (1..6).contains(&to_parse) {
        vals[3] *= CORRECTION[to_parse - 1];
    }
    while bytes.at(p).is_ascii_digit() {
        p += 1;
    }
    (i32::from(bytes.at(p) != 0), vals)
}

/// `parse_isoformat_time` from `start` to `end`; `Ok((parts, has_offset))`.
fn parse_isoformat_time(
    bytes: &Bytes<'_>,
    start: usize,
    end: usize,
) -> Result<(TimeParts, bool), ()> {
    let mut tzinfo_pos = start;
    loop {
        let c = bytes.at(tzinfo_pos);
        if c == b'Z' || c == b'+' || c == b'-' {
            break;
        }
        tzinfo_pos += 1;
        if tzinfo_pos >= end {
            break;
        }
    }
    let (rv, vals) = parse_hh_mm_ss_ff(bytes, start, tzinfo_pos);
    let mut parts = TimeParts {
        hour: vals[0],
        minute: vals[1],
        second: vals[2],
        microsecond: vals[3],
        ..TimeParts::default()
    };
    if rv < 0 {
        return Err(());
    } else if tzinfo_pos == end {
        return if rv == 1 { Err(()) } else { Ok((parts, false)) };
    }
    if bytes.at(tzinfo_pos) == b'Z' {
        return if bytes.at(tzinfo_pos + 1) != 0 {
            Err(())
        } else {
            Ok((parts, true))
        };
    }
    let sign = if bytes.at(tzinfo_pos) == b'-' { -1 } else { 1 };
    let (rv, tz) = parse_hh_mm_ss_ff(bytes, tzinfo_pos + 1, end);
    parts.tz_offset = sign * (tz[0] * 3600 + tz[1] * 60 + tz[2]);
    parts.tz_microsecond = sign * tz[3];
    if rv != 0 { Err(()) } else { Ok((parts, true)) }
}

/// `new_timezone`'s range check, with `timedelta.__repr__` in the message.
fn check_offset_range(seconds: i64, microseconds: i64) -> Result<(), SemanticaError> {
    let total = seconds * 1_000_000 + microseconds;
    let day = 86_400_000_000i64;
    if total > -day && total < day {
        return Ok(());
    }
    let days = total.div_euclid(day);
    let rem = total.rem_euclid(day);
    let (secs, us) = (rem / 1_000_000, rem % 1_000_000);
    let mut args = Vec::new();
    if days != 0 {
        args.push(format!("days={days}"));
    }
    if secs != 0 {
        args.push(format!("seconds={secs}"));
    }
    if us != 0 {
        args.push(format!("microseconds={us}"));
    }
    if args.is_empty() {
        args.push("0".to_string());
    }
    Err(SemanticaError::Value(format!(
        "offset must be a timedelta strictly between -timedelta(hours=24) and timedelta(hours=24), not datetime.timedelta({}).",
        args.join(", ")
    )))
}

/// `datetime(year, month, day, hour, minute, second, microsecond)` with CPython's range checks.
fn new_datetime(
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: i64,
    microsecond: i64,
) -> Result<NaiveDateTime, SemanticaError> {
    let value_error = |msg: String| SemanticaError::Value(msg);
    if !(1..=9999).contains(&year) {
        return Err(value_error(format!("year {year} is out of range")));
    }
    if !(1..=12).contains(&month) {
        return Err(value_error("month must be in 1..12".to_string()));
    }
    let date = if (1..=31).contains(&day) {
        NaiveDate::from_ymd_opt(year as i32, month as u32, day as u32)
    } else {
        None
    }
    .ok_or_else(|| value_error("day is out of range for month".to_string()))?;
    if !(0..=23).contains(&hour) {
        return Err(value_error("hour must be in 0..23".to_string()));
    }
    if !(0..=59).contains(&minute) {
        return Err(value_error("minute must be in 0..59".to_string()));
    }
    if !(0..=59).contains(&second) {
        return Err(value_error("second must be in 0..59".to_string()));
    }
    if !(0..=999_999).contains(&microsecond) {
        return Err(value_error("microsecond must be in 0..999999".to_string()));
    }
    let time = NaiveTime::from_hms_micro_opt(
        hour as u32,
        minute as u32,
        second as u32,
        microsecond as u32,
    )
    .expect("time fields validated");
    Ok(date.and_time(time))
}

/// `datetime.isoformat()` of a parsed value (`T` separator), the form rdflib writes for a
/// normalised `xsd:dateTime` literal.
pub fn normalize_datetime_lexical(lexical: &str) -> Option<String> {
    datetime_fromisoformat(lexical)
        .ok()
        .map(|dt| dt.isoformat('T'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_forms() {
        let ok = |s: &str| datetime_fromisoformat(s).unwrap().isoformat('T');
        assert_eq!(ok("2026-01-01T00:00:00"), "2026-01-01T00:00:00");
        assert_eq!(ok("2026-01-01T00:00:00.5"), "2026-01-01T00:00:00.500000");
        assert_eq!(ok("2026-01-01T00:00:00Z"), "2026-01-01T00:00:00+00:00");
        assert_eq!(ok("2026W013"), "2025-12-31T00:00:00");
        assert!(datetime_fromisoformat("t0").is_err());
    }
}
