//! A port of CPython 3.11's `zoneinfo`: TZif files from the system time zone database
//! (`TZPATH`), POSIX TZ rule strings for times after the last transition, `utcoffset` /
//! `tzname` lookups with `fold`, and `fromutc`. Also the local time zone the C library would
//! use (`TZ`, else `/etc/localtime`), with `time.localtime` / `time.tzname` semantics.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::{Datelike, NaiveDate, NaiveDateTime, Timelike};
use once_cell::sync::Lazy;
use regex::Regex;

/// Seconds between 0001-01-01 and 1970-01-01 in days (`date(1970, 1, 1).toordinal()`).
const EPOCHORDINAL: i64 = 719_163;

/// Python's default `zoneinfo.TZPATH` on the reference platform.
pub const DEFAULT_TZPATH: [&str; 4] = [
    "/usr/share/zoneinfo",
    "/usr/lib/zoneinfo",
    "/usr/share/lib/zoneinfo",
    "/etc/zoneinfo",
];

static TZPATH: Lazy<Mutex<Vec<PathBuf>>> = Lazy::new(|| Mutex::new(initial_tzpath()));
static CACHE: Lazy<Mutex<HashMap<String, Arc<ZoneInfo>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn initial_tzpath() -> Vec<PathBuf> {
    match std::env::var("PYTHONTZPATH") {
        Ok(value) if !value.is_empty() => value
            .split(':')
            .filter(|p| Path::new(p).is_absolute())
            .map(PathBuf::from)
            .collect(),
        _ => DEFAULT_TZPATH.iter().map(PathBuf::from).collect(),
    }
}

/// `zoneinfo.reset_tzpath(to)`: replace the search path (`None` restores the default) and
/// clear the zone cache.
pub fn reset_tzpath(to: Option<&[PathBuf]>) {
    let mut tzpath = TZPATH.lock().unwrap_or_else(|e| e.into_inner());
    *tzpath = match to {
        Some(paths) => paths.to_vec(),
        None => initial_tzpath(),
    };
    CACHE.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

/// The current `zoneinfo.TZPATH`.
pub fn tzpath() -> Vec<PathBuf> {
    TZPATH.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// One local time type (`_ttinfo`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TtInfo {
    /// UTC offset in seconds.
    pub utcoff: i64,
    /// Whether this is daylight saving time.
    pub isdst: bool,
    /// The abbreviation (`tzname()`).
    pub tzname: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DstRule {
    /// `Jn` (julian, 1-based, no leap day) or `n` (0-based).
    Day { d: i64, julian: bool, secs: i64 },
    /// `Mm.w.d`.
    Calendar { m: u32, w: u32, d: u32, secs: i64 },
}

fn post_epoch_days_before_year(year: i64) -> i64 {
    let y = year - 1;
    y * 365 + y.div_euclid(4) - y.div_euclid(100) + y.div_euclid(400) - EPOCHORDINAL
}

fn is_leap(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

/// `calendar.monthrange(year, month)`: (weekday of the 1st, Monday = 0; number of days).
pub(crate) fn monthrange(year: i64, month: u32) -> (u32, u32) {
    const MDAYS: [u32; 13] = [0, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let days = if month == 2 && is_leap(year) {
        29
    } else {
        MDAYS[month as usize]
    };
    // calendar.weekday maps years outside 1..9999 into the 400-year cycle.
    let y = if (1..=9999).contains(&year) {
        year
    } else {
        2000 + year.rem_euclid(400)
    };
    let ordinal = ymd_to_ordinal(y, month, 1);
    let weekday = ((ordinal + 6) % 7) as u32; // ordinal 1 (0001-01-01) is a Monday
    (weekday, days)
}

/// `date(year, month, day).toordinal()` (proleptic Gregorian, any year).
pub(crate) fn ymd_to_ordinal(year: i64, month: u32, day: u32) -> i64 {
    const DAYS_BEFORE_MONTH: [i64; 13] = [0, 0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let y = year - 1;
    let mut days = y * 365 + y.div_euclid(4) - y.div_euclid(100) + y.div_euclid(400);
    days += DAYS_BEFORE_MONTH[month as usize];
    if month > 2 && is_leap(year) {
        days += 1;
    }
    days + i64::from(day)
}

impl DstRule {
    fn year_to_epoch(&self, year: i64) -> i64 {
        match *self {
            DstRule::Day { d, julian, secs } => {
                let mut d = d;
                if julian && d >= 59 && is_leap(year) {
                    d += 1;
                }
                (post_epoch_days_before_year(year) + d) * 86400 + secs
            }
            DstRule::Calendar { m, w, d, secs } => {
                let (first_day, days_in_month) = monthrange(year, m);
                let mut month_day = (i64::from(d) - (i64::from(first_day) + 1)).rem_euclid(7) + 1;
                month_day += (i64::from(w) - 1) * 7;
                if month_day > i64::from(days_in_month) {
                    month_day -= 7;
                }
                const DAYS_BEFORE_MONTH: [i64; 13] =
                    [-1, 0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
                let ordinal = post_epoch_days_before_year(year)
                    + DAYS_BEFORE_MONTH[m as usize]
                    + i64::from(m > 2 && is_leap(year))
                    + month_day;
                ordinal * 86400 + secs
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TzStr {
    std: TtInfo,
    dst: TtInfo,
    start: DstRule,
    end: DstRule,
    dst_diff: i64,
}

impl TzStr {
    fn transitions(&self, year: i64) -> (i64, i64) {
        (self.start.year_to_epoch(year), self.end.year_to_epoch(year))
    }

    fn get_trans_info(&self, ts: i64, year: i64, fold: u8) -> &TtInfo {
        let (mut start, mut end) = self.transitions(year);
        if (fold == 1) == (self.dst_diff >= 0) {
            end -= self.dst_diff;
        } else {
            start += self.dst_diff;
        }
        let isdst = if start < end {
            start <= ts && ts < end
        } else {
            !(end <= ts && ts < start)
        };
        if isdst { &self.dst } else { &self.std }
    }

    fn get_trans_info_fromutc(&self, ts: i64, year: i64) -> (&TtInfo, u8) {
        let (mut start, mut end) = self.transitions(year);
        start -= self.std.utcoff;
        end -= self.dst.utcoff;
        let isdst = if start < end {
            start <= ts && ts < end
        } else {
            !(end <= ts && ts < start)
        };
        let (ambig_start, ambig_end) = if self.dst_diff > 0 {
            (end, end + self.dst_diff)
        } else {
            (start, start - self.dst_diff)
        };
        let fold = u8::from(ambig_start <= ts && ts < ambig_end);
        (if isdst { &self.dst } else { &self.std }, fold)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TzAfter {
    Fixed(TtInfo),
    Rule(TzStr),
}

/// A time zone loaded from a TZif file (`zoneinfo.ZoneInfo`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoneInfo {
    key: Option<String>,
    trans_utc: Vec<i64>,
    trans_local: [Vec<i64>; 2],
    ttinfos: Vec<TtInfo>,
    tti_before: Option<TtInfo>,
    tz_after: TzAfter,
}

/// Why a zone could not be loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZoneError {
    /// The key is not a valid relative path (`ValueError`).
    InvalidKey(String),
    /// No such zone (`ZoneInfoNotFoundError`).
    NotFound(String),
    /// The file is not valid TZif (`ValueError`).
    Invalid(String),
}

impl std::fmt::Display for ZoneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ZoneError::InvalidKey(m) | ZoneError::Invalid(m) => f.write_str(m),
            ZoneError::NotFound(key) => write!(f, "'No time zone found with key {key}'"),
        }
    }
}

fn local_timestamp(wall: &NaiveDateTime) -> i64 {
    let date = wall.date();
    let ordinal = ymd_to_ordinal(i64::from(date.year()), date.month(), date.day());
    (ordinal - EPOCHORDINAL) * 86400
        + i64::from(wall.hour()) * 3600
        + i64::from(wall.minute()) * 60
        + i64::from(wall.second())
}

fn bisect_right(list: &[i64], x: i64) -> usize {
    list.partition_point(|&v| v <= x)
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], ZoneError> {
        if self.pos + n > self.data.len() {
            return Err(ZoneError::Invalid(
                "Invalid TZif file: truncated".to_string(),
            ));
        }
        let slice = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    fn u32(&mut self) -> Result<u32, ZoneError> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
}

struct Header {
    version: u8,
    isutcnt: usize,
    isstdcnt: usize,
    leapcnt: usize,
    timecnt: usize,
    typecnt: usize,
    charcnt: usize,
}

fn read_header(r: &mut Reader<'_>) -> Result<Header, ZoneError> {
    let magic = r.take(4)?;
    if magic != b"TZif" {
        return Err(ZoneError::Invalid(format!(
            "Invalid TZif file: magic not found: {magic:?}"
        )));
    }
    let version_byte = r.take(1)?[0];
    let version = if version_byte == 0 {
        1
    } else {
        version_byte.wrapping_sub(b'0')
    };
    r.take(15)?;
    Ok(Header {
        version,
        isutcnt: r.u32()? as usize,
        isstdcnt: r.u32()? as usize,
        leapcnt: r.u32()? as usize,
        timecnt: r.u32()? as usize,
        typecnt: r.u32()? as usize,
        charcnt: r.u32()? as usize,
    })
}

impl ZoneInfo {
    /// `ZoneInfo(key)`: load (and cache) the zone from `TZPATH`.
    pub fn new(key: &str) -> Result<Arc<ZoneInfo>, ZoneError> {
        if let Some(zone) = CACHE.lock().unwrap_or_else(|e| e.into_inner()).get(key) {
            return Ok(zone.clone());
        }
        validate_key(key)?;
        let path = tzpath()
            .into_iter()
            .map(|dir| dir.join(key))
            .find(|p| p.is_file());
        let zone = match path {
            Some(path) => {
                let data =
                    std::fs::read(&path).map_err(|_| ZoneError::NotFound(key.to_string()))?;
                ZoneInfo::from_bytes(&data, Some(key))?
            }
            None if key == "UTC" || key == "Etc/UTC" => ZoneInfo::fixed(0, "UTC", Some(key)),
            None => return Err(ZoneError::NotFound(key.to_string())),
        };
        let zone = Arc::new(zone);
        CACHE
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key.to_string(), zone.clone());
        Ok(zone)
    }

    /// A zone with one fixed offset and no transitions.
    pub fn fixed(utcoff: i64, tzname: &str, key: Option<&str>) -> ZoneInfo {
        ZoneInfo {
            key: key.map(str::to_string),
            trans_utc: Vec::new(),
            trans_local: [Vec::new(), Vec::new()],
            ttinfos: Vec::new(),
            tti_before: None,
            tz_after: TzAfter::Fixed(TtInfo {
                utcoff,
                isdst: false,
                tzname: tzname.to_string(),
            }),
        }
    }

    /// A zone defined only by a POSIX TZ string (the `TZ` environment variable form).
    pub fn from_tz_string(tz: &str) -> Result<ZoneInfo, ZoneError> {
        let tz_after = parse_tz_str(tz).map_err(ZoneError::Invalid)?;
        Ok(ZoneInfo {
            key: None,
            trans_utc: Vec::new(),
            trans_local: [Vec::new(), Vec::new()],
            ttinfos: Vec::new(),
            tti_before: None,
            tz_after,
        })
    }

    /// `ZoneInfo.from_file`: parse TZif data.
    pub fn from_bytes(data: &[u8], key: Option<&str>) -> Result<ZoneInfo, ZoneError> {
        let mut r = Reader { data, pos: 0 };
        let mut header = read_header(&mut r)?;
        let time_size = if header.version == 1 {
            4
        } else {
            let skip = header.timecnt * 5
                + header.typecnt * 6
                + header.charcnt
                + header.leapcnt * 8
                + header.isstdcnt
                + header.isutcnt;
            r.take(skip)?;
            header = read_header(&mut r)?;
            8
        };
        let mut trans_utc = Vec::with_capacity(header.timecnt);
        for _ in 0..header.timecnt {
            let b = r.take(time_size)?;
            let t = if time_size == 4 {
                i64::from(i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
            } else {
                i64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
            };
            trans_utc.push(t);
        }
        let trans_idx: Vec<usize> = r
            .take(header.timecnt)?
            .iter()
            .map(|&b| b as usize)
            .collect();
        let mut utcoff = Vec::with_capacity(header.typecnt);
        let mut isdst = Vec::with_capacity(header.typecnt);
        let mut abbrind = Vec::with_capacity(header.typecnt);
        for _ in 0..header.typecnt {
            let b = r.take(6)?;
            utcoff.push(i64::from(i32::from_be_bytes([b[0], b[1], b[2], b[3]])));
            isdst.push(b[4] as i8 != 0);
            abbrind.push(b[5] as usize);
        }
        let abbr_chars = r.take(header.charcnt)?.to_vec();
        let abbr: Vec<String> = abbrind
            .iter()
            .map(|&idx| {
                let start = idx.min(abbr_chars.len());
                let end = abbr_chars[start..]
                    .iter()
                    .position(|&b| b == 0)
                    .map(|p| start + p)
                    .unwrap_or(abbr_chars.len());
                String::from_utf8_lossy(&abbr_chars[start..end]).into_owned()
            })
            .collect();
        let tz_str = if header.version >= 2 {
            let leap_size = if time_size == 8 { 12 } else { 8 };
            r.take(header.leapcnt * leap_size + header.isstdcnt + header.isutcnt)?;
            let rest = &data[r.pos..];
            // "\n<tz string>\n"
            let rest = rest.strip_prefix(b"\n").unwrap_or(rest);
            let end = rest.iter().position(|&b| b == b'\n').unwrap_or(rest.len());
            Some(String::from_utf8_lossy(&rest[..end]).into_owned())
        } else {
            None
        };
        if trans_idx.iter().any(|&i| i >= header.typecnt) {
            return Err(ZoneError::Invalid(
                "Invalid TZif file: bad type index".to_string(),
            ));
        }

        let ttinfo_list: Vec<TtInfo> = (0..header.typecnt)
            .map(|i| TtInfo {
                utcoff: utcoff[i],
                isdst: isdst[i],
                tzname: abbr[i].clone(),
            })
            .collect();
        let trans_local = ts_to_local(&trans_idx, &trans_utc, &utcoff);
        let ttinfos: Vec<TtInfo> = trans_idx.iter().map(|&i| ttinfo_list[i].clone()).collect();
        let tti_before = isdst
            .iter()
            .position(|d| !d)
            .map(|i| ttinfo_list[i].clone())
            .or_else(|| ttinfos.first().cloned());
        let tz_after = match tz_str.filter(|s| !s.is_empty()) {
            Some(s) => parse_tz_str(&s).map_err(ZoneError::Invalid)?,
            None => {
                if ttinfos.is_empty() && ttinfo_list.is_empty() {
                    return Err(ZoneError::Invalid(
                        "No time zone information found.".to_string(),
                    ));
                }
                TzAfter::Fixed(
                    ttinfos
                        .last()
                        .cloned()
                        .unwrap_or_else(|| ttinfo_list[ttinfo_list.len() - 1].clone()),
                )
            }
        };
        Ok(ZoneInfo {
            key: key.map(str::to_string),
            trans_utc,
            trans_local,
            ttinfos,
            tti_before,
            tz_after,
        })
    }

    /// The zone key (`str(zone)`).
    pub fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    fn find_trans(&self, wall: &NaiveDateTime, fold: u8) -> &TtInfo {
        let ts = local_timestamp(wall);
        let lt = &self.trans_local[usize::from(fold.min(1))];
        if !lt.is_empty() && ts < lt[0] {
            return self
                .tti_before
                .as_ref()
                .expect("zones with transitions have a type before them");
        }
        if lt.is_empty() || ts > lt[lt.len() - 1] {
            return match &self.tz_after {
                TzAfter::Rule(rule) => rule.get_trans_info(ts, i64::from(wall.year()), fold),
                TzAfter::Fixed(tti) => tti,
            };
        }
        let idx = bisect_right(lt, ts) - 1;
        &self.ttinfos[idx]
    }

    /// `utcoffset(dt)` in seconds for a wall time.
    pub fn utcoffset(&self, wall: &NaiveDateTime, fold: u8) -> i64 {
        self.find_trans(wall, fold).utcoff
    }

    /// `tzname(dt)` for a wall time.
    pub fn tzname(&self, wall: &NaiveDateTime, fold: u8) -> String {
        self.find_trans(wall, fold).tzname.clone()
    }

    /// The local time type for a wall time.
    pub fn ttinfo(&self, wall: &NaiveDateTime, fold: u8) -> TtInfo {
        self.find_trans(wall, fold).clone()
    }

    /// `fromutc(dt)`: the wall time (and fold) for a UTC wall time.
    pub fn fromutc(&self, utc: &NaiveDateTime) -> (i64, u8, TtInfo) {
        let timestamp = local_timestamp(utc);
        let num_trans = self.trans_utc.len();
        let (tti, fold): (&TtInfo, u8) = if num_trans >= 1 && timestamp < self.trans_utc[0] {
            (
                self.tti_before
                    .as_ref()
                    .expect("zones with transitions have a type before them"),
                0,
            )
        } else if (num_trans == 0 || timestamp > self.trans_utc[num_trans - 1])
            && matches!(self.tz_after, TzAfter::Rule(_))
        {
            let TzAfter::Rule(rule) = &self.tz_after else {
                unreachable!()
            };
            rule.get_trans_info_fromutc(timestamp, i64::from(utc.year()))
        } else if num_trans == 0 {
            let TzAfter::Fixed(tti) = &self.tz_after else {
                unreachable!()
            };
            (tti, 0)
        } else {
            let idx = bisect_right(&self.trans_utc, timestamp);
            let (tti_prev, tti): (&TtInfo, &TtInfo) =
                if num_trans > 1 && timestamp >= self.trans_utc[1] {
                    (&self.ttinfos[idx - 2], &self.ttinfos[idx - 1])
                } else if timestamp > self.trans_utc[num_trans - 1] {
                    let TzAfter::Fixed(after) = &self.tz_after else {
                        unreachable!()
                    };
                    (&self.ttinfos[num_trans - 1], after)
                } else {
                    (
                        self.tti_before
                            .as_ref()
                            .expect("zones with transitions have a type before them"),
                        &self.ttinfos[0],
                    )
                };
            let shift = tti_prev.utcoff - tti.utcoff;
            let fold = u8::from(shift > timestamp - self.trans_utc[idx - 1]);
            (tti, fold)
        };
        (tti.utcoff, fold, tti.clone())
    }
}

fn ts_to_local(trans_idx: &[usize], trans_utc: &[i64], utcoffsets: &[i64]) -> [Vec<i64>; 2] {
    if trans_utc.is_empty() {
        return [Vec::new(), Vec::new()];
    }
    let mut wall0 = trans_utc.to_vec();
    let mut wall1 = trans_utc.to_vec();
    let (mut offset_0, mut offset_1) = if utcoffsets.len() > 1 {
        (utcoffsets[0], utcoffsets[trans_idx[0]])
    } else {
        (utcoffsets[0], utcoffsets[0])
    };
    if offset_1 > offset_0 {
        std::mem::swap(&mut offset_0, &mut offset_1);
    }
    wall0[0] += offset_0;
    wall1[0] += offset_1;
    for i in 1..trans_idx.len() {
        let mut offset_0 = utcoffsets[trans_idx[i - 1]];
        let mut offset_1 = utcoffsets[trans_idx[i]];
        if offset_1 > offset_0 {
            std::mem::swap(&mut offset_0, &mut offset_1);
        }
        wall0[i] += offset_0;
        wall1[i] += offset_1;
    }
    [wall0, wall1]
}

fn validate_key(key: &str) -> Result<(), ZoneError> {
    if key.starts_with('/') {
        return Err(ZoneError::InvalidKey(format!(
            "ZoneInfo keys may not be absolute paths, got: {key}"
        )));
    }
    // os.path.normpath must not change the key's length.
    let normalized = normpath(key);
    if normalized.chars().count() != key.chars().count() {
        return Err(ZoneError::InvalidKey(format!(
            "ZoneInfo keys must be normalized relative paths, got: {key}"
        )));
    }
    if normalized == ".." || normalized.starts_with("../") {
        return Err(ZoneError::InvalidKey(format!(
            "ZoneInfo keys must refer to subdirectories of TZPATH, got: {key}"
        )));
    }
    Ok(())
}

/// `posixpath.normpath` for relative paths.
fn normpath(path: &str) -> String {
    if path.is_empty() {
        return ".".to_string();
    }
    let mut parts: Vec<&str> = Vec::new();
    for comp in path.split('/') {
        if comp.is_empty() || comp == "." {
            continue;
        }
        if comp == ".." && parts.last().is_some_and(|p| *p != "..") {
            parts.pop();
        } else {
            parts.push(comp);
        }
    }
    let joined = parts.join("/");
    if joined.is_empty() {
        ".".to_string()
    } else {
        joined
    }
}

static TZ_STR_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^(?P<std>[^<0-9:.+\-]+|<[a-zA-Z0-9+\-]+>)(?:(?P<stdoff>[+\-]?[0-9]{1,3}(?::[0-9]{2}(?::[0-9]{2})?)?)(?:(?P<dst>[^0-9:.+\-]+|<[a-zA-Z0-9+\-]+>)(?P<dstoff>[+\-]?[0-9]{1,3}(?::[0-9]{2}(?::[0-9]{2})?)?)?)?)?$",
    )
    .expect("valid regex")
});
static TZ_DELTA_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(?P<sign>[+\-])?(?P<h>[0-9]{1,3})(:(?P<m>[0-9]{2})(:(?P<s>[0-9]{2}))?)?$")
        .expect("valid regex")
});
static DST_M_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^M([0-9]{1,2})\.([0-9]).([0-9])$").expect("valid regex"));

fn parse_tz_delta(text: &str) -> Result<i64, String> {
    let caps = TZ_DELTA_RE
        .captures(text)
        .ok_or_else(|| format!("Invalid offset: {text}"))?;
    let num = |name: &str| -> i64 {
        caps.name(name)
            .map_or(0, |m| m.as_str().parse().unwrap_or(0))
    };
    let (h, m, s) = (num("h"), num("m"), num("s"));
    let mut total = h * 3600 + m * 60 + s;
    if h > 24 {
        return Err(format!("Offset hours must be in [0, 24]: {text}"));
    }
    if caps.name("sign").map(|m| m.as_str()) != Some("-") {
        total = -total;
    }
    Ok(total)
}

fn parse_transition_time(text: &str) -> Result<i64, String> {
    let caps = TZ_DELTA_RE
        .captures(text)
        .ok_or_else(|| format!("Invalid time: {text}"))?;
    let num = |name: &str| -> i64 {
        caps.name(name)
            .map_or(0, |m| m.as_str().parse().unwrap_or(0))
    };
    let (h, m, s) = (num("h"), num("m"), num("s"));
    if h > 167 {
        return Err(format!("Hour must be in [0, 167]: {text}"));
    }
    let total = h * 3600 + m * 60 + s;
    Ok(if caps.name("sign").map(|m| m.as_str()) == Some("-") {
        -total
    } else {
        total
    })
}

fn parse_dst_start_end(text: &str) -> Result<DstRule, String> {
    let (date, time) = match text.split_once('/') {
        Some((d, t)) => (d, Some(t)),
        None => (text, None),
    };
    let secs = match time {
        Some(t) => parse_transition_time(t)?,
        None => 7200,
    };
    if date.starts_with('M') {
        let caps = DST_M_RE
            .captures(date)
            .ok_or_else(|| format!("Invalid dst start/end date: {text}"))?;
        let m: u32 = caps[1].parse().map_err(|_| "bad month".to_string())?;
        let w: u32 = caps[2].parse().map_err(|_| "bad week".to_string())?;
        let d: u32 = caps[3].parse().map_err(|_| "bad day".to_string())?;
        if !(1..=12).contains(&m) {
            return Err("m must be in [1, 12]".to_string());
        }
        if !(1..=5).contains(&w) {
            return Err("w must be in [1, 5]".to_string());
        }
        if d > 6 {
            return Err("d must be in [0, 6]".to_string());
        }
        Ok(DstRule::Calendar { m, w, d, secs })
    } else {
        let (julian, digits) = match date.strip_prefix('J') {
            Some(rest) => (true, rest),
            None => (false, date),
        };
        let d: i64 = digits
            .trim()
            .parse()
            .map_err(|_| format!("invalid literal for int() with base 10: '{digits}'"))?;
        let min_day = i64::from(julian);
        if !(min_day..=365).contains(&d) {
            return Err(format!("d must be in [{min_day}, 365], not: {d}"));
        }
        Ok(DstRule::Day { d, julian, secs })
    }
}

fn parse_tz_str(tz_str: &str) -> Result<TzAfter, String> {
    let (offset_str, start_end) = match tz_str.split_once(',') {
        Some((o, rest)) => (o, Some(rest)),
        None => (tz_str, None),
    };
    let caps = TZ_STR_RE
        .captures(offset_str)
        .ok_or_else(|| format!("{tz_str} is not a valid TZ string"))?;
    let strip_angle = |s: &str| s.trim_matches(|c| c == '<' || c == '>').to_string();
    let std_abbr = strip_angle(&caps["std"]);
    let dst_abbr = caps.name("dst").map(|m| strip_angle(m.as_str()));
    let std_offset = match caps.name("stdoff") {
        Some(m) => {
            parse_tz_delta(m.as_str()).map_err(|_| format!("Invalid STD offset in {tz_str}"))?
        }
        None => 0,
    };
    match dst_abbr {
        Some(dst_abbr) => {
            let dst_offset = match caps.name("dstoff") {
                Some(m) => parse_tz_delta(m.as_str())
                    .map_err(|_| format!("Invalid DST offset in {tz_str}"))?,
                None => std_offset + 3600,
            };
            let start_end =
                start_end.ok_or_else(|| format!("Missing transition rules: {tz_str}"))?;
            let (start, end) = start_end
                .split_once(',')
                .ok_or_else(|| format!("Invalid TZ string: {tz_str}"))?;
            let start =
                parse_dst_start_end(start).map_err(|_| format!("Invalid TZ string: {tz_str}"))?;
            let end =
                parse_dst_start_end(end).map_err(|_| format!("Invalid TZ string: {tz_str}"))?;
            Ok(TzAfter::Rule(TzStr {
                std: TtInfo {
                    utcoff: std_offset,
                    isdst: false,
                    tzname: std_abbr,
                },
                dst: TtInfo {
                    utcoff: dst_offset,
                    isdst: true,
                    tzname: dst_abbr,
                },
                start,
                end,
                dst_diff: dst_offset - std_offset,
            }))
        }
        None if start_end.is_some() => {
            Err(format!("Transition rule present without DST: {tz_str}"))
        }
        None => Ok(TzAfter::Fixed(TtInfo {
            utcoff: std_offset,
            isdst: false,
            tzname: std_abbr,
        })),
    }
}

/// The local time zone of the C library: `TZ` (a zone key, optionally prefixed with `:`, or a
/// POSIX TZ string), else `/etc/localtime`, else UTC.
pub fn local_zone() -> Arc<ZoneInfo> {
    static LOCAL: Lazy<Arc<ZoneInfo>> = Lazy::new(|| Arc::new(load_local_zone()));
    LOCAL.clone()
}

fn load_local_zone() -> ZoneInfo {
    if let Ok(tz) = std::env::var("TZ")
        && !tz.is_empty()
    {
        let key = tz.strip_prefix(':').unwrap_or(&tz);
        if let Ok(zone) = ZoneInfo::new(key) {
            return (*zone).clone();
        }
        if let Ok(zone) = ZoneInfo::from_tz_string(&tz) {
            return zone;
        }
        return ZoneInfo::fixed(0, "UTC", None);
    }
    match std::fs::read("/etc/localtime") {
        Ok(data) => {
            ZoneInfo::from_bytes(&data, None).unwrap_or_else(|_| ZoneInfo::fixed(0, "UTC", None))
        }
        Err(_) => ZoneInfo::fixed(0, "UTC", None),
    }
}

/// `time.localtime(timestamp)` broken down: the local wall time and its time type.
pub fn localtime(timestamp: i64) -> (NaiveDateTime, TtInfo) {
    let zone = local_zone();
    let utc = epoch_to_wall(timestamp);
    let (offset, _, tti) = zone.fromutc(&utc);
    (epoch_to_wall(timestamp + offset), tti)
}

/// Seconds since the epoch -> naive wall time (proleptic, any representable year).
pub(crate) fn epoch_to_wall(timestamp: i64) -> NaiveDateTime {
    let days = timestamp.div_euclid(86400);
    let secs = timestamp.rem_euclid(86400) as u32;
    let date = NaiveDate::from_num_days_from_ce_opt(
        i32::try_from(days + EPOCHORDINAL).unwrap_or(i32::MAX),
    )
    .unwrap_or(NaiveDate::MAX);
    date.and_hms_opt(secs / 3600, (secs / 60) % 60, secs % 60)
        .expect("valid time of day")
}

/// The C library's view of the local zone, as CPython's `time` module computes it (the
/// offsets and names in effect on January 1st and half a year later): `(timezone, altzone,
/// daylight, tzname)`.
pub fn time_module_zone(now: i64) -> (i64, i64, bool, (String, String)) {
    const YEAR: i64 = (365 * 24 + 6) * 3600;
    let t = now.div_euclid(YEAR) * YEAR;
    let (_, jan) = localtime(t);
    let (_, jul) = localtime(t + YEAR / 2);
    let janzone = -jan.utcoff;
    let julyzone = -jul.utcoff;
    if janzone < julyzone {
        (
            julyzone,
            janzone,
            janzone != julyzone,
            (jul.tzname, jan.tzname),
        )
    } else {
        (
            janzone,
            julyzone,
            janzone != julyzone,
            (jan.tzname, jul.tzname),
        )
    }
}

/// Seconds since the epoch of a wall time read as UTC (`calendar.timegm`).
pub(crate) fn wall_timestamp(wall: &NaiveDateTime) -> i64 {
    local_timestamp(wall)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn posix_rules_follow_cpython() {
        let zone = ZoneInfo::from_tz_string("EST5EDT,M3.2.0,M11.1.0").unwrap();
        let winter = NaiveDate::from_ymd_opt(2030, 1, 15)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap();
        let summer = NaiveDate::from_ymd_opt(2030, 7, 15)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap();
        assert_eq!(zone.utcoffset(&winter, 0), -5 * 3600);
        assert_eq!(zone.tzname(&summer, 0), "EDT");
        assert_eq!(zone.utcoffset(&summer, 0), -4 * 3600);
    }

    #[test]
    fn keys_are_validated() {
        assert!(matches!(
            validate_key("/etc/passwd"),
            Err(ZoneError::InvalidKey(_))
        ));
        assert!(matches!(
            validate_key("../x"),
            Err(ZoneError::InvalidKey(_))
        ));
        assert!(matches!(
            validate_key("a/../b"),
            Err(ZoneError::InvalidKey(_))
        ));
        assert!(validate_key("America/New_York").is_ok());
    }
}
