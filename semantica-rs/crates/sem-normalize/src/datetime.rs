//! Python `datetime.datetime` values with the `tzinfo` kinds the date normalizer produces:
//! `datetime.timezone` (UTC or a fixed offset), `dateutil.tz.tzoffset` (a named offset, not
//! range-checked until used), `zoneinfo.ZoneInfo` and `dateutil.tz.tzlocal`.
//!
//! Arithmetic and conversions follow CPython 3.11: wall-clock arithmetic, `fold`, the
//! 1..=9999 year range, `astimezone` treating naive values as local time, and `isoformat` /
//! `strftime` output.

use std::fmt;
use std::sync::Arc;

use chrono::{
    Datelike, Duration, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Timelike,
};
use sem_utils::SemanticaError;
use sem_utils::pycompat::time::PyDateTime;

use crate::zoneinfo::{self, ZoneInfo};

/// The time zone of an aware [`DateTime`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TzInfo {
    /// `datetime.timezone.utc` (also `dateutil.tz.UTC`).
    Utc,
    /// `datetime.timezone(timedelta(seconds=n))`.
    Fixed(i32),
    /// `dateutil.tz.tzoffset(name, seconds)` (any offset; an offset of a day or more fails
    /// when used, as in Python).
    TzOffset {
        /// `tzname()`.
        name: Option<String>,
        /// Offset east of UTC in seconds.
        offset: i64,
    },
    /// `zoneinfo.ZoneInfo(key)`.
    Zone(Arc<ZoneInfo>),
    /// `dateutil.tz.tzlocal()`.
    Local,
}

/// A Python `datetime` (naive when `tzinfo` is `None`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateTime {
    /// The wall-clock fields (year .. microsecond).
    pub wall: NaiveDateTime,
    /// `fold` (0 or 1).
    pub fold: u8,
    /// `tzinfo`.
    pub tzinfo: Option<TzInfo>,
}

impl From<NaiveDateTime> for DateTime {
    fn from(wall: NaiveDateTime) -> Self {
        DateTime::naive(wall)
    }
}

impl From<chrono::DateTime<FixedOffset>> for DateTime {
    fn from(value: chrono::DateTime<FixedOffset>) -> Self {
        DateTime::aware(
            value.naive_local(),
            TzInfo::Fixed(value.offset().local_minus_utc()),
        )
    }
}

impl From<chrono::DateTime<chrono::Utc>> for DateTime {
    fn from(value: chrono::DateTime<chrono::Utc>) -> Self {
        DateTime::aware(value.naive_utc(), TzInfo::Utc)
    }
}

impl From<PyDateTime> for DateTime {
    fn from(value: PyDateTime) -> Self {
        match value {
            PyDateTime::Naive(wall) => DateTime::naive(wall),
            PyDateTime::Aware(dt) => DateTime::from(dt),
        }
    }
}

fn overflow() -> SemanticaError {
    SemanticaError::Overflow("date value out of range".to_string())
}

/// The range check CPython applies to `tzinfo.utcoffset()` results.
fn check_offset(offset: i64) -> Result<i64, SemanticaError> {
    if offset.abs() >= 86400 {
        return Err(SemanticaError::Value(
            "offset must be a timedelta strictly between -timedelta(hours=24) and timedelta(hours=24)."
                .to_string(),
        ));
    }
    Ok(offset)
}

/// `+HH:MM[:SS]` (`isoformat`) or `+HHMM[SS]` (`%z`).
fn format_offset(seconds: i64, colons: bool) -> String {
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

fn in_range(wall: &NaiveDateTime) -> bool {
    (1..=9999).contains(&wall.year())
}

/// The state `dateutil.tz.tzlocal()` captures from the `time` module.
struct TzLocal {
    std_offset: i64,
    dst_offset: i64,
    hasdst: bool,
    tznames: (String, String),
    timezone: i64,
}

fn tzlocal() -> TzLocal {
    let now = chrono::Utc::now().timestamp();
    let (timezone, altzone, daylight, tznames) = zoneinfo::time_module_zone(now);
    let std_offset = -timezone;
    let dst_offset = if daylight { -altzone } else { std_offset };
    TzLocal {
        std_offset,
        dst_offset,
        hasdst: dst_offset != std_offset,
        tznames,
        timezone,
    }
}

impl TzLocal {
    fn naive_is_dst(&self, wall: &NaiveDateTime) -> bool {
        let ts = zoneinfo::wall_timestamp(wall);
        zoneinfo::localtime(ts + self.timezone).1.isdst
    }

    fn isdst(&self, wall: &NaiveDateTime, fold: u8) -> bool {
        if !self.hasdst {
            return false;
        }
        let dstval = self.naive_is_dst(wall);
        let dst_saved = self.dst_offset - self.std_offset;
        let ambiguous =
            !dstval && dstval != self.naive_is_dst(&(*wall - Duration::seconds(dst_saved)));
        if ambiguous {
            return fold == 0;
        }
        dstval
    }
}

/// `time.tzname` of the local zone (what `dateutil` compares parsed zone names against).
pub(crate) fn local_tznames() -> (String, String) {
    tzlocal().tznames
}

impl DateTime {
    /// A naive datetime.
    pub fn naive(wall: NaiveDateTime) -> DateTime {
        DateTime {
            wall,
            fold: 0,
            tzinfo: None,
        }
    }

    /// An aware datetime (`fold = 0`).
    pub fn aware(wall: NaiveDateTime, tzinfo: TzInfo) -> DateTime {
        DateTime {
            wall,
            fold: 0,
            tzinfo: Some(tzinfo),
        }
    }

    /// `datetime(year, month, day, hour, minute, second)` (naive).
    pub fn from_ymd_hms(
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
        second: u32,
    ) -> Option<DateTime> {
        NaiveDate::from_ymd_opt(year, month, day)
            .and_then(|d| d.and_hms_opt(hour, minute, second))
            .map(DateTime::naive)
    }

    /// `datetime.now()` (naive local time).
    pub fn now() -> DateTime {
        DateTime::naive(chrono::Local::now().naive_local())
    }

    /// `dt.replace(tzinfo=tz)`.
    pub fn replace_tzinfo(&self, tzinfo: Option<TzInfo>) -> DateTime {
        DateTime {
            wall: self.wall,
            fold: self.fold,
            tzinfo,
        }
    }

    /// `dt.utcoffset()` in seconds (`None` for naive values); out-of-range `tzoffset` values
    /// raise Python's `ValueError`.
    pub fn utcoffset(&self) -> Result<Option<i64>, SemanticaError> {
        let Some(tz) = &self.tzinfo else {
            return Ok(None);
        };
        let offset = match tz {
            TzInfo::Utc => 0,
            TzInfo::Fixed(n) => i64::from(*n),
            TzInfo::TzOffset { offset, .. } => *offset,
            TzInfo::Zone(zone) => zone.utcoffset(&self.wall, self.fold),
            TzInfo::Local => {
                let local = tzlocal();
                if local.isdst(&self.wall, self.fold) {
                    local.dst_offset
                } else {
                    local.std_offset
                }
            }
        };
        check_offset(offset).map(Some)
    }

    /// `dt.tzname()`.
    pub fn tzname(&self) -> Option<String> {
        match self.tzinfo.as_ref()? {
            TzInfo::Utc => Some("UTC".to_string()),
            TzInfo::Fixed(0) => Some("UTC".to_string()),
            TzInfo::Fixed(n) => Some(format!("UTC{}", format_offset(i64::from(*n), true))),
            TzInfo::TzOffset { name, .. } => name.clone(),
            TzInfo::Zone(zone) => Some(zone.tzname(&self.wall, self.fold)),
            TzInfo::Local => {
                let local = tzlocal();
                let names = &local.tznames;
                Some(if local.isdst(&self.wall, self.fold) {
                    names.1.clone()
                } else {
                    names.0.clone()
                })
            }
        }
    }

    /// `dt.isoformat(sep)`.
    pub fn isoformat_sep(&self, sep: char) -> Result<String, SemanticaError> {
        let mut out = format!("{}{sep}{}", self.date_isoformat(), self.time_isoformat());
        if let Some(offset) = self.utcoffset()? {
            out.push_str(&format_offset(offset, true));
        }
        Ok(out)
    }

    /// `dt.isoformat()`.
    pub fn isoformat(&self) -> Result<String, SemanticaError> {
        self.isoformat_sep('T')
    }

    /// `dt.date().isoformat()`.
    pub fn date_isoformat(&self) -> String {
        format!(
            "{:04}-{:02}-{:02}",
            self.wall.year(),
            self.wall.month(),
            self.wall.day()
        )
    }

    /// `dt.time().isoformat()` (`HH:MM:SS[.ffffff]`).
    pub fn time_isoformat(&self) -> String {
        let micros = self.wall.nanosecond() / 1000;
        let mut out = format!(
            "{:02}:{:02}:{:02}",
            self.wall.hour(),
            self.wall.minute(),
            self.wall.second()
        );
        if micros != 0 {
            out.push_str(&format!(".{micros:06}"));
        }
        out
    }

    /// `dt.strftime(format)`.
    pub fn strftime(&self, format: &str) -> Result<String, SemanticaError> {
        let offset = self.utcoffset()?;
        let tzname = if offset.is_some() {
            self.tzname().unwrap_or_default()
        } else {
            String::new()
        };
        // Expand %Z ourselves (zone abbreviations), then let the shared strftime do the rest.
        let mut prepared = String::with_capacity(format.len());
        let mut chars = format.chars().peekable();
        while let Some(c) = chars.next() {
            if c != '%' {
                prepared.push(c);
                continue;
            }
            match chars.next() {
                Some('Z') => prepared.push_str(&tzname.replace('%', "%%")),
                Some(other) => {
                    prepared.push('%');
                    prepared.push(other);
                }
                None => prepared.push('%'),
            }
        }
        let py = match offset {
            None => PyDateTime::Naive(self.wall),
            Some(offset) => {
                let fixed = FixedOffset::east_opt(offset as i32)
                    .expect("offset checked to be within a day");
                PyDateTime::Aware(
                    fixed
                        .from_local_datetime(&self.wall)
                        .single()
                        .expect("fixed offsets are unambiguous"),
                )
            }
        };
        Ok(sem_utils::pycompat::time::strftime(&py, &prepared))
    }

    /// `dt + timedelta(seconds=..)` (wall-clock arithmetic; `OverflowError` outside years
    /// 1..9999).
    pub fn add(&self, delta: Duration) -> Result<DateTime, SemanticaError> {
        let wall = self.wall.checked_add_signed(delta).ok_or_else(overflow)?;
        if !in_range(&wall) {
            return Err(overflow());
        }
        Ok(DateTime {
            wall,
            fold: 0,
            tzinfo: self.tzinfo.clone(),
        })
    }

    /// The UTC instant (as a UTC wall time) of this value; naive values are local time, as in
    /// `astimezone()`.
    fn utc_wall(&self) -> Result<NaiveDateTime, SemanticaError> {
        let offset = match self.utcoffset()? {
            Some(offset) => offset,
            None => local_offset_for_naive(&self.wall, self.fold),
        };
        self.wall
            .checked_sub_signed(Duration::seconds(offset))
            .ok_or_else(overflow)
    }

    /// `dt.astimezone(tz)`.
    pub fn astimezone(&self, tz: &TzInfo) -> Result<DateTime, SemanticaError> {
        if self.tzinfo.as_ref() == Some(tz) {
            return Ok(self.clone());
        }
        let utc = self.utc_wall()?;
        let (wall, fold) = match tz {
            TzInfo::Utc => (utc, 0),
            TzInfo::Fixed(n) => (utc + Duration::seconds(i64::from(*n)), 0),
            TzInfo::TzOffset { offset, .. } => {
                check_offset(*offset)?;
                (utc + Duration::seconds(*offset), 0)
            }
            TzInfo::Zone(zone) => {
                let (offset, fold, _) = zone.fromutc(&utc);
                (utc + Duration::seconds(offset), fold)
            }
            TzInfo::Local => {
                let (offset, fold, _) = zoneinfo::local_zone().fromutc(&utc);
                (utc + Duration::seconds(offset), fold)
            }
        };
        if !in_range(&wall) {
            return Err(overflow());
        }
        Ok(DateTime {
            wall,
            fold,
            tzinfo: Some(tz.clone()),
        })
    }

    /// The `date` part.
    pub fn date(&self) -> NaiveDate {
        self.wall.date()
    }

    /// The `time` part.
    pub fn time(&self) -> NaiveTime {
        self.wall.time()
    }
}

/// CPython's `local()` / `_mktime` search: the local UTC offset of a naive wall time
/// (`fold` picks between the two readings of an ambiguous time).
fn local_offset_for_naive(wall: &NaiveDateTime, fold: u8) -> i64 {
    let t = zoneinfo::wall_timestamp(wall);
    let local = |u: i64| -> i64 {
        let (lt, _) = zoneinfo::localtime(u);
        zoneinfo::wall_timestamp(&lt)
    };
    const MAX_FOLD_SECONDS: i64 = 24 * 3600;
    let a = local(t) - t;
    let u1 = t - a;
    let t1 = local(u1);
    let b;
    let u = 'solve: {
        if t1 == t {
            let u2 = u1
                + if fold == 0 {
                    -MAX_FOLD_SECONDS
                } else {
                    MAX_FOLD_SECONDS
                };
            let b2 = local(u2) - u2;
            if a == b2 {
                break 'solve u1;
            }
            b = b2;
        } else {
            b = t1 - u1;
        }
        let u2 = t - b;
        let t2 = local(u2);
        if t2 == t {
            u2
        } else if t1 == t {
            u1
        } else if fold == 0 {
            u1.max(u2)
        } else {
            u1.min(u2)
        }
    };
    // `astimezone` builds `timezone(gmtoff)` from `localtime(u)`.
    local(u) - u
}

impl fmt::Display for DateTime {
    /// `str(dt)` (`isoformat(' ')`; an unusable offset prints without it).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.isoformat_sep(' ') {
            Ok(text) => f.write_str(&text),
            Err(_) => write!(f, "{} {}", self.date_isoformat(), self.time_isoformat()),
        }
    }
}
