//! A port of `dateutil.parser.parse` (python-dateutil 2.9.0) with the default `parserinfo`,
//! and of the `relativedelta` arithmetic the date normalizer uses.
//!
//! The lexer (`_timelex`), the token loop (`_parse`), the year/month/day resolution (`_ymd`),
//! `_build_naive` and `_build_tzaware` follow the original line by line, including its fuzzy
//! mode. Numbers use Python's `float()`/`int()`/`Decimal` acceptance rules.

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Timelike};
use sem_utils::SemanticaError;
use sem_utils::pycompat::warnings::{RUNTIME_WARNING, WarningCategory, warn};

use crate::datetime::{DateTime, TzInfo};
use crate::pystr;
use crate::zoneinfo::monthrange;

/// `dateutil.parser.UnknownTimezoneWarning` (a `RuntimeWarning`).
pub static UNKNOWN_TIMEZONE_WARNING: WarningCategory =
    WarningCategory::new("UnknownTimezoneWarning", &RUNTIME_WARNING);

/// Why parsing failed (`ParserError`, or another exception escaping `parse`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ParseFailure(pub String);

impl From<SemanticaError> for ParseFailure {
    fn from(e: SemanticaError) -> Self {
        ParseFailure(e.to_string())
    }
}

/// An exception inside `_parse` (`ValueError`/`IndexError`, which make `_parse` return
/// `None`).
#[derive(Debug)]
struct Abort;

type Step<T> = Result<T, Abort>;

// ---------------------------------------------------------------------------------------
// _timelex

fn timelex_split(s: &str) -> Vec<String> {
    let chars: Vec<char> = s.chars().filter(|&c| c != '\0').collect();
    let mut pos = 0usize;
    let mut charstack: Vec<char> = Vec::new();
    let mut tokenstack: std::collections::VecDeque<String> = std::collections::VecDeque::new();
    let mut eof = false;
    let mut tokens = Vec::new();

    loop {
        if let Some(tok) = tokenstack.pop_front() {
            tokens.push(tok);
            continue;
        }
        let mut seenletters = false;
        let mut token: Option<String> = None;
        let mut state: Option<&str> = None;
        while !eof {
            let nextchar = if !charstack.is_empty() {
                Some(charstack.remove(0))
            } else if pos < chars.len() {
                pos += 1;
                Some(chars[pos - 1])
            } else {
                None
            };
            let Some(nextchar) = nextchar else {
                eof = true;
                break;
            };
            match state {
                None => {
                    token = Some(nextchar.to_string());
                    if pystr::is_alpha(nextchar) {
                        state = Some("a");
                    } else if pystr::is_digit(nextchar) {
                        state = Some("0");
                    } else if pystr::is_space(nextchar) {
                        token = Some(" ".to_string());
                        break;
                    } else {
                        break;
                    }
                }
                Some("a") => {
                    seenletters = true;
                    let t = token.as_mut().expect("token started");
                    if pystr::is_alpha(nextchar) {
                        t.push(nextchar);
                    } else if nextchar == '.' {
                        t.push(nextchar);
                        state = Some("a.");
                    } else {
                        charstack.push(nextchar);
                        break;
                    }
                }
                Some("0") => {
                    let t = token.as_mut().expect("token started");
                    if pystr::is_digit(nextchar) {
                        t.push(nextchar);
                    } else if nextchar == '.' || (nextchar == ',' && pystr::char_len(t) >= 2) {
                        t.push(nextchar);
                        state = Some("0.");
                    } else {
                        charstack.push(nextchar);
                        break;
                    }
                }
                Some("a.") => {
                    seenletters = true;
                    let t = token.as_mut().expect("token started");
                    if nextchar == '.' || pystr::is_alpha(nextchar) {
                        t.push(nextchar);
                    } else if pystr::is_digit(nextchar) && t.ends_with('.') {
                        t.push(nextchar);
                        state = Some("0.");
                    } else {
                        charstack.push(nextchar);
                        break;
                    }
                }
                Some("0.") => {
                    let t = token.as_mut().expect("token started");
                    if nextchar == '.' || pystr::is_digit(nextchar) {
                        t.push(nextchar);
                    } else if pystr::is_alpha(nextchar) && t.ends_with('.') {
                        t.push(nextchar);
                        state = Some("a.");
                    } else {
                        charstack.push(nextchar);
                        break;
                    }
                }
                Some(_) => unreachable!(),
            }
        }
        let Some(mut tok) = token else {
            break;
        };
        if matches!(state, Some("a.") | Some("0."))
            && (seenletters
                || tok.matches('.').count() > 1
                || tok.ends_with('.')
                || tok.ends_with(','))
        {
            let parts = split_decimal(&tok);
            tok = parts[0].clone();
            for part in &parts[1..] {
                if !part.is_empty() {
                    tokenstack.push_back(part.clone());
                }
            }
        }
        if state == Some("0.") && !tok.contains('.') {
            tok = tok.replace(',', ".");
        }
        tokens.push(tok);
    }
    tokens
}

/// `re.split("([.,])", token)`.
fn split_decimal(token: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    for c in token.chars() {
        if c == '.' || c == ',' {
            parts.push(std::mem::take(&mut current));
            parts.push(c.to_string());
        } else {
            current.push(c);
        }
    }
    parts.push(current);
    parts
}

// ---------------------------------------------------------------------------------------
// parserinfo

const JUMP: [&str; 18] = [
    " ", ".", ",", ";", "-", "/", "'", "at", "on", "and", "ad", "m", "t", "of", "st", "nd", "rd",
    "th",
];
const WEEKDAYS: [&[&str]; 7] = [
    &["mon", "monday"],
    &["tue", "tuesday"],
    &["wed", "wednesday"],
    &["thu", "thursday"],
    &["fri", "friday"],
    &["sat", "saturday"],
    &["sun", "sunday"],
];
const MONTHS: [&[&str]; 12] = [
    &["jan", "january"],
    &["feb", "february"],
    &["mar", "march"],
    &["apr", "april"],
    &["may", "may"],
    &["jun", "june"],
    &["jul", "july"],
    &["aug", "august"],
    &["sep", "sept", "september"],
    &["oct", "october"],
    &["nov", "november"],
    &["dec", "december"],
];
const HMS: [&[&str]; 3] = [
    &["h", "hour", "hours"],
    &["m", "minute", "minutes"],
    &["s", "second", "seconds"],
];
const AMPM: [&[&str]; 2] = [&["am", "a"], &["pm", "p"]];
const UTCZONE: [&str; 4] = ["UTC", "GMT", "Z", "z"];

fn lookup(table: &[&[&str]], name: &str) -> Option<usize> {
    let lower = pystr::lower(name);
    table
        .iter()
        .position(|names| names.contains(&lower.as_str()))
}

fn info_jump(name: &str) -> bool {
    JUMP.contains(&pystr::lower(name).as_str())
}

fn info_weekday(name: &str) -> Option<usize> {
    lookup(&WEEKDAYS, name)
}

fn info_month(name: &str) -> Option<i128> {
    lookup(&MONTHS, name).map(|i| i as i128 + 1)
}

fn info_hms(name: &str) -> Option<usize> {
    lookup(&HMS, name)
}

fn info_ampm(name: &str) -> Option<usize> {
    lookup(&AMPM, name)
}

fn info_pertain(name: &str) -> bool {
    pystr::lower(name) == "of"
}

fn info_utczone(name: &str) -> bool {
    matches!(pystr::lower(name).as_str(), "utc" | "gmt" | "z")
}

fn info_tzoffset(name: &str) -> Option<i64> {
    // `name in self._utczone` compares against the lowercased keys.
    if matches!(name, "utc" | "gmt" | "z") {
        Some(0)
    } else {
        None
    }
}

fn convertyear(year: i128, century_specified: bool, current_year: i128) -> i128 {
    let mut year = year;
    if year < 100 && !century_specified {
        let century = current_year / 100 * 100;
        year += century;
        if year >= current_year + 50 {
            year -= 100;
        } else if year < current_year - 50 {
            year += 100;
        }
    }
    year
}

// ---------------------------------------------------------------------------------------
// Decimal (the subset of `decimal.Decimal` the parser uses on numeric tokens)

/// A finite non-negative decimal from a token: `int_part` + `frac` (decimal digits).
#[derive(Debug, Clone, PartialEq)]
struct Dec {
    int_part: i128,
    frac: String,
}

impl Dec {
    fn parse(token: &str) -> Step<Dec> {
        // Unicode decimal digits read as ASCII, as Decimal() does.
        let mut ascii = String::new();
        for c in token.chars() {
            if pystr::is_decimal(c) {
                let digit = pystr::py_int(&c.to_string()).map_err(|_| Abort)?;
                ascii.push(char::from(b'0' + digit as u8));
            } else {
                ascii.push(c);
            }
        }
        let (int_str, frac) = match ascii.split_once('.') {
            Some((i, f)) => (i, f),
            None => (ascii.as_str(), ""),
        };
        if int_str.is_empty() && frac.is_empty() {
            return Err(Abort);
        }
        if !int_str.bytes().all(|b| b.is_ascii_digit()) || !frac.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(Abort);
        }
        let int_part = if int_str.is_empty() {
            0
        } else {
            int_str.parse::<i128>().map_err(|_| Abort)?
        };
        Ok(Dec {
            int_part,
            frac: frac.to_string(),
        })
    }

    fn has_fraction(&self) -> bool {
        self.frac.bytes().any(|b| b != b'0')
    }

    /// `int(60 * (value % 1))`.
    fn sixty_times_fraction(&self) -> i128 {
        let digits: String = self.frac.chars().take(30).collect();
        if digits.is_empty() {
            return 0;
        }
        let numerator: i128 = digits.parse().unwrap_or(0);
        let denominator = 10i128.pow(digits.len() as u32);
        numerator * 60 / denominator
    }

    /// Compare with an integer.
    fn cmp_int(&self, n: i128) -> std::cmp::Ordering {
        match self.int_part.cmp(&n) {
            std::cmp::Ordering::Equal if self.has_fraction() => std::cmp::Ordering::Greater,
            other => other,
        }
    }

    fn gt(&self, n: i128) -> bool {
        self.cmp_int(n) == std::cmp::Ordering::Greater
    }

    fn ge(&self, n: i128) -> bool {
        self.cmp_int(n) != std::cmp::Ordering::Less
    }

    fn lt(&self, n: i128) -> bool {
        self.cmp_int(n) == std::cmp::Ordering::Less
    }

    fn le(&self, n: i128) -> bool {
        self.cmp_int(n) != std::cmp::Ordering::Greater
    }
}

/// `float(token)` succeeded but the value is not finite (`_to_decimal` rejects it).
fn token_is_float(token: &str) -> bool {
    pystr::py_float(token).is_ok()
}

fn int_of(s: &str) -> Step<i128> {
    pystr::py_int(s).map_err(|_| Abort)
}

/// `s[a:b]` over code points.
fn slice(s: &str, start: usize, end: Option<usize>) -> String {
    let chars: Vec<char> = s.chars().collect();
    let end = end.unwrap_or(chars.len()).min(chars.len());
    let start = start.min(end);
    chars[start..end].iter().collect()
}

// ---------------------------------------------------------------------------------------
// _ymd

#[derive(Debug, Default)]
struct Ymd {
    values: Vec<i128>,
    century_specified: bool,
    dstridx: Option<usize>,
    mstridx: Option<usize>,
    ystridx: Option<usize>,
}

/// A value appended to `_ymd`: a token string or a number.
enum YmdVal<'a> {
    Str(&'a str),
    Dec(&'a Dec),
    Int(i128),
}

impl Ymd {
    fn len(&self) -> usize {
        self.values.len()
    }

    fn could_be_day(&self, value: &Dec) -> bool {
        if self.dstridx.is_some() {
            return false;
        }
        let Some(mstridx) = self.mstridx else {
            return value.ge(1) && value.le(31);
        };
        let month = self.values[mstridx];
        let year = match self.ystridx {
            None => 2000,
            Some(y) => self.values[y],
        };
        if !(1..=12).contains(&month) {
            return false;
        }
        let days = monthrange(i64::try_from(year).unwrap_or(2000), month as u32).1;
        value.ge(1) && value.le(i128::from(days))
    }

    fn append(&mut self, val: YmdVal<'_>, label: Option<char>) -> Step<()> {
        let mut label = label;
        let int_val = match val {
            YmdVal::Str(s) => {
                if pystr::str_is_digit(s) && pystr::char_len(s) > 2 {
                    self.century_specified = true;
                    if !matches!(label, None | Some('Y')) {
                        return Err(Abort);
                    }
                    label = Some('Y');
                }
                int_of(s)?
            }
            YmdVal::Dec(d) => {
                if d.gt(100) {
                    self.century_specified = true;
                    if !matches!(label, None | Some('Y')) {
                        return Err(Abort);
                    }
                    label = Some('Y');
                }
                d.int_part
            }
            YmdVal::Int(i) => {
                if i > 100 {
                    self.century_specified = true;
                    if !matches!(label, None | Some('Y')) {
                        return Err(Abort);
                    }
                    label = Some('Y');
                }
                i
            }
        };
        self.values.push(int_val);
        let idx = self.values.len() - 1;
        match label {
            Some('M') => {
                if self.mstridx.is_some() {
                    return Err(Abort);
                }
                self.mstridx = Some(idx);
            }
            Some('D') => {
                if self.dstridx.is_some() {
                    return Err(Abort);
                }
                self.dstridx = Some(idx);
            }
            Some('Y') => {
                if self.ystridx.is_some() {
                    return Err(Abort);
                }
                self.ystridx = Some(idx);
            }
            _ => {}
        }
        Ok(())
    }

    /// Python list indexing (negative indices count from the end).
    fn at(&self, idx: isize) -> Step<i128> {
        let len = self.values.len() as isize;
        let i = if idx < 0 { idx + len } else { idx };
        if i < 0 || i >= len {
            return Err(Abort);
        }
        Ok(self.values[i as usize])
    }

    fn resolve_from_stridxs(
        &self,
        strids: &mut Vec<(char, usize)>,
    ) -> (Option<i128>, Option<i128>, Option<i128>) {
        if self.len() == 3 && strids.len() == 2 {
            let used: Vec<usize> = strids.iter().map(|(_, v)| *v).collect();
            let missing = (0..3)
                .find(|x| !used.contains(x))
                .expect("one index missing");
            let key = ['y', 'm', 'd']
                .into_iter()
                .find(|k| !strids.iter().any(|(s, _)| s == k))
                .expect("one key missing");
            strids.push((key, missing));
        }
        let get = |k: char| -> Option<i128> {
            strids
                .iter()
                .find(|(s, _)| *s == k)
                .map(|(_, idx)| self.values[*idx])
        };
        (get('y'), get('m'), get('d'))
    }

    fn resolve_ymd(
        &self,
        yearfirst: bool,
        dayfirst: bool,
    ) -> Step<(Option<i128>, Option<i128>, Option<i128>)> {
        let len_ymd = self.len();
        let mut strids: Vec<(char, usize)> = [
            ('y', self.ystridx),
            ('m', self.mstridx),
            ('d', self.dstridx),
        ]
        .into_iter()
        .filter_map(|(k, v)| v.map(|v| (k, v)))
        .collect();
        if (len_ymd == strids.len() && !strids.is_empty()) || (len_ymd == 3 && strids.len() == 2) {
            return Ok(self.resolve_from_stridxs(&mut strids));
        }
        let mstridx = self.mstridx;
        let (mut year, mut month, mut day) = (None, None, None);
        if len_ymd > 3 {
            return Err(Abort);
        } else if len_ymd == 1 || (mstridx.is_some() && len_ymd == 2) {
            let other;
            if let Some(m) = mstridx {
                month = Some(self.values[m]);
                other = self.at(m as isize - 1)?;
            } else {
                other = self.values[0];
            }
            if len_ymd > 1 || mstridx.is_none() {
                if other > 31 {
                    year = Some(other);
                } else {
                    day = Some(other);
                }
            }
        } else if len_ymd == 2 {
            let (a, b) = (self.values[0], self.values[1]);
            if a > 31 {
                year = Some(a);
                month = Some(b);
            } else if b > 31 {
                month = Some(a);
                year = Some(b);
            } else if dayfirst && b <= 12 {
                day = Some(a);
                month = Some(b);
            } else {
                month = Some(a);
                day = Some(b);
            }
        } else if len_ymd == 3 {
            let (a, b, c) = (self.values[0], self.values[1], self.values[2]);
            match mstridx {
                Some(0) => {
                    if b > 31 {
                        month = Some(a);
                        year = Some(b);
                        day = Some(c);
                    } else {
                        month = Some(a);
                        day = Some(b);
                        year = Some(c);
                    }
                }
                Some(1) => {
                    if a > 31 || (yearfirst && c <= 31) {
                        year = Some(a);
                        month = Some(b);
                        day = Some(c);
                    } else {
                        day = Some(a);
                        month = Some(b);
                        year = Some(c);
                    }
                }
                Some(2) => {
                    if b > 31 {
                        day = Some(a);
                        year = Some(b);
                        month = Some(c);
                    } else {
                        year = Some(a);
                        day = Some(b);
                        month = Some(c);
                    }
                }
                _ => {
                    if a > 31 || self.ystridx == Some(0) || (yearfirst && b <= 12 && c <= 31) {
                        if dayfirst && c <= 12 {
                            year = Some(a);
                            day = Some(b);
                            month = Some(c);
                        } else {
                            year = Some(a);
                            month = Some(b);
                            day = Some(c);
                        }
                    } else if a > 12 || (dayfirst && b <= 12) {
                        day = Some(a);
                        month = Some(b);
                        year = Some(c);
                    } else {
                        month = Some(a);
                        day = Some(b);
                        year = Some(c);
                    }
                }
            }
        }
        Ok((year, month, day))
    }
}

// ---------------------------------------------------------------------------------------
// parser._parse

#[derive(Debug, Default, Clone)]
struct ParseResult {
    year: Option<i128>,
    month: Option<i128>,
    day: Option<i128>,
    weekday: Option<usize>,
    hour: Option<i128>,
    minute: Option<i128>,
    second: Option<i128>,
    microsecond: Option<i128>,
    tzname: Option<String>,
    tzoffset: Option<i64>,
    ampm: Option<usize>,
    century_specified: bool,
}

impl ParseResult {
    /// `len(res)`: the number of fields set.
    fn len(&self) -> usize {
        [
            self.year.is_some(),
            self.month.is_some(),
            self.day.is_some(),
            self.weekday.is_some(),
            self.hour.is_some(),
            self.minute.is_some(),
            self.second.is_some(),
            self.microsecond.is_some(),
            self.tzname.is_some(),
            self.tzoffset.is_some(),
            self.ampm.is_some(),
        ]
        .iter()
        .filter(|b| **b)
        .count()
    }
}

fn tok(tokens: &[String], idx: usize) -> Step<&str> {
    tokens.get(idx).map(String::as_str).ok_or(Abort)
}

fn could_be_tzname(
    hour: Option<i128>,
    tzname: &Option<String>,
    tzoffset: Option<i64>,
    token: &str,
) -> bool {
    hour.is_some()
        && tzname.is_none()
        && tzoffset.is_none()
        && pystr::char_len(token) <= 5
        && (token.chars().all(|c| c.is_ascii_uppercase()) || UTCZONE.contains(&token))
}

fn ampm_valid(hour: Option<i128>, ampm: Option<usize>, fuzzy: bool) -> Step<bool> {
    let mut val_is_ampm = true;
    if fuzzy && ampm.is_some() {
        val_is_ampm = false;
    }
    match hour {
        None => {
            if fuzzy {
                val_is_ampm = false;
            } else {
                return Err(Abort);
            }
        }
        Some(h) if !(0..=12).contains(&h) => {
            if fuzzy {
                val_is_ampm = false;
            } else {
                return Err(Abort);
            }
        }
        _ => {}
    }
    Ok(val_is_ampm)
}

fn adjust_ampm(hour: i128, ampm: usize) -> i128 {
    if hour < 12 && ampm == 1 {
        hour + 12
    } else if hour == 12 && ampm == 0 {
        0
    } else {
        hour
    }
}

fn parse_min_sec(value: &Dec) -> (i128, Option<i128>) {
    let minute = value.int_part;
    let second = if value.has_fraction() {
        Some(value.sixty_times_fraction())
    } else {
        None
    };
    (minute, second)
}

fn parsems(value: &str) -> Step<(i128, i128)> {
    if !value.contains('.') {
        return Ok((int_of(value)?, 0));
    }
    let parts: Vec<&str> = value.split('.').collect();
    if parts.len() != 2 {
        return Err(Abort);
    }
    let (i, f) = (parts[0], parts[1]);
    let mut padded = f.to_string();
    while pystr::char_len(&padded) < 6 {
        padded.push('0');
    }
    let six: String = padded.chars().take(6).collect();
    Ok((int_of(i)?, int_of(&six)?))
}

fn find_hms_idx(idx: usize, tokens: &[String], allow_jump: bool) -> Option<usize> {
    let len_l = tokens.len();
    if idx + 1 < len_l && info_hms(&tokens[idx + 1]).is_some() {
        Some(idx + 1)
    } else if allow_jump
        && idx + 2 < len_l
        && tokens[idx + 1] == " "
        && info_hms(&tokens[idx + 2]).is_some()
    {
        Some(idx + 2)
    } else if idx > 0 && info_hms(&tokens[idx - 1]).is_some() {
        Some(idx - 1)
    } else if 1 < idx
        && idx == len_l - 1
        && tokens[idx - 1] == " "
        && info_hms(&tokens[idx - 2]).is_some()
    {
        Some(idx - 2)
    } else {
        None
    }
}

fn parse_hms(idx: usize, tokens: &[String], hms_idx: Option<usize>) -> (usize, Option<usize>) {
    match hms_idx {
        None => (idx, None),
        Some(h) if h > idx => (h, info_hms(&tokens[h])),
        Some(h) => (idx, info_hms(&tokens[h]).map(|v| v + 1)),
    }
}

fn assign_hms(res: &mut ParseResult, value_repr: &str, hms: usize) -> Step<()> {
    let value = Dec::parse(value_repr)?;
    match hms {
        0 => {
            res.hour = Some(value.int_part);
            if value.has_fraction() {
                res.minute = Some(value.sixty_times_fraction());
            }
        }
        1 => {
            let (minute, second) = parse_min_sec(&value);
            res.minute = Some(minute);
            res.second = second;
        }
        2 => {
            let (second, micro) = parsems(value_repr)?;
            res.second = Some(second);
            res.microsecond = Some(micro);
        }
        _ => {}
    }
    Ok(())
}

fn parse_numeric_token(
    tokens: &[String],
    idx: usize,
    ymd: &mut Ymd,
    res: &mut ParseResult,
    fuzzy: bool,
) -> Step<usize> {
    let mut idx = idx;
    let value_repr = tokens[idx].clone();
    let value = Dec::parse(&value_repr)?;
    let len_li = pystr::char_len(&value_repr);
    let len_l = tokens.len();

    if ymd.len() == 3
        && (len_li == 2 || len_li == 4)
        && res.hour.is_none()
        && (idx + 1 >= len_l || (tokens[idx + 1] != ":" && info_hms(&tokens[idx + 1]).is_none()))
    {
        let s = &tokens[idx];
        res.hour = Some(int_of(&slice(s, 0, Some(2)))?);
        if len_li == 4 {
            res.minute = Some(int_of(&slice(s, 2, None))?);
        }
    } else if len_li == 6 || (len_li > 6 && tokens[idx].chars().position(|c| c == '.') == Some(6)) {
        let s = tokens[idx].clone();
        if ymd.len() == 0 && !tokens[idx].contains('.') {
            ymd.append(YmdVal::Str(&slice(&s, 0, Some(2))), None)?;
            ymd.append(YmdVal::Str(&slice(&s, 2, Some(4))), None)?;
            ymd.append(YmdVal::Str(&slice(&s, 4, None)), None)?;
        } else {
            res.hour = Some(int_of(&slice(&s, 0, Some(2)))?);
            res.minute = Some(int_of(&slice(&s, 2, Some(4)))?);
            let (second, micro) = parsems(&slice(&s, 4, None))?;
            res.second = Some(second);
            res.microsecond = Some(micro);
        }
    } else if len_li == 8 || len_li == 12 || len_li == 14 {
        let s = tokens[idx].clone();
        ymd.append(YmdVal::Str(&slice(&s, 0, Some(4))), Some('Y'))?;
        ymd.append(YmdVal::Str(&slice(&s, 4, Some(6))), None)?;
        ymd.append(YmdVal::Str(&slice(&s, 6, Some(8))), None)?;
        if len_li > 8 {
            res.hour = Some(int_of(&slice(&s, 8, Some(10)))?);
            res.minute = Some(int_of(&slice(&s, 10, Some(12)))?);
            if len_li > 12 {
                res.second = Some(int_of(&slice(&s, 12, None))?);
            }
        }
    } else if let Some(hms_idx) = find_hms_idx(idx, tokens, true) {
        let (new_idx, hms) = parse_hms(idx, tokens, Some(hms_idx));
        idx = new_idx;
        if let Some(hms) = hms {
            assign_hms(res, &value_repr, hms)?;
        }
    } else if idx + 2 < len_l && tokens[idx + 1] == ":" {
        res.hour = Some(value.int_part);
        let value = Dec::parse(&tokens[idx + 2])?;
        let (minute, second) = parse_min_sec(&value);
        res.minute = Some(minute);
        res.second = second;
        if idx + 4 < len_l && tokens[idx + 3] == ":" {
            let (second, micro) = parsems(&tokens[idx + 4])?;
            res.second = Some(second);
            res.microsecond = Some(micro);
            idx += 2;
        }
        idx += 2;
    } else if idx + 1 < len_l && matches!(tokens[idx + 1].as_str(), "-" | "/" | ".") {
        let sep = tokens[idx + 1].clone();
        ymd.append(YmdVal::Str(&value_repr), None)?;
        if idx + 2 < len_l && !info_jump(&tokens[idx + 2]) {
            if pystr::str_is_digit(&tokens[idx + 2]) {
                ymd.append(YmdVal::Str(&tokens[idx + 2]), None)?;
            } else {
                match info_month(&tokens[idx + 2]) {
                    Some(m) => ymd.append(YmdVal::Int(m), Some('M'))?,
                    None => return Err(Abort),
                }
            }
            if idx + 3 < len_l && tokens[idx + 3] == sep {
                let next = tok(tokens, idx + 4)?;
                match info_month(next) {
                    Some(m) => ymd.append(YmdVal::Int(m), Some('M'))?,
                    None => ymd.append(YmdVal::Str(next), None)?,
                }
                idx += 2;
            }
            idx += 1;
        }
        idx += 1;
    } else if idx + 1 >= len_l || info_jump(&tokens[idx + 1]) {
        if idx + 2 < len_l && info_ampm(&tokens[idx + 2]).is_some() {
            let hour = value.int_part;
            res.hour = Some(adjust_ampm(
                hour,
                info_ampm(&tokens[idx + 2]).expect("checked"),
            ));
            idx += 1;
        } else {
            ymd.append(YmdVal::Dec(&value), None)?;
        }
        idx += 1;
    } else if info_ampm(&tokens[idx + 1]).is_some() && value.ge(0) && value.lt(24) {
        let hour = value.int_part;
        res.hour = Some(adjust_ampm(
            hour,
            info_ampm(&tokens[idx + 1]).expect("checked"),
        ));
        idx += 1;
    } else if ymd.could_be_day(&value) {
        ymd.append(YmdVal::Dec(&value), None)?;
    } else if !fuzzy {
        return Err(Abort);
    }
    Ok(idx)
}

fn parse_tokens(timestr: &str, fuzzy: bool, current_year: i128) -> Option<ParseResult> {
    let mut res = ParseResult::default();
    let mut l = timelex_split(timestr);
    let mut ymd = Ymd::default();
    let len_l = l.len();
    let mut i = 0usize;

    let outcome: Step<()> = (|| {
        while i < len_l {
            if token_is_float(&l[i]) {
                i = parse_numeric_token(&l, i, &mut ymd, &mut res, fuzzy)?;
            } else if let Some(weekday) = info_weekday(&l[i]) {
                res.weekday = Some(weekday);
            } else if let Some(month) = info_month(&l[i]) {
                ymd.append(YmdVal::Int(month), Some('M'))?;
                if i + 1 < len_l {
                    if l[i + 1] == "-" || l[i + 1] == "/" {
                        let sep = l[i + 1].clone();
                        let next = tok(&l, i + 2)?.to_string();
                        ymd.append(YmdVal::Str(&next), None)?;
                        if i + 3 < len_l && l[i + 3] == sep {
                            let next = tok(&l, i + 4)?.to_string();
                            ymd.append(YmdVal::Str(&next), None)?;
                            i += 2;
                        }
                        i += 2;
                    } else if i + 4 < len_l
                        && l[i + 1] == " "
                        && l[i + 3] == " "
                        && info_pertain(&l[i + 2])
                    {
                        if pystr::str_is_digit(&l[i + 4]) {
                            let value = int_of(&l[i + 4])?;
                            let year = convertyear(value, false, current_year);
                            ymd.append(YmdVal::Str(&year.to_string()), Some('Y'))?;
                        }
                        i += 4;
                    }
                }
            } else if let Some(value) = info_ampm(&l[i]) {
                let val_is_ampm = ampm_valid(res.hour, res.ampm, fuzzy)?;
                if val_is_ampm {
                    res.hour = Some(adjust_ampm(res.hour.expect("validated"), value));
                    res.ampm = Some(value);
                }
            } else if could_be_tzname(res.hour, &res.tzname, res.tzoffset, &l[i]) {
                res.tzname = Some(l[i].clone());
                res.tzoffset = info_tzoffset(&l[i]);
                if i + 1 < len_l && (l[i + 1] == "+" || l[i + 1] == "-") {
                    l[i + 1] = if l[i + 1] == "+" { "-" } else { "+" }.to_string();
                    res.tzoffset = None;
                    if info_utczone(res.tzname.as_deref().unwrap_or("")) {
                        res.tzname = None;
                    }
                }
            } else if res.hour.is_some() && (l[i] == "+" || l[i] == "-") {
                let signal: i64 = if l[i] == "+" { 1 } else { -1 };
                let next = tok(&l, i + 1)?.to_string();
                let len_li = pystr::char_len(&next);
                let (hour_offset, min_offset);
                if len_li == 4 {
                    hour_offset = int_of(&slice(&next, 0, Some(2)))?;
                    min_offset = int_of(&slice(&next, 2, None))?;
                } else if i + 2 < len_l && l[i + 2] == ":" {
                    hour_offset = int_of(&next)?;
                    min_offset = int_of(tok(&l, i + 3)?)?;
                    i += 2;
                } else if len_li <= 2 {
                    hour_offset = int_of(&slice(&next, 0, Some(2)))?;
                    min_offset = 0;
                } else {
                    return Err(Abort);
                }
                let total = hour_offset
                    .checked_mul(3600)
                    .and_then(|h| h.checked_add(min_offset.checked_mul(60)?))
                    .ok_or(Abort)?;
                res.tzoffset = Some(i64::try_from(total).map_err(|_| Abort)? * signal);
                if i + 5 < len_l
                    && info_jump(&l[i + 2])
                    && l[i + 3] == "("
                    && l[i + 5] == ")"
                    && pystr::char_len(&l[i + 4]) >= 3
                    && could_be_tzname(res.hour, &res.tzname, None, &l[i + 4])
                {
                    res.tzname = Some(l[i + 4].clone());
                    i += 4;
                }
                i += 1;
            } else if !(info_jump(&l[i]) || fuzzy) {
                return Err(Abort);
            }
            i += 1;
        }
        let (year, month, day) = ymd.resolve_ymd(false, false)?;
        res.century_specified = ymd.century_specified;
        res.year = year;
        res.month = month;
        res.day = day;
        Ok(())
    })();
    outcome.ok()?;

    // parserinfo.validate
    if let Some(year) = res.year {
        res.year = Some(convertyear(year, res.century_specified, current_year));
    }
    if (res.tzoffset == Some(0) && res.tzname.as_deref().is_none_or(str::is_empty))
        || matches!(res.tzname.as_deref(), Some("Z") | Some("z"))
    {
        res.tzname = Some("UTC".to_string());
        res.tzoffset = Some(0);
    } else if res.tzoffset != Some(0)
        && res.tzname.as_deref().is_some_and(|n| !n.is_empty())
        && info_utczone(res.tzname.as_deref().unwrap_or(""))
    {
        res.tzoffset = Some(0);
    }
    Some(res)
}

fn field_error(message: String) -> ParseFailure {
    ParseFailure(message)
}

fn build_naive(
    res: &ParseResult,
    default: NaiveDateTime,
    timestr: &str,
) -> Result<NaiveDateTime, ParseFailure> {
    let wrap = |msg: String| field_error(format!("{msg}: {timestr}"));
    let year = res.year.unwrap_or(i128::from(default.year()));
    let month = res.month.unwrap_or(i128::from(default.month()));
    let mut day = res.day.unwrap_or(i128::from(default.day()));
    if res.day.is_none() {
        if !(1..=12).contains(&month) {
            return Err(wrap(format!("bad month number {month}; must be 1-12")));
        }
        let ndays = i128::from(monthrange(i64::try_from(year).unwrap_or(1), month as u32).1);
        if day > ndays {
            day = ndays;
        }
    }
    if !(1..=9999).contains(&year) {
        return Err(wrap(format!("year {year} is out of range")));
    }
    if !(1..=12).contains(&month) {
        return Err(wrap("month must be in 1..12".to_string()));
    }
    let ndays = i128::from(monthrange(year as i64, month as u32).1);
    if !(1..=ndays).contains(&day) {
        return Err(wrap("day is out of range for month".to_string()));
    }
    let hour = res.hour.unwrap_or(i128::from(default.hour()));
    let minute = res.minute.unwrap_or(i128::from(default.minute()));
    let second = res.second.unwrap_or(i128::from(default.second()));
    let micro = res
        .microsecond
        .unwrap_or(i128::from(default.nanosecond() / 1000));
    if !(0..=23).contains(&hour) {
        return Err(wrap("hour must be in 0..23".to_string()));
    }
    if !(0..=59).contains(&minute) {
        return Err(wrap("minute must be in 0..59".to_string()));
    }
    if !(0..=59).contains(&second) {
        return Err(wrap("second must be in 0..59".to_string()));
    }
    if !(0..=999_999).contains(&micro) {
        return Err(wrap("microsecond must be in 0..999999".to_string()));
    }
    let naive = NaiveDate::from_ymd_opt(year as i32, month as u32, day as u32)
        .and_then(|d| d.and_hms_micro_opt(hour as u32, minute as u32, second as u32, micro as u32))
        .ok_or_else(|| wrap("invalid date".to_string()))?;
    let mut naive = naive;
    if let Some(weekday) = res.weekday
        && res.day.is_none_or(|d| d == 0)
    {
        // relativedelta(weekday=N): forward to the next such weekday (today included).
        let jumpdays = (7 - i64::from(naive.weekday().num_days_from_monday()) + weekday as i64) % 7;
        naive = naive
            .checked_add_signed(Duration::days(jumpdays))
            .filter(|d| d.year() <= 9999)
            .ok_or_else(|| ParseFailure("date value out of range".to_string()))?;
    }
    Ok(naive)
}

fn build_tzaware(naive: NaiveDateTime, res: &ParseResult) -> DateTime {
    let tzname = res.tzname.clone().filter(|n| !n.is_empty());
    let local_names = crate::datetime::local_tznames();
    if let Some(name) = &tzname
        && (*name == local_names.0 || *name == local_names.1)
    {
        let mut aware = DateTime::aware(naive, TzInfo::Local);
        // _assign_tzname: try the other fold when the name does not match.
        if aware.tzname().as_deref() != Some(name.as_str()) {
            let mut folded = aware.clone();
            folded.fold = 1;
            if folded.tzname().as_deref() == Some(name.as_str()) {
                aware = folded;
            }
        }
        if aware.tzname().as_deref() != Some(name.as_str()) && UTCZONE.contains(&name.as_str()) {
            aware = aware.replace_tzinfo(Some(TzInfo::Utc));
        }
        return aware;
    }
    match res.tzoffset {
        Some(0) => DateTime::aware(naive, TzInfo::Utc),
        Some(offset) => DateTime::aware(
            naive,
            TzInfo::TzOffset {
                name: tzname,
                offset,
            },
        ),
        None => {
            if let Some(name) = tzname {
                warn(
                    format!(
                        "tzname {name} identified but not understood.  Pass `tzinfos` argument in order to correctly return a timezone-aware datetime.  In a future version, this will raise an exception."
                    ),
                    UNKNOWN_TIMEZONE_WARNING,
                );
            }
            DateTime::naive(naive)
        }
    }
}

/// `dateutil.parser.parse(timestr, fuzzy=fuzzy)` with `default` = today at midnight (local
/// time) and `current_year` = the local year (for two-digit years).
pub(crate) fn parse(
    timestr: &str,
    fuzzy: bool,
    now: NaiveDateTime,
) -> Result<DateTime, ParseFailure> {
    let default = now.date().and_hms_opt(0, 0, 0).expect("midnight is valid");
    let current_year = i128::from(now.year());
    let Some(res) = parse_tokens(timestr, fuzzy, current_year) else {
        return Err(ParseFailure(format!("Unknown string format: {timestr}")));
    };
    if res.len() == 0 {
        return Err(ParseFailure(format!(
            "String does not contain a date: {timestr}"
        )));
    }
    let naive = build_naive(&res, default, timestr)?;
    Ok(build_tzaware(naive, &res))
}

// ---------------------------------------------------------------------------------------
// relativedelta(months=..) / relativedelta(years=..)

/// `dt + relativedelta(years=years, months=months)` (months already within -12..=12 after
/// `relativedelta`'s normalization; the day is clamped to the month's length).
pub(crate) fn add_relative_months(
    dt: &DateTime,
    years: i64,
    months: i64,
) -> Result<DateTime, SemanticaError> {
    // relativedelta._fix: carry whole years out of the months.
    let (mut years, mut months) = (years, months);
    if months.abs() > 11 {
        let sign = months.signum();
        let (div, rem) = (
            (months * sign).div_euclid(12),
            (months * sign).rem_euclid(12),
        );
        months = rem * sign;
        years += div * sign;
    }
    let mut year = i64::from(dt.wall.year()) + years;
    let mut month = i64::from(dt.wall.month());
    if months != 0 {
        month += months;
        if month > 12 {
            year += 1;
            month -= 12;
        } else if month < 1 {
            year -= 1;
            month += 12;
        }
    }
    let ndays = monthrange(year, month as u32).1;
    let day = dt.wall.day().min(ndays);
    if !(1..=9999).contains(&year) {
        return Err(SemanticaError::Value(format!(
            "year {year} is out of range"
        )));
    }
    let date = NaiveDate::from_ymd_opt(year as i32, month as u32, day)
        .ok_or_else(|| SemanticaError::Value("day is out of range for month".to_string()))?;
    Ok(DateTime {
        wall: date.and_time(dt.wall.time()),
        fold: dt.fold,
        tzinfo: dt.tzinfo.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 21)
            .unwrap()
            .and_hms_opt(11, 52, 3)
            .unwrap()
    }

    fn iso(s: &str, fuzzy: bool) -> String {
        match parse(s, fuzzy, now()) {
            Ok(dt) => dt.isoformat().unwrap_or_else(|e| format!("ERR {e}")),
            Err(e) => format!("ERR {}", e.0),
        }
    }

    #[test]
    fn lexer_matches_dateutil() {
        assert_eq!(
            timelex_split("Sep.20.2009"),
            vec!["Sep", ".", "20", ".", "2009"]
        );
        assert_eq!(
            timelex_split("4:30:21.447"),
            vec!["4", ":", "30", ":", "21.447"]
        );
        assert_eq!(timelex_split("10,5"), vec!["10.5"]);
    }

    #[test]
    fn parses_like_dateutil() {
        assert_eq!(iso("2023-01-01", false), "2023-01-01T00:00:00");
        assert_eq!(
            iso("2023-01-01T12:00:00+01:00", false),
            "2023-01-01T12:00:00+01:00"
        );
        assert_eq!(
            iso("2023-01-01T12:00:00Z", false),
            "2023-01-01T12:00:00+00:00"
        );
        assert_eq!(iso("today", false), "ERR Unknown string format: today");
        assert_eq!(
            iso("today", true),
            "ERR String does not contain a date: today"
        );
        assert_eq!(iso("3 days ago", true), "2026-10-03T00:00:00");
        assert_eq!(iso("5 January 2023 10:30 PM", false), "2023-01-05T22:30:00");
        assert_eq!(
            iso("2023/02/30", false),
            "ERR day is out of range for month: 2023/02/30"
        );
        assert_eq!(iso("Monday", false), "2026-10-26T00:00:00");
        assert_eq!(iso("12/31/99", false), "1999-12-31T00:00:00");
        assert_eq!(iso("99", false), "1999-10-21T00:00:00");
        assert_eq!(iso("1.5h", false), "2026-10-21T01:30:00");
        assert_eq!(iso("20230115T103000", false), "2023-01-15T10:30:00");
        assert_eq!(iso("Sep.20.2009", false), "2009-09-20T00:00:00");
        assert_eq!(iso("4:30:21.447", false), "2026-10-21T04:30:21.447000");
        assert_eq!(iso("nan", false), "ERR Unknown string format: nan");
        assert_eq!(
            iso("The meeting is on March 3rd, 2021 at 4pm", true),
            "2021-03-03T16:00:00"
        );
        assert_eq!(
            iso("2023-01-01T12:00:00.123456+05:30", false),
            "2023-01-01T12:00:00.123456+05:30"
        );
        assert_eq!(
            iso("2023-01-01 GMT+3", false),
            "ERR Unknown string format: 2023-01-01 GMT+3"
        );
    }
}
