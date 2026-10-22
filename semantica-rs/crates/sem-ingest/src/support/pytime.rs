//! `time.time()` / `time.sleep()` with per-thread replacements ([`TIME`], [`SLEEP`]), the Rust
//! form of `patch("time.time")` / `patch("time.sleep")`.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::hooks::Hook;

/// Replacement for `time.time()`.
pub static TIME: Hook<Box<dyn Fn() -> f64>> = Hook::new("time.time");

/// Replacement for `time.sleep(seconds)`.
pub static SLEEP: Hook<Box<dyn Fn(f64)>> = Hook::new("time.sleep");

/// `time.time()`.
pub fn time() -> f64 {
    if let Some(f) = TIME.get() {
        return f();
    }
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// `time.sleep(seconds)` (negative values sleep for zero seconds instead of raising).
pub fn sleep(seconds: f64) {
    if let Some(f) = SLEEP.get() {
        f(seconds);
        return;
    }
    if seconds > 0.0 && seconds.is_finite() {
        std::thread::sleep(Duration::from_secs_f64(seconds));
    }
}

/// Python `str.splitlines()` (all Unicode line boundaries, `\r\n` as one).
pub fn splitlines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut iter = text.char_indices().peekable();
    while let Some((i, c)) = iter.next() {
        let is_break = matches!(
            c,
            '\n' | '\r' | '\x0b' | '\x0c' | '\x1c' | '\x1d' | '\x1e' | '\u{85}' | '\u{2028}' | '\u{2029}'
        );
        if is_break {
            lines.push(&text[start..i]);
            let mut next = i + c.len_utf8();
            if c == '\r'
                && let Some(&(j, '\n')) = iter.peek()
            {
                iter.next();
                next = j + 1;
            }
            start = next;
        }
    }
    if start < text.len() {
        lines.push(&text[start..]);
    }
    lines
}
