//! `datetime.utcnow()` as the provenance modules use it.
//!
//! The original reads the wall clock through `datetime.utcnow()` in `schemas.py` (the
//! `ProvenanceEntry.timestamp` default), `manager.py` and `bridge_axiom.py`. Every such read in
//! this crate goes through [`utcnow`], which a thread may redirect to a deterministic clock (the
//! Rust counterpart of patching `datetime` in a test). Production code never installs one.

use std::cell::RefCell;

use chrono::{NaiveDateTime, TimeDelta, Timelike, Utc};
use sem_utils::pycompat::time::PyDateTime;

/// A replacement clock: returns the next `datetime.utcnow()` value.
pub type ClockFn = Box<dyn FnMut() -> NaiveDateTime>;

thread_local! {
    static CLOCK: RefCell<Option<ClockFn>> = const { RefCell::new(None) };
}

/// `datetime.utcnow()`: the current UTC time as a naive datetime (microsecond precision).
pub fn utcnow() -> NaiveDateTime {
    let overridden = CLOCK.with(|clock| clock.borrow_mut().as_mut().map(|next| next()));
    let now = overridden.unwrap_or_else(|| Utc::now().naive_utc());
    // Python datetimes carry microseconds, not nanoseconds.
    let nanos = (now.nanosecond() / 1000) * 1000;
    now.with_nanosecond(nanos).unwrap_or(now)
}

/// `datetime.utcnow().isoformat()` (e.g. `2026-01-01T00:00:01.250000`, no fraction when the
/// microseconds are zero).
pub fn utcnow_isoformat() -> String {
    PyDateTime::Naive(utcnow()).isoformat('T')
}

/// Installs `clock` (or, with `None`, the wall clock) for the current thread and returns the
/// clock that was installed before.
pub fn set_thread_clock(clock: Option<ClockFn>) -> Option<ClockFn> {
    CLOCK.with(|slot| std::mem::replace(&mut *slot.borrow_mut(), clock))
}

/// Restores the previously installed clock of this thread when dropped.
#[must_use = "the clock is uninstalled when the guard is dropped"]
pub struct ClockGuard {
    previous: Option<Option<ClockFn>>,
}

impl Drop for ClockGuard {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.take() {
            set_thread_clock(previous);
        }
    }
}

/// Installs, for the current thread, a clock whose `n`-th reading (counting from 0) is
/// `start + n * step`. The previous clock comes back when the guard is dropped.
pub fn fixed_clock(start: NaiveDateTime, step: TimeDelta) -> ClockGuard {
    let mut n: i32 = 0;
    let previous = set_thread_clock(Some(Box::new(move || {
        let value = start + step * n;
        n += 1;
        value
    })));
    ClockGuard {
        previous: Some(previous),
    }
}
