//! `_parse_W3CDTF_to_datetime` of python-docx / python-pptx core properties.

use chrono::{Duration, NaiveDateTime};
use once_cell::sync::Lazy;
use regex::Regex;

static OFFSET: Lazy<Regex> = Lazy::new(|| Regex::new(r"^([+-])(\d\d):(\d\d)").expect("valid regex"));

/// Parse a W3CDTF date (`yyyy`, `yyyy-mm`, `yyyy-mm-dd` or `yyyy-mm-ddThh:mm:ss` plus an
/// optional `±hh:mm` offset, which is applied to give UTC). `None` where the libraries'
/// `ValueError` makes the property `None`.
pub(crate) fn parse(text: &str) -> Option<NaiveDateTime> {
    let chars: Vec<char> = text.chars().collect();
    let split = chars.len().min(19);
    let parseable: String = chars[..split].iter().collect();
    let offset: String = chars[split..].iter().collect();
    let mut parsed = None;
    for template in ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%d", "%Y-%m", "%Y"] {
        if let Ok(dt) = sem_utils::pycompat::time::strptime(&parseable, template) {
            parsed = Some(dt.wall());
        }
    }
    let dt = parsed?;
    if offset.chars().count() == 6 {
        let caps = OFFSET.captures(&offset)?;
        let sign: i64 = if &caps[1] == "+" { -1 } else { 1 };
        let hours: i64 = caps[2].parse().ok()?;
        let minutes: i64 = caps[3].parse().ok()?;
        return dt.checked_add_signed(Duration::hours(hours * sign) + Duration::minutes(minutes * sign));
    }
    Some(dt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_like_python_docx() {
        let s = |t: &str| parse(t).map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string());
        assert_eq!(s("2013-12-23T23:15:00Z").as_deref(), Some("2013-12-23 23:15:00"));
        assert_eq!(s("2013-12-23T23:15:00-08:00").as_deref(), Some("2013-12-24 07:15:00"));
        assert_eq!(s("2013-12-23T23:15:00+05:30").as_deref(), Some("2013-12-23 17:45:00"));
        assert_eq!(s("2003").as_deref(), Some("2003-01-01 00:00:00"));
        assert_eq!(s("2003-12").as_deref(), Some("2003-12-01 00:00:00"));
        assert_eq!(s("2013-12-23T23:15:00.123Z").as_deref(), Some("2013-12-23 23:15:00"));
        assert_eq!(s("not a date"), None);
        // Expected values from python-docx 1.2.0: a 5-character tail is not an offset and is
        // ignored; a 6-character tail that is not `±hh:mm` raises ValueError.
        assert_eq!(s("2013-12-23T23:15:00xx:yy").as_deref(), Some("2013-12-23 23:15:00"));
        assert_eq!(s("2013-12-23T23:15:00xxx:yy"), None);
    }
}
