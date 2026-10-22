//! Test-support diagnostic trace. Calls and emissions are recorded before
//! callbacks; normal returns update them. Unwinding leaves actual progress intact.
use serde_json::{Value, json};
use std::{cell::RefCell, fmt::Display, rc::Rc};

#[derive(Clone, Default)]
pub(super) struct LoaderTrace(Rc<RefCell<Vec<Value>>>);
impl LoaderTrace {
    pub(super) fn begin(&self, iri: &str) -> ActiveLoad {
        let ordinal = {
            let mut calls = self.0.borrow_mut();
            let ordinal = calls.len();
            calls.push(
                json!({"ordinal":ordinal,"iri":iri,"emissions":[],"status":"started","error":null}),
            );
            ordinal
        };
        ActiveLoad {
            trace: self.clone(),
            ordinal,
        }
    }
    pub(super) fn snapshot(&self) -> Vec<Value> {
        self.0.borrow().clone()
    }
}

pub(super) struct ActiveLoad {
    trace: LoaderTrace,
    ordinal: usize,
}
impl ActiveLoad {
    pub(super) fn emission<T, E: Display>(
        &self,
        terms: Value,
        emit: impl FnOnce() -> Result<T, E>,
    ) -> Result<T, E> {
        let ordinal = {
            let mut calls = self.trace.0.borrow_mut();
            let emissions = calls[self.ordinal]["emissions"].as_array_mut().unwrap();
            let ordinal = emissions.len();
            emissions.push(
                json!({"ordinal":ordinal,"terms":terms,"status":"emit_started","error":null}),
            );
            ordinal
        };
        // No RefCell borrow crosses the callback, including a panic or a nested load.
        let result = emit();
        let mut calls = self.trace.0.borrow_mut();
        let emission = &mut calls[self.ordinal]["emissions"][ordinal];
        emission["status"] = json!(if result.is_ok() {
            "emitted"
        } else {
            "emit_error"
        });
        emission["error"] = json!(result.as_ref().err().map(ToString::to_string));
        result
    }
    pub(super) fn finish<E: Display>(&self, result: &Result<bool, E>) {
        let mut calls = self.trace.0.borrow_mut();
        let call = &mut calls[self.ordinal];
        call["status"] = json!(match result {
            Ok(true) => "loaded",
            Ok(false) => "observed_absent",
            Err(_) => "loader_error",
        });
        call["error"] = json!(result.as_ref().err().map(ToString::to_string));
    }
}
