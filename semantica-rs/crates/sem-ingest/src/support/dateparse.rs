//! A subset of `dateutil.parser.parse` (default options: month-first, today's date filling
//! missing parts) for the date strings feeds carry: ISO 8601 variants, RFC 822/1123 with
//! named or numeric zones, `D Mon YYYY`, `Mon D, YYYY`, `M/D/YYYY`, `YYYYMMDD`, times with
//! AM/PM. Strings with unknown words fail (dateutil's `ParserError`).

use chrono::{Datelike, FixedOffset, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone};
use sem_utils::pycompat::time::PyDateTime;

const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];
const MONTHS_FULL: [&str; 12] = [
    "january", "february", "march", "april", "may", "june", "july", "august", "september",
    "october", "november", "december",
];
const WEEKDAYS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];
const WEEKDAYS_FULL: [&str; 7] = [
    "monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday",
];
const SKIP: [&str; 6] = ["at", "on", "and", "ad", "m", "t"];

fn month_of(word: &str) -> Option<u32> {
    let w = word.to_lowercase();
    MONTHS
        .iter()
        .position(|m| *m == w)
        .or_else(|| MONTHS_FULL.iter().position(|m| *m == w))
        .or_else(|| if w == "sept" { Some(8) } else { None })
        .map(|i| i as u32 + 1)
}

fn is_weekday(word: &str) -> bool {
    let w = word.to_lowercase();
    WEEKDAYS.contains(&w.as_str()) || WEEKDAYS_FULL.contains(&w.as_str())
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Num(String),
    Word(String),
    Sep(char),
}

fn tokenize(s: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || (chars[i] == '.' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit() && out.last().is_some_and(|t| matches!(t, Token::Sep(':'))))) {
                i += 1;
            }
            out.push(Token::Num(chars[start..i].iter().collect()));
        } else if c.is_alphabetic() {
            let start = i;
            while i < chars.len() && chars[i].is_alphabetic() {
                i += 1;
            }
            out.push(Token::Word(chars[start..i].iter().collect()));
        } else {
            if !c.is_whitespace() {
                out.push(Token::Sep(c));
            }
            i += 1;
        }
    }
    out
}

/// Parses `text`; `None` when dateutil would raise.
pub fn parse(text: &str) -> Option<PyDateTime> {
    let tokens = tokenize(text);
    if tokens.is_empty() {
        return None;
    }
    let mut year: Option<i32> = None;
    let mut month: Option<u32> = None;
    let mut day: Option<u32> = None;
    let (mut hour, mut minute, mut second, mut micro) = (None::<u32>, 0u32, 0u32, 0u32);
    let mut ampm: Option<bool> = None;
    let mut offset: Option<i32> = None;
    let mut loose_numbers: Vec<u32> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        match &tokens[i] {
            Token::Num(n) => {
                // Time: H:M[:S[.f]]
                if matches!(tokens.get(i + 1), Some(Token::Sep(':'))) && matches!(tokens.get(i + 2), Some(Token::Num(_))) {
                    hour = n.parse().ok();
                    if let Some(Token::Num(m)) = tokens.get(i + 2) {
                        minute = m.parse().ok()?;
                    }
                    i += 3;
                    if matches!(tokens.get(i), Some(Token::Sep(':'))) && let Some(Token::Num(s)) = tokens.get(i + 1) {
                        let (whole, frac) = match s.split_once('.') {
                            Some((w, f)) => (w, f),
                            None => (s.as_str(), ""),
                        };
                        second = whole.parse().ok()?;
                        if !frac.is_empty() {
                            let f: String = format!("{frac:0<6}").chars().take(6).collect();
                            micro = f.parse().ok()?;
                        }
                        i += 2;
                    }
                    continue;
                }
                // ISO date: YYYY-MM-DD / YYYY/MM/DD
                if n.len() == 4
                    && matches!(tokens.get(i + 1), Some(Token::Sep('-' | '/' | '.')))
                    && let Some(Token::Num(m)) = tokens.get(i + 2)
                {
                    year = n.parse().ok();
                    month = m.parse().ok();
                    i += 3;
                    if matches!(tokens.get(i), Some(Token::Sep('-' | '/' | '.'))) && let Some(Token::Num(d)) = tokens.get(i + 1) {
                        day = d.parse().ok();
                        i += 2;
                    }
                    continue;
                }
                // M/D/Y
                if matches!(tokens.get(i + 1), Some(Token::Sep('/' | '-' | '.')))
                    && let Some(Token::Num(b)) = tokens.get(i + 2)
                {
                    let a: u32 = n.parse().ok()?;
                    let b: u32 = b.parse().ok()?;
                    i += 3;
                    let mut c: Option<String> = None;
                    if matches!(tokens.get(i), Some(Token::Sep('/' | '-' | '.'))) && let Some(Token::Num(x)) = tokens.get(i + 1) {
                        c = Some(x.clone());
                        i += 2;
                    }
                    if a > 12 {
                        day = Some(a);
                        month = Some(b);
                    } else {
                        month = Some(a);
                        day = Some(b);
                    }
                    if let Some(c) = c {
                        let y: i32 = c.parse().ok()?;
                        year = Some(if c.len() <= 2 { 2000 + y } else { y });
                    }
                    continue;
                }
                if n.len() == 8 && year.is_none() {
                    year = n[..4].parse().ok();
                    month = n[4..6].parse().ok();
                    day = n[6..].parse().ok();
                    i += 1;
                    continue;
                }
                // Offset digits after a sign handled with Sep.
                loose_numbers.push(n.parse().ok()?);
                if n.len() == 4 && year.is_none() && loose_numbers.len() > 1 {
                    year = n.parse().ok();
                    loose_numbers.pop();
                }
                i += 1;
            }
            Token::Word(w) => {
                let lower = w.to_lowercase();
                if let Some(m) = month_of(w) {
                    month = Some(m);
                } else if is_weekday(w) {
                } else if lower == "am" || lower == "pm" {
                    ampm = Some(lower == "pm");
                } else if lower == "utc" || lower == "gmt" || lower == "z" {
                    offset = Some(0);
                } else if (lower.len() == 3 || lower.len() == 4) && w.chars().all(|c| c.is_ascii_uppercase()) && hour.is_some() {
                    // Unknown time zone names are ignored (naive result), as dateutil does.
                } else if SKIP.contains(&lower.as_str()) {
                } else {
                    return None;
                }
                i += 1;
            }
            Token::Sep(c @ ('+' | '-')) => {
                if hour.is_some()
                    && let Some(Token::Num(n)) = tokens.get(i + 1)
                {
                    let sign = if *c == '-' { -1 } else { 1 };
                    let (h, m) = if n.len() == 4 {
                        (n[..2].parse::<i32>().ok()?, n[2..].parse::<i32>().ok()?)
                    } else if matches!(tokens.get(i + 2), Some(Token::Sep(':'))) && let Some(Token::Num(mm)) = tokens.get(i + 3) {
                        i += 2;
                        (n.parse::<i32>().ok()?, mm.parse::<i32>().ok()?)
                    } else {
                        (n.parse::<i32>().ok()?, 0)
                    };
                    offset = Some(sign * (h * 3600 + m * 60));
                    i += 2;
                    continue;
                }
                i += 1;
            }
            Token::Sep(_) => i += 1,
        }
    }
    // Remaining loose numbers: day then year (e.g. "27 Jan 2025", "Jan 27, 2025").
    for n in loose_numbers {
        if day.is_none() && n <= 31 {
            day = Some(n);
        } else if year.is_none() {
            year = Some(if n < 100 { 2000 + n as i32 } else { n as i32 });
        } else {
            return None;
        }
    }
    if year.is_none() && month.is_none() && day.is_none() && hour.is_none() {
        return None;
    }
    let today = Local::now().date_naive();
    let date = NaiveDate::from_ymd_opt(
        year.unwrap_or(today.year()),
        month.unwrap_or(today.month()),
        day.unwrap_or(today.day()),
    )?;
    let mut h = hour.unwrap_or(0);
    if let Some(pm) = ampm {
        if pm && h < 12 {
            h += 12;
        } else if !pm && h == 12 {
            h = 0;
        }
    }
    let time = NaiveTime::from_hms_micro_opt(h, minute, second, micro)?;
    let naive = NaiveDateTime::new(date, time);
    match offset {
        Some(secs) => {
            let tz = FixedOffset::east_opt(secs)?;
            Some(PyDateTime::Aware(tz.from_local_datetime(&naive).single()?))
        }
        None => Some(PyDateTime::Naive(naive)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_formats() {
        assert!(parse("Not a date").is_none());
        let d = parse("Mon, 27 Jan 2025 12:00:00 EST").unwrap();
        assert_eq!(d.isoformat('T'), "2025-01-27T12:00:00");
        let d = parse("2025-01-27T12:00:00.5+02:00").unwrap();
        assert_eq!(d.isoformat('T'), "2025-01-27T12:00:00.500000+02:00");
        assert_eq!(parse("January 5, 2024").unwrap().isoformat('T'), "2024-01-05T00:00:00");
        assert_eq!(parse("1/2/2024 3:04 PM").unwrap().isoformat('T'), "2024-01-02T15:04:00");
    }
}
