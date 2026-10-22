//! A small stand-in for Python's `warnings` module: categories with an inheritance chain,
//! `warn()`, the default filters (deprecation-style warnings are ignored, others are shown once
//! per call site on stderr) and `catch_warnings()` recording for tests.
//!
//! Unlike Python, recording is per thread, so concurrently running tests do not see each
//! other's warnings.

use std::cell::RefCell;
use std::collections::HashSet;
use std::fmt;
use std::panic::Location;
use std::sync::Mutex;

use once_cell::sync::Lazy;

/// A warning class (`Warning` subclass), identified by name and parent class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WarningCategory {
    name: &'static str,
    parent: Option<&'static WarningCategory>,
}

impl WarningCategory {
    /// A new category deriving from `parent`.
    pub const fn new(name: &'static str, parent: &'static WarningCategory) -> Self {
        WarningCategory {
            name,
            parent: Some(parent),
        }
    }

    /// `category.__name__`.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// The parent class (`None` only for `Warning`).
    pub fn parent(&self) -> Option<&'static WarningCategory> {
        self.parent
    }

    /// `issubclass(self, other)`.
    pub fn is_subclass_of(&self, other: impl Into<WarningCategory>) -> bool {
        let other = other.into();
        let mut current = Some(*self);
        while let Some(category) = current {
            if category.name == other.name {
                return true;
            }
            current = category.parent.copied();
        }
        false
    }
}

impl From<&WarningCategory> for WarningCategory {
    fn from(value: &WarningCategory) -> Self {
        *value
    }
}

impl fmt::Display for WarningCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name)
    }
}

/// `Warning`.
pub static WARNING: WarningCategory = WarningCategory {
    name: "Warning",
    parent: None,
};
/// `UserWarning` (the default category of `warn`).
pub static USER_WARNING: WarningCategory = WarningCategory::new("UserWarning", &WARNING);
/// `DeprecationWarning` (ignored by the default filters).
pub static DEPRECATION_WARNING: WarningCategory =
    WarningCategory::new("DeprecationWarning", &WARNING);
/// `PendingDeprecationWarning` (ignored by the default filters).
pub static PENDING_DEPRECATION_WARNING: WarningCategory =
    WarningCategory::new("PendingDeprecationWarning", &WARNING);
/// `RuntimeWarning`.
pub static RUNTIME_WARNING: WarningCategory = WarningCategory::new("RuntimeWarning", &WARNING);
/// `FutureWarning`.
pub static FUTURE_WARNING: WarningCategory = WarningCategory::new("FutureWarning", &WARNING);
/// `ImportWarning` (ignored by the default filters).
pub static IMPORT_WARNING: WarningCategory = WarningCategory::new("ImportWarning", &WARNING);
/// `ResourceWarning` (ignored by the default filters).
pub static RESOURCE_WARNING: WarningCategory = WarningCategory::new("ResourceWarning", &WARNING);

/// One issued warning (`warnings.WarningMessage`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WarningMessage {
    /// `str(w.message)`.
    pub message: String,
    /// `w.category`.
    pub category: WarningCategory,
    /// Source file of the call site.
    pub filename: String,
    /// Line of the call site.
    pub lineno: u32,
}

impl fmt::Display for WarningMessage {
    /// The line `warnings.showwarning` prints (without the source line).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}: {}: {}",
            self.filename, self.lineno, self.category.name, self.message
        )
    }
}

thread_local! {
    static RECORDERS: RefCell<Vec<Vec<WarningMessage>>> = const { RefCell::new(Vec::new()) };
}

/// `(message, category, file, line)` of a warning already shown.
type ShownKey = (String, &'static str, String, u32);

static SHOWN: Lazy<Mutex<HashSet<ShownKey>>> = Lazy::new(|| Mutex::new(HashSet::new()));

/// `warnings.warn(message, category)`, attributed to the caller's source location.
#[track_caller]
pub fn warn(message: impl Into<String>, category: impl Into<WarningCategory>) {
    let location = Location::caller();
    let warning = WarningMessage {
        message: message.into(),
        category: category.into(),
        filename: location.file().to_string(),
        lineno: location.line(),
    };
    let recorded = RECORDERS.with(|recorders| {
        let mut recorders = recorders.borrow_mut();
        match recorders.last_mut() {
            Some(record) => {
                record.push(warning.clone());
                true
            }
            None => false,
        }
    });
    if recorded {
        return;
    }
    let ignored = [
        &DEPRECATION_WARNING,
        &PENDING_DEPRECATION_WARNING,
        &IMPORT_WARNING,
        &RESOURCE_WARNING,
    ];
    if ignored.iter().any(|c| warning.category.is_subclass_of(*c)) {
        return;
    }
    let key = (
        warning.message.clone(),
        warning.category.name,
        warning.filename.clone(),
        warning.lineno,
    );
    let first = SHOWN
        .lock()
        .map(|mut shown| shown.insert(key))
        .unwrap_or(true);
    if first {
        eprintln!("{warning}");
    }
}

/// `with warnings.catch_warnings(record=True): warnings.simplefilter("always"); f()`:
/// run `f` and return its result with every warning it issued on this thread.
pub fn catch_warnings<R>(f: impl FnOnce() -> R) -> (R, Vec<WarningMessage>) {
    RECORDERS.with(|recorders| recorders.borrow_mut().push(Vec::new()));
    struct Pop;
    impl Drop for Pop {
        fn drop(&mut self) {
            RECORDERS.with(|recorders| {
                recorders.borrow_mut().pop();
            });
        }
    }
    let guard = Pop;
    let result = f();
    let recorded = RECORDERS.with(|recorders| {
        recorders
            .borrow_mut()
            .last_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    });
    drop(guard);
    (result, recorded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_and_subclass_checks() {
        let ((), caught) = catch_warnings(|| {
            warn("first", USER_WARNING);
            warn("second", DEPRECATION_WARNING);
        });
        assert_eq!(caught.len(), 2);
        assert_eq!(caught[0].message, "first");
        assert!(caught[0].category.is_subclass_of(WARNING));
        assert!(!caught[1].category.is_subclass_of(USER_WARNING));
    }
}
