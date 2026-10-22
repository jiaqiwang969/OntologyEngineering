//! Optional backends. Python imports `bs4`, `git`, `pyarrow`, `duckdb`, ... lazily and turns a
//! missing module into the documented `ImportError`/`ConfigurationError`. The Rust backends are
//! compiled in, so a module is "missing" only when an external executable it needs is absent
//! (`git`, `duckdb`) or when it is blocked on the current thread with [`block_modules`] — the
//! Rust form of the original tests' import blocker (`sys.meta_path` finder).

use std::cell::RefCell;

use sem_utils::SemanticaError;

thread_local! {
    static BLOCKED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Unblocks the modules on drop.
pub struct BlockedModulesGuard {
    previous: Vec<String>,
}

impl Drop for BlockedModulesGuard {
    fn drop(&mut self) {
        let previous = std::mem::take(&mut self.previous);
        BLOCKED.with(|b| *b.borrow_mut() = previous);
    }
}

/// Makes `import <name>` fail with `ModuleNotFoundError` on this thread until the guard is
/// dropped (top-level names, as the original blocker matches `fullname.split(".")[0]`).
pub fn block_modules(names: &[&str]) -> BlockedModulesGuard {
    let previous = BLOCKED.with(|b| {
        let mut blocked = b.borrow_mut();
        let previous = blocked.clone();
        blocked.extend(names.iter().map(|s| s.to_string()));
        previous
    });
    BlockedModulesGuard { previous }
}

/// Whether `name` (or its top-level package) is blocked on this thread.
pub fn is_blocked(name: &str) -> bool {
    let root = name.split('.').next().unwrap_or(name);
    BLOCKED.with(|b| b.borrow().iter().any(|m| m == root))
}

/// `ModuleNotFoundError(f"No module named '{name}'")` with `.name` = `name`.
pub fn module_not_found(name: &str) -> SemanticaError {
    SemanticaError::Other {
        module: "builtins".to_string(),
        type_name: "ModuleNotFoundError".to_string(),
        message: format!("No module named '{name}'"),
    }
}

/// `exc.name` of a `ModuleNotFoundError`.
pub fn missing_module_name(error: &SemanticaError) -> Option<String> {
    match error {
        SemanticaError::Other {
            type_name, message, ..
        } if type_name == "ModuleNotFoundError" => message
            .strip_prefix("No module named '")
            .and_then(|s| s.strip_suffix('\''))
            .map(str::to_string),
        _ => None,
    }
}

/// `import <name>`: fails when blocked.
pub fn import_module(name: &str) -> Result<(), SemanticaError> {
    if is_blocked(name) {
        let root = name.split('.').next().unwrap_or(name);
        return Err(module_not_found(root));
    }
    Ok(())
}

/// Whether an executable is on `PATH`.
pub fn executable_available(program: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| {
        let candidate = dir.join(program);
        candidate.is_file()
    })
}
