use std::time::{Duration, SystemTime};

pub fn retry_after(value: Option<&str>) -> Option<f64> {
    retry_after_at(value, SystemTime::now())
}

/// A finite delay, in seconds. No waiting or retry policy is imposed here.
pub fn retry_after_at(value: Option<&str>, now: SystemTime) -> Option<f64> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }
    let mut pieces = value.split('.');
    let whole = pieces.next()?;
    let fraction = pieces.next();
    let numeric = !whole.is_empty()
        && whole.bytes().all(|c| c.is_ascii_digit())
        && fraction.is_none_or(|p| !p.is_empty() && p.bytes().all(|c| c.is_ascii_digit()))
        && pieces.next().is_none();
    if numeric {
        return value.parse::<f64>().ok().filter(|v| v.is_finite());
    }
    let deadline = httpdate::parse_http_date(value)
        .ok()
        .or_else(|| offset_date(value))?;
    Some(
        deadline
            .duration_since(now)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0),
    )
}

// Retain the Python adapter's accepted explicit RFC timezone form as well as
// the three HTTP-date formats supported by httpdate (all otherwise UTC).
fn offset_date(value: &str) -> Option<SystemTime> {
    let (head, zone) = value.rsplit_once(' ')?;
    let bytes = zone.as_bytes();
    if bytes.len() != 5
        || !matches!(bytes[0], b'+' | b'-')
        || !bytes[1..].iter().all(u8::is_ascii_digit)
    {
        return None;
    }
    let hours: u64 = zone[1..3].parse().ok()?;
    let minutes: u64 = zone[3..5].parse().ok()?;
    if hours >= 24 || minutes >= 60 {
        return None;
    }
    let date = httpdate::parse_http_date(&format!("{head} GMT")).ok()?;
    let offset = Duration::from_secs(hours * 3600 + minutes * 60);
    if bytes[0] == b'+' {
        date.checked_sub(offset)
    } else {
        date.checked_add(offset)
    }
}
