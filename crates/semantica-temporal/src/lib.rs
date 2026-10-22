//! Shared bounded-caller RDFLib/CPython temporal parsing, extracted from the
//! runtime literal normalizer. No query, graph or engineering-decision owner.
//! `DateTime` is the immutable value returned by the fixed fromisoformat domain;
//! parse failure is separate from the caller's lexical-preservation policy.
use chrono::{Datelike, NaiveDate, Weekday};
use std::cmp::Ordering;

mod calendar;
pub use calendar::{
    DurationContext, DurationDecimal, DurationError, DurationValue, normalize_duration,
};

mod duration;
pub use duration::{DurationParts, Timedelta, duration_parts, format_duration_tail};

/// The actual date cache produced by the fixed RDFLib date converter.
/// Timezone suffixes follow that converter; they are not dateTime offsets.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Date(NaiveDate);
impl Date {
    pub fn isoformat(self) -> String {
        date_text(self.0)
    }
    pub fn parts(self) -> (i32, u32, u32) {
        (self.0.year(), self.0.month(), self.0.day())
    }
}
pub fn parse_date(text: &str) -> Option<Date> {
    xsd_date(text).map(Date)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DateTime {
    date: NaiveDate,
    time: Time,
}
impl DateTime {
    pub fn isoformat(self) -> String {
        format!("{}T{}", date_text(self.date), self.time.text())
    }
    pub fn date_parts(self) -> (i32, u32, u32) {
        (self.date.year(), self.date.month(), self.date.day())
    }
    pub fn time_parts(self) -> [u32; 4] {
        [self.time.h, self.time.m, self.time.s, self.time.us]
    }
    pub fn offset_micros(self) -> Option<i64> {
        self.time.offset
    }
    fn comparison_micros(self) -> i64 {
        (i64::from(self.date.num_days_from_ce()) * 86_400
            + i64::from(self.time.h) * 3600
            + i64::from(self.time.m) * 60
            + i64::from(self.time.s))
            * 1_000_000
            + i64::from(self.time.us)
            - self.time.offset.unwrap_or(0)
    }
    /// None is Python's naive/aware ordered-comparison TypeError. Source
    /// Literal.__gt__ owns its total-order caster; raw values are not a total order.
    pub fn value_cmp(self, other: Self) -> Option<Ordering> {
        (self.time.offset.is_some() == other.time.offset.is_some())
            .then(|| self.comparison_micros().cmp(&other.comparison_micros()))
    }
    pub fn value_eq(self, other: Self) -> bool {
        self.value_cmp(other) == Some(Ordering::Equal)
    }
}
/// Exact difference of fixed-offset/naive values admitted by fromisoformat.
/// None preserves the original mixed-awareness TypeError; this is not a
/// timezone-database or arbitrary tzinfo/fold implementation.
pub fn datetime_difference(left: DateTime, right: DateTime) -> Option<Timedelta> {
    if left.time.offset.is_some() != right.time.offset.is_some() {
        return None;
    }
    Timedelta::from_micros(
        i128::from(left.comparison_micros()) - i128::from(right.comparison_micros()),
    )
}
/// Date caches have already applied the original converter's suffix policy.
pub fn date_difference(left: Date, right: Date) -> Timedelta {
    Timedelta::from_micros(
        i128::from(left.0.signed_duration_since(right.0).num_days()) * 86_400_000_000,
    )
    .expect("difference of year1..9999 dates is inside original timedelta bounds")
}
/// Callers enforce their own existing input budgets. No trimming or XML Schema
/// substitution: this is the fixed CPython datetime.fromisoformat syntax.
pub fn parse_datetime(text: &str) -> Option<DateTime> {
    datetime(text).map(|(date, time)| DateTime { date, time })
}
pub fn normalize_date(text: &str) -> Option<String> {
    parse_date(text).map(Date::isoformat)
}
pub fn normalize_time(text: &str) -> Option<String> {
    parse_time(text).map(Time::isoformat)
}

fn digits(s: &[u8], at: &mut usize, count: usize) -> Option<u32> {
    let part = s.get(*at..*at + count)?;
    if !part.iter().all(u8::is_ascii_digit) {
        return None;
    }
    *at += count;
    Some(part.iter().fold(0, |n, c| n * 10 + u32::from(c - b'0')))
}
fn iso_date(s: &str) -> Option<NaiveDate> {
    if ![7, 8, 10].contains(&s.len()) {
        return None;
    }
    let bytes = s.as_bytes();
    let mut at = 0;
    let year = digits(bytes, &mut at, 4)? as i32;
    if !(1..=9999).contains(&year) {
        return None;
    }
    let separated = bytes.get(at) == Some(&b'-');
    if separated {
        at += 1;
    }
    let date = if bytes.get(at) == Some(&b'W') {
        at += 1;
        let week = digits(bytes, &mut at, 2)?;
        let day = if at < s.len() {
            if separated {
                if bytes.get(at) != Some(&b'-') {
                    return None;
                }
                at += 1;
            }
            digits(bytes, &mut at, 1)?
        } else {
            1
        };
        let weekday = match day {
            1 => Weekday::Mon,
            2 => Weekday::Tue,
            3 => Weekday::Wed,
            4 => Weekday::Thu,
            5 => Weekday::Fri,
            6 => Weekday::Sat,
            7 => Weekday::Sun,
            _ => return None,
        };
        NaiveDate::from_isoywd_opt(year, week, weekday)?
    } else {
        let month = digits(bytes, &mut at, 2)?;
        if separated {
            if bytes.get(at) != Some(&b'-') {
                return None;
            }
            at += 1;
        }
        let day = digits(bytes, &mut at, 2)?;
        NaiveDate::from_ymd_opt(year, month, day)?
    };
    if at != s.len() || !(1..=9999).contains(&date.year()) {
        return None;
    }
    Some(date)
}
fn date_text(d: NaiveDate) -> String {
    format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day())
}
fn xsd_date(raw: &str) -> Option<NaiveDate> {
    let mut s = raw.strip_suffix(['Z', 'z']).unwrap_or(raw);
    if s.starts_with('-') {
        return None;
    }
    if let Some((before, _)) = s.split_once('T') {
        s = before;
    } else if let Some(at) = s.rfind('+').filter(|at| *at > 0) {
        s = &s[..at];
    } else if let Some((before, after)) = s.rsplit_once('-') {
        if after.contains(':') {
            s = before;
        }
    }
    if !s.contains('-') {
        return None;
    }
    iso_date(s)
}

#[derive(Clone, Copy, Default, Debug, Eq, PartialEq)]
pub struct Time {
    h: u32,
    m: u32,
    s: u32,
    us: u32,
    offset: Option<i64>,
}
/// The fixed CPython time.fromisoformat value, without a date or a timezone
/// database. Callers enforce input/output budgets and construction provenance.
pub fn parse_time(text: &str) -> Option<Time> {
    time(text.strip_prefix('T').unwrap_or(text))
}
impl Time {
    pub fn isoformat(self) -> String {
        self.text()
    }
    pub fn parts(self) -> [u32; 4] {
        [self.h, self.m, self.s, self.us]
    }
    pub fn offset_micros(self) -> Option<i64> {
        self.offset
    }
    /// Fixed CPython 3.13 time_richcompare compatibility, not datetime or
    /// elapsed-time arithmetic. Unlike datetime, unequal aware offsets use
    /// whole timedelta seconds; their fractional microseconds are not applied.
    /// Floor division preserves the original negative timedelta decomposition.
    /// None denotes the original naive/aware ordered-comparison TypeError.
    pub fn source_value_cmp(self, other: Self) -> Option<Ordering> {
        if self.offset == other.offset {
            return Some(self.parts().cmp(&other.parts()));
        }
        let (Some(a), Some(b)) = (self.offset, other.offset) else {
            return None;
        };
        let seconds = |t: Self| i64::from(t.h) * 3600 + i64::from(t.m) * 60 + i64::from(t.s);
        Some(
            (seconds(self) - a.div_euclid(1_000_000), self.us)
                .cmp(&(seconds(other) - b.div_euclid(1_000_000), other.us)),
        )
    }
    pub fn source_value_eq(self, other: Self) -> bool {
        self.source_value_cmp(other) == Some(Ordering::Equal)
    }
    fn text(self) -> String {
        let mut out = format!("{:02}:{:02}:{:02}", self.h, self.m, self.s);
        if self.us != 0 {
            out.push_str(&format!(".{:06}", self.us));
        }
        if let Some(offset) = self.offset {
            let n = offset.unsigned_abs();
            let seconds = n / 1_000_000;
            let fraction = n % 1_000_000;
            out.push_str(&format!(
                "{}{:02}:{:02}",
                if offset < 0 { '-' } else { '+' },
                seconds / 3600,
                (seconds / 60) % 60
            ));
            if seconds % 60 != 0 || fraction != 0 {
                out.push_str(&format!(":{:02}", seconds % 60));
                if fraction != 0 {
                    out.push_str(&format!(".{fraction:06}"));
                }
            }
        }
        out
    }
}
/// The boolean records trailing input, matching CPython's positive return
/// rather than conflating it with a conversion failure before a timezone.
fn parts(s: &str) -> Option<(Time, bool)> {
    let b = s.as_bytes();
    let mut at = 0;
    let mut values = [0; 3];
    let mut separated = false;
    for i in 0..3 {
        values[i] = digits(b, &mut at, 2)?;
        if at == b.len() {
            return Some((
                Time {
                    h: values[0],
                    m: values[1],
                    s: values[2],
                    ..Time::default()
                },
                false,
            ));
        }
        let c = b[at];
        at += 1;
        if i == 0 {
            separated = c == b':';
        }
        if at >= b.len() {
            return Some((
                Time {
                    h: values[0],
                    m: values[1],
                    s: values[2],
                    ..Time::default()
                },
                true,
            ));
        }
        if separated && c == b':' {
            continue;
        }
        if c == b'.' || c == b',' {
            let start = at;
            let n = (b.len() - start).min(6);
            let us = digits(b, &mut at, n)? * 10_u32.pow(6 - n as u32);
            while b.get(at).is_some_and(u8::is_ascii_digit) {
                at += 1;
            }
            return Some((
                Time {
                    h: values[0],
                    m: values[1],
                    s: values[2],
                    us,
                    offset: None,
                },
                at != b.len(),
            ));
        }
        if separated {
            return None;
        }
        at -= 1;
    }
    // After all three pairs, CPython treats remaining digits as a fractional
    // tail even when no dot was supplied.
    let start = at;
    let n = (b.len() - start).min(6);
    let us = digits(b, &mut at, n)? * 10_u32.pow(6 - n as u32);
    while b.get(at).is_some_and(u8::is_ascii_digit) {
        at += 1;
    }
    Some((
        Time {
            h: values[0],
            m: values[1],
            s: values[2],
            us,
            offset: None,
        },
        at != b.len(),
    ))
}
fn time(s: &str) -> Option<Time> {
    let zone = s.find(['Z', '+', '-']);
    let (main, tail) = parts(zone.map_or(s, |at| &s[..at]))?;
    if main.h > 23 || main.m > 59 || main.s > 59 {
        return None;
    }
    let offset = if let Some(at) = zone {
        let z = &s[at..];
        if z == "Z" {
            Some(0)
        } else {
            let sign = if z.starts_with('-') {
                -1
            } else if z.starts_with('+') {
                1
            } else {
                return None;
            };
            let (p, left) = parts(&z[1..])?;
            if left {
                return None;
            }
            let seconds = i64::from(p.h) * 3600 + i64::from(p.m) * 60 + i64::from(p.s);
            // CPython returns UTC immediately when whole seconds are zero.
            let offset = if seconds == 0 {
                0
            } else {
                sign * (seconds * 1_000_000 + i64::from(p.us))
            };
            if offset.abs() >= 86_400_000_000 {
                return None;
            }
            Some(offset)
        }
    } else {
        if tail {
            return None;
        }
        None
    };
    Some(Time { offset, ..main })
}
fn datetime(s: &str) -> Option<(NaiveDate, Time)> {
    let b = s.as_bytes();
    if b.len() < 7 {
        return None;
    }
    let sep = if b.len() == 7 {
        7
    } else if b[4] == b'-' {
        if b.get(5) == Some(&b'W') {
            if b.len() < 8 {
                return None;
            }
            if b.len() > 8 && b[8] == b'-' {
                if b.len() == 9 {
                    return None;
                }
                if b.len() > 10 && b[10].is_ascii_digit() {
                    8
                } else {
                    10
                }
            } else {
                8
            }
        } else {
            10
        }
    } else if b[4] == b'W' {
        let mut at = 7;
        while b.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
        if at < 9 {
            at
        } else if at % 2 == 0 {
            7
        } else {
            8
        }
    } else {
        8
    };
    let d = iso_date(s.get(..sep)?)?;
    let remainder = s.get(sep..)?;
    let t = if remainder.is_empty() {
        Time::default()
    } else {
        time(&remainder[remainder.chars().next()?.len_utf8()..])?
    };
    Some((d, t))
}
