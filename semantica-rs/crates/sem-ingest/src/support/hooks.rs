//! Per-thread replacement points: the Rust counterpart of `unittest.mock.patch` on a class,
//! a constructor or a module-level dependency (`patch("...WebIngestor.ingest_url")`,
//! `patch("boto3.client")`, `patch("time.sleep")`). Production code asks [`Hook::get`] and falls
//! back to the real implementation; a caller installs a replacement with [`Hook::set`] and it
//! stays active on the current thread until the returned guard is dropped.

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::rc::Rc;

thread_local! {
    static REGISTRY: RefCell<HashMap<&'static str, Vec<Rc<dyn Any>>>> = RefCell::new(HashMap::new());
}

/// A named replacement point holding values of type `T` (usually `Box<dyn Fn(..)>`).
pub struct Hook<T: 'static> {
    name: &'static str,
    _marker: PhantomData<fn() -> T>,
}

/// Removes the replacement on drop.
pub struct HookGuard {
    name: &'static str,
}

impl Drop for HookGuard {
    fn drop(&mut self) {
        REGISTRY.with(|r| {
            if let Some(stack) = r.borrow_mut().get_mut(self.name) {
                stack.pop();
            }
        });
    }
}

impl<T: 'static> Hook<T> {
    /// Declares a replacement point.
    pub const fn new(name: &'static str) -> Hook<T> {
        Hook {
            name,
            _marker: PhantomData,
        }
    }

    /// The point's name (`"module.Class.method"`).
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Installs `value` on the current thread until the guard is dropped (nested installs
    /// shadow earlier ones).
    pub fn set(&'static self, value: T) -> HookGuard {
        let value: Rc<dyn Any> = Rc::new(value);
        REGISTRY.with(|r| r.borrow_mut().entry(self.name).or_default().push(value));
        HookGuard { name: self.name }
    }

    /// The replacement in effect, if any.
    pub fn get(&self) -> Option<Rc<T>> {
        REGISTRY.with(|r| {
            r.borrow()
                .get(self.name)
                .and_then(|stack| stack.last().cloned())
                .and_then(|v| v.downcast::<T>().ok())
        })
    }

    /// Whether a replacement is installed.
    pub fn is_set(&self) -> bool {
        self.get().is_some()
    }
}
