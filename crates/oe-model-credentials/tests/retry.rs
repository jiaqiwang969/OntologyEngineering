use std::time::Duration;

use oe_model_credentials::retry_after_at;

#[test]
fn retry_after_keeps_fractional_seconds_and_utc_http_dates() {
    let now = httpdate::parse_http_date("Sun, 27 Sep 2026 00:00:00 GMT").unwrap();
    let cases = [
        ("Sun, 27 Sep 2026 00:01:00 GMT", 60.0),
        ("Sun, 27 Sep 2026 02:01:00 +0200", 60.0),
        ("Sun Sep 27 00:01:00 2026", 60.0),
        ("Sunday, 27-Sep-26 00:01:00 GMT", 60.0),
        ("Sat, 26 Sep 2026 23:59:00 GMT", 0.0),
        (" 2.5 ", 2.5),
        ("0", 0.0),
    ];
    for (header, expected) in cases {
        assert_eq!(
            retry_after_at(Some(header), now),
            Some(expected),
            "{header}"
        );
    }
    assert_eq!(
        retry_after_at(
            Some("Sun, 27 Sep 2026 00:01:00 GMT"),
            now + Duration::from_secs(120)
        ),
        Some(0.0)
    );
}

#[test]
fn invalid_or_nonfinite_delays_are_not_used() {
    let now = std::time::SystemTime::now();
    let mut headers = vec![
        "not a date".to_owned(),
        "Sun, 99 Sep 2026 00:01:00 GMT".to_owned(),
        "NaN".to_owned(),
        "Infinity".to_owned(),
        "1e309".to_owned(),
        "-1".to_owned(),
        "+1".to_owned(),
        "1.".to_owned(),
        ".1".to_owned(),
        "1.2.3".to_owned(),
        "".to_owned(),
        " ".to_owned(),
        "Sun, 27 Sep 2026 02:01:00 +9960".to_owned(),
    ];
    headers.push("9".repeat(400));
    for header in headers {
        assert_eq!(retry_after_at(Some(&header), now), None);
    }
    assert_eq!(retry_after_at(None, now), None);
}
