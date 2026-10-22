//! Python keyword arguments for the pipeline API (no counterpart file in the original).
//!
//! The original moves configuration and execution options around as `**kwargs` dicts whose
//! values can be any Python object: plain data, step handlers (callables) and collaborators
//! such as a version manager or a triplet store, which flow untouched from
//! `ExecutionEngine.execute_pipeline(pipeline, data, **options)` into
//! `step.handler(data, **step.config, **options)`. [`Kwargs`] is such a dict (insertion
//! ordered, like a Python dict) and [`KwargValue`] one of its values.
//!
//! The module also holds the bits of Python calling behaviour the pipeline relies on:
//! merging `**a, **b` into one call (with CPython 3.11's duplicate-key errors), the
//! "exception currently being handled" that `traceback.format_exc()` reads, and the
//! `config=None, **kwargs` constructor convention.

use std::any::Any;
use std::cell::RefCell;
use std::fmt;
use std::sync::Arc;

use indexmap::IndexMap;
use sem_utils::SemanticaError;
use sem_utils::pycompat::fmt::{float_repr, str_repr};
use serde_json::{Map, Number, Value};

/// A Python `dict` of keyword arguments (`**kwargs`), in insertion order.
pub type Kwargs = IndexMap<String, KwargValue>;

/// Build a [`Kwargs`] dict: `kwargs! {"handler" => h, "dependencies" => ["a"], "x" => 1}`.
///
/// Every value goes through [`KwargValue::from`].
#[macro_export]
macro_rules! kwargs {
    () => {
        $crate::Kwargs::new()
    };
    ($($key:expr => $value:expr),+ $(,)?) => {{
        let mut map = $crate::Kwargs::new();
        $(
            map.insert(
                ::std::string::String::from($key),
                $crate::KwargValue::from($value),
            );
        )+
        map
    }};
}

/// One value of a [`Kwargs`] dict: Python data, a step handler, or any other object passed
/// through untouched.
#[derive(Clone, Default)]
pub enum KwargValue {
    /// `None`.
    #[default]
    None,
    /// A `bool`.
    Bool(bool),
    /// An `int`.
    Int(i64),
    /// A `float`.
    Float(f64),
    /// A `str`.
    Str(String),
    /// A `list` (or `tuple`).
    List(Vec<KwargValue>),
    /// A `dict` with string keys.
    Dict(Kwargs),
    /// A callable step handler (`handler=...`).
    Handler(StepHandler),
    /// Any other object (a service client, a registry, an enum member, ...).
    Object(PyObject),
}

impl KwargValue {
    /// Wrap any value as an opaque object (see [`PyObject::new`]).
    pub fn object<T: Any + Send + Sync>(value: T) -> Self {
        KwargValue::Object(PyObject::new(value))
    }

    /// The Python value a JSON document decodes to (`json.loads`).
    pub fn from_json(value: Value) -> Self {
        match value {
            Value::Null => KwargValue::None,
            Value::Bool(b) => KwargValue::Bool(b),
            Value::Number(n) => number_value(&n),
            Value::String(s) => KwargValue::Str(s),
            Value::Array(items) => {
                KwargValue::List(items.into_iter().map(KwargValue::from_json).collect())
            }
            Value::Object(map) => KwargValue::Dict(
                map.into_iter()
                    .map(|(k, v)| (k, KwargValue::from_json(v)))
                    .collect(),
            ),
        }
    }

    /// `type(value).__name__`.
    pub fn type_name(&self) -> String {
        match self {
            KwargValue::None => "NoneType".to_string(),
            KwargValue::Bool(_) => "bool".to_string(),
            KwargValue::Int(_) => "int".to_string(),
            KwargValue::Float(_) => "float".to_string(),
            KwargValue::Str(_) => "str".to_string(),
            KwargValue::List(_) => "list".to_string(),
            KwargValue::Dict(_) => "dict".to_string(),
            KwargValue::Handler(_) => "function".to_string(),
            KwargValue::Object(obj) => obj.type_name().to_string(),
        }
    }

    /// Python truthiness (`bool(value)`); handlers and objects are truthy.
    pub fn truthy(&self) -> bool {
        match self {
            KwargValue::None => false,
            KwargValue::Bool(b) => *b,
            KwargValue::Int(i) => *i != 0,
            KwargValue::Float(f) => *f != 0.0,
            KwargValue::Str(s) => !s.is_empty(),
            KwargValue::List(items) => !items.is_empty(),
            KwargValue::Dict(map) => !map.is_empty(),
            KwargValue::Handler(_) | KwargValue::Object(_) => true,
        }
    }

    /// `value is None`.
    pub fn is_none(&self) -> bool {
        matches!(self, KwargValue::None)
    }

    /// The text of a `str`.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            KwargValue::Str(s) => Some(s),
            _ => None,
        }
    }

    /// A `bool`.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            KwargValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// An `int` (a `bool` counts as `0`/`1`, as in Python).
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            KwargValue::Int(i) => Some(*i),
            KwargValue::Bool(b) => Some(i64::from(*b)),
            _ => None,
        }
    }

    /// Any number as a float (`float(x)` for `int`, `float` and `bool`).
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            KwargValue::Int(i) => Some(*i as f64),
            KwargValue::Float(f) => Some(*f),
            KwargValue::Bool(b) => Some(f64::from(u8::from(*b))),
            _ => None,
        }
    }

    /// The items of a `list`.
    pub fn as_list(&self) -> Option<&[KwargValue]> {
        match self {
            KwargValue::List(items) => Some(items),
            _ => None,
        }
    }

    /// The entries of a `dict`.
    pub fn as_dict(&self) -> Option<&Kwargs> {
        match self {
            KwargValue::Dict(map) => Some(map),
            _ => None,
        }
    }

    /// The entries of a `dict`, mutably.
    pub fn as_dict_mut(&mut self) -> Option<&mut Kwargs> {
        match self {
            KwargValue::Dict(map) => Some(map),
            _ => None,
        }
    }

    /// A step handler.
    pub fn as_handler(&self) -> Option<&StepHandler> {
        match self {
            KwargValue::Handler(h) => Some(h),
            _ => None,
        }
    }

    /// An opaque object.
    pub fn as_object(&self) -> Option<&PyObject> {
        match self {
            KwargValue::Object(obj) => Some(obj),
            _ => None,
        }
    }

    /// The object wrapped by [`KwargValue::object`], if it has type `T`.
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        self.as_object().and_then(PyObject::downcast_ref)
    }

    /// `value.get(key)` on a `dict` (`None` for missing keys and non-dicts).
    pub fn get(&self, key: &str) -> Option<&KwargValue> {
        self.as_dict().and_then(|map| map.get(key))
    }

    /// `str(value)`.
    pub fn py_str(&self) -> String {
        match self {
            KwargValue::Str(s) => s.clone(),
            other => other.py_repr(),
        }
    }

    /// `repr(value)`.
    pub fn py_repr(&self) -> String {
        match self {
            KwargValue::None => "None".to_string(),
            KwargValue::Bool(true) => "True".to_string(),
            KwargValue::Bool(false) => "False".to_string(),
            KwargValue::Int(i) => i.to_string(),
            KwargValue::Float(f) => float_repr(*f),
            KwargValue::Str(s) => str_repr(s),
            KwargValue::List(items) => {
                let parts: Vec<String> = items.iter().map(KwargValue::py_repr).collect();
                format!("[{}]", parts.join(", "))
            }
            KwargValue::Dict(map) => kwargs_repr(map),
            KwargValue::Handler(h) => format!("{h:?}"),
            KwargValue::Object(obj) => format!("{obj:?}"),
        }
    }

    /// The JSON form `json.dumps` writes, or its `TypeError` for a handler or object
    /// (`Object of type function is not JSON serializable`).
    pub fn to_json(&self) -> Result<Value, SemanticaError> {
        self.to_json_with(&mut |v| {
            Err(SemanticaError::Type(format!(
                "Object of type {} is not JSON serializable",
                v.type_name()
            )))
        })
    }

    /// The JSON form `json.dumps(..., default=str)` writes: handlers and objects become their
    /// `str()`.
    pub fn to_json_default_str(&self) -> Value {
        self.to_json_with(&mut |v| Ok(Value::String(v.py_str())))
            .unwrap_or(Value::Null)
    }

    fn to_json_with(
        &self,
        default: &mut dyn FnMut(&KwargValue) -> Result<Value, SemanticaError>,
    ) -> Result<Value, SemanticaError> {
        Ok(match self {
            KwargValue::None => Value::Null,
            KwargValue::Bool(b) => Value::Bool(*b),
            KwargValue::Int(i) => Value::from(*i),
            KwargValue::Float(f) => Number::from_f64(*f).map_or(Value::Null, Value::Number),
            KwargValue::Str(s) => Value::String(s.clone()),
            KwargValue::List(items) => Value::Array(
                items
                    .iter()
                    .map(|item| item.to_json_with(default))
                    .collect::<Result<_, _>>()?,
            ),
            KwargValue::Dict(map) => {
                let mut out = Map::new();
                for (k, v) in map {
                    out.insert(k.clone(), v.to_json_with(default)?);
                }
                Value::Object(out)
            }
            KwargValue::Handler(_) | KwargValue::Object(_) => default(self)?,
        })
    }

    /// Python `==`: numbers compare by value (`1 == 1.0 == True`), containers element-wise
    /// (dicts ignore order), handlers and objects by identity.
    pub fn py_eq(&self, other: &KwargValue) -> bool {
        use KwargValue as K;
        match (self, other) {
            (K::None, K::None) => true,
            (K::Str(a), K::Str(b)) => a == b,
            (K::List(a), K::List(b)) => {
                a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.py_eq(y))
            }
            (K::Dict(a), K::Dict(b)) => {
                a.len() == b.len() && a.iter().all(|(k, v)| b.get(k).is_some_and(|w| v.py_eq(w)))
            }
            (K::Handler(a), K::Handler(b)) => a.ptr_eq(b),
            (K::Object(a), K::Object(b)) => a.ptr_eq(b),
            (K::Int(a), K::Int(b)) => a == b,
            (a, b) => match (a.as_f64(), b.as_f64()) {
                (Some(x), Some(y)) => x == y,
                _ => false,
            },
        }
    }
}

fn number_value(n: &Number) -> KwargValue {
    if let Some(i) = n.as_i64() {
        KwargValue::Int(i)
    } else {
        KwargValue::Float(n.as_f64().unwrap_or(f64::NAN))
    }
}

/// `repr()` of a dict of keyword arguments (`{'a': 1, 'b': 'x'}`).
pub fn kwargs_repr(map: &Kwargs) -> String {
    let parts: Vec<String> = map
        .iter()
        .map(|(k, v)| format!("{}: {}", str_repr(k), v.py_repr()))
        .collect();
    format!("{{{}}}", parts.join(", "))
}

/// The [`Kwargs`] a JSON object decodes to (non-objects give an empty dict).
pub fn kwargs_from_json(value: Value) -> Kwargs {
    match KwargValue::from_json(value) {
        KwargValue::Dict(map) => map,
        _ => Kwargs::new(),
    }
}

/// The JSON object `json.dumps` writes for a dict of keyword arguments, or its `TypeError`.
pub fn kwargs_to_json(map: &Kwargs) -> Result<Map<String, Value>, SemanticaError> {
    let mut out = Map::new();
    for (k, v) in map {
        out.insert(k.clone(), v.to_json()?);
    }
    Ok(out)
}

impl fmt::Debug for KwargValue {
    /// `repr(value)`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.py_repr())
    }
}

impl fmt::Display for KwargValue {
    /// `str(value)`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.py_str())
    }
}

impl PartialEq for KwargValue {
    fn eq(&self, other: &KwargValue) -> bool {
        self.py_eq(other)
    }
}

impl PartialEq<Value> for KwargValue {
    fn eq(&self, other: &Value) -> bool {
        self.py_eq(&KwargValue::from_json(other.clone()))
    }
}

impl PartialEq<str> for KwargValue {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == Some(other)
    }
}

impl PartialEq<&str> for KwargValue {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == Some(*other)
    }
}

impl PartialEq<i64> for KwargValue {
    fn eq(&self, other: &i64) -> bool {
        self.py_eq(&KwargValue::Int(*other))
    }
}

impl PartialEq<f64> for KwargValue {
    fn eq(&self, other: &f64) -> bool {
        self.py_eq(&KwargValue::Float(*other))
    }
}

impl PartialEq<bool> for KwargValue {
    fn eq(&self, other: &bool) -> bool {
        self.py_eq(&KwargValue::Bool(*other))
    }
}

impl From<bool> for KwargValue {
    fn from(value: bool) -> Self {
        KwargValue::Bool(value)
    }
}

macro_rules! int_from {
    ($($t:ty),*) => {$(
        impl From<$t> for KwargValue {
            fn from(value: $t) -> Self {
                KwargValue::Int(i64::from(value))
            }
        }
    )*};
}
int_from!(i8, i16, i32, i64, u8, u16, u32);

impl From<u64> for KwargValue {
    fn from(value: u64) -> Self {
        i64::try_from(value).map_or(KwargValue::Float(value as f64), KwargValue::Int)
    }
}

impl From<usize> for KwargValue {
    fn from(value: usize) -> Self {
        KwargValue::from(value as u64)
    }
}

impl From<f32> for KwargValue {
    fn from(value: f32) -> Self {
        KwargValue::Float(f64::from(value))
    }
}

impl From<f64> for KwargValue {
    fn from(value: f64) -> Self {
        KwargValue::Float(value)
    }
}

impl From<&str> for KwargValue {
    fn from(value: &str) -> Self {
        KwargValue::Str(value.to_string())
    }
}

impl From<String> for KwargValue {
    fn from(value: String) -> Self {
        KwargValue::Str(value)
    }
}

impl From<&String> for KwargValue {
    fn from(value: &String) -> Self {
        KwargValue::Str(value.clone())
    }
}

impl From<Value> for KwargValue {
    fn from(value: Value) -> Self {
        KwargValue::from_json(value)
    }
}

impl From<&Value> for KwargValue {
    fn from(value: &Value) -> Self {
        KwargValue::from_json(value.clone())
    }
}

impl<T: Into<KwargValue>> From<Vec<T>> for KwargValue {
    fn from(value: Vec<T>) -> Self {
        KwargValue::List(value.into_iter().map(Into::into).collect())
    }
}

impl<T: Into<KwargValue>, const N: usize> From<[T; N]> for KwargValue {
    fn from(value: [T; N]) -> Self {
        KwargValue::List(value.into_iter().map(Into::into).collect())
    }
}

impl<T: Into<KwargValue>> From<Option<T>> for KwargValue {
    fn from(value: Option<T>) -> Self {
        value.map_or(KwargValue::None, Into::into)
    }
}

impl From<Kwargs> for KwargValue {
    fn from(value: Kwargs) -> Self {
        KwargValue::Dict(value)
    }
}

impl From<StepHandler> for KwargValue {
    fn from(value: StepHandler) -> Self {
        KwargValue::Handler(value)
    }
}

impl From<PyObject> for KwargValue {
    fn from(value: PyObject) -> Self {
        KwargValue::Object(value)
    }
}

/// An opaque Python object carried in [`Kwargs`]: compared by identity, printed as
/// `<Type object at 0x...>`.
///
/// To pass a trait object, wrap the `Arc<dyn Trait>` itself
/// (`PyObject::new(arc_of_dyn_trait)`) and read it back with
/// `downcast_ref::<Arc<dyn Trait>>()`.
#[derive(Clone)]
pub struct PyObject {
    type_name: Arc<str>,
    value: Arc<dyn Any + Send + Sync>,
}

impl PyObject {
    /// Wrap `value`; its Python type name is the last path segment of `T`.
    pub fn new<T: Any + Send + Sync>(value: T) -> Self {
        PyObject::with_type_name(short_type_name::<T>(), value)
    }

    /// Wrap `value` under an explicit Python type name.
    pub fn with_type_name<T: Any + Send + Sync>(type_name: impl Into<String>, value: T) -> Self {
        PyObject {
            type_name: Arc::from(type_name.into()),
            value: Arc::new(value),
        }
    }

    /// `type(obj).__name__`.
    pub fn type_name(&self) -> &str {
        &self.type_name
    }

    /// The wrapped value, if it has type `T`.
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        self.value.downcast_ref::<T>()
    }

    /// `a is b`.
    pub fn ptr_eq(&self, other: &PyObject) -> bool {
        Arc::ptr_eq(&self.value, &other.value)
    }

    fn address(&self) -> usize {
        Arc::as_ptr(&self.value) as *const () as usize
    }
}

impl fmt::Debug for PyObject {
    /// The default `object.__repr__`: `<Type object at 0x...>`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<{} object at {:#x}>", self.type_name, self.address())
    }
}

fn short_type_name<T>() -> String {
    let full = std::any::type_name::<T>();
    let base = full.split('<').next().unwrap_or(full);
    base.rsplit("::").next().unwrap_or(base).to_string()
}

/// The signature of a step handler: `handler(data, **kwargs)`.
pub type StepFn = dyn Fn(Value, &Kwargs) -> Result<Value, SemanticaError> + Send + Sync;

/// A pipeline step handler, the Rust form of the callable the original stores in
/// `PipelineStep.handler`: it receives the data flowing through the pipeline and the step
/// configuration merged with the execution options as keyword arguments.
#[derive(Clone)]
pub struct StepHandler {
    name: Arc<str>,
    func: Arc<StepFn>,
}

impl StepHandler {
    /// A handler called `name` (used where Python prints the function, e.g. `repr` and the
    /// duplicate-keyword `TypeError`).
    pub fn new<F>(name: impl Into<String>, func: F) -> Self
    where
        F: Fn(Value, &Kwargs) -> Result<Value, SemanticaError> + Send + Sync + 'static,
    {
        StepHandler {
            name: Arc::from(name.into()),
            func: Arc::new(func),
        }
    }

    /// `lambda d, **kwargs: d`.
    pub fn identity() -> Self {
        StepHandler::new("<lambda>", |data, _| Ok(data))
    }

    /// The stand-in for a truthy non-callable `handler=` value: calling it raises
    /// `TypeError: '<type>' object is not callable`, as calling that value does in Python.
    pub(crate) fn not_callable(type_name: &str) -> Self {
        let message = format!("'{type_name}' object is not callable");
        StepHandler::new(type_name.to_string(), move |_, _| {
            Err(SemanticaError::Type(message.clone()))
        })
    }

    /// The function name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// `handler(data, **kwargs)`.
    pub fn call(&self, data: Value, kwargs: &Kwargs) -> Result<Value, SemanticaError> {
        (self.func)(data, kwargs)
    }

    /// `a is b`.
    pub fn ptr_eq(&self, other: &StepHandler) -> bool {
        Arc::ptr_eq(&self.func, &other.func)
    }

    fn address(&self) -> usize {
        Arc::as_ptr(&self.func) as *const () as usize
    }
}

impl fmt::Debug for StepHandler {
    /// `<function name at 0x...>`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<function {} at {:#x}>", self.name, self.address())
    }
}

impl fmt::Display for StepHandler {
    /// `str(function)` (the same as its `repr`).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

thread_local! {
    static HANDLED: RefCell<Vec<SemanticaError>> = const { RefCell::new(Vec::new()) };
}

/// Marks the code it guards as running inside `except ... as e:` for `e` (see
/// [`handling`]); dropping it leaves the `except` block.
pub(crate) struct HandlingGuard(());

impl Drop for HandlingGuard {
    fn drop(&mut self) {
        HANDLED.with(|h| {
            h.borrow_mut().pop();
        });
    }
}

/// Enter an `except` block for `error`: until the guard drops, it is the exception
/// `traceback.format_exc()` reports on this thread.
pub(crate) fn handling(error: &SemanticaError) -> HandlingGuard {
    HANDLED.with(|h| h.borrow_mut().push(error.clone()));
    HandlingGuard(())
}

/// The exception being handled on this thread (`sys.exc_info()[1]`).
pub(crate) fn handled_exception() -> Option<SemanticaError> {
    HANDLED.with(|h| h.borrow().last().cloned())
}

/// `traceback.format_exc()`: `NoneType: None` outside an `except` block, else the handled
/// exception in the frameless form `sem_utils` uses for tracebacks.
pub(crate) fn format_exc() -> String {
    match handled_exception() {
        Some(error) => error.traceback_text(),
        None => "NoneType: None\n".to_string(),
    }
}

/// The keyword arguments of `func(..., **parts[0], **parts[1], ...)`.
///
/// A key given twice raises CPython 3.11's `TypeError: func() got multiple values for keyword
/// argument 'key'` — except while an exception is being handled, where CPython 3.11 leaks the
/// bare `KeyError('key')` instead (the `DICT_MERGE` error is normalised for exception
/// chaining before it can be rewritten).
pub(crate) fn merge_call_kwargs(func: &str, parts: &[&Kwargs]) -> Result<Kwargs, SemanticaError> {
    let mut merged = Kwargs::new();
    for part in parts {
        for (key, value) in part.iter() {
            if merged.contains_key(key) {
                return Err(if handled_exception().is_some() {
                    SemanticaError::Key(key.clone())
                } else {
                    SemanticaError::Type(format!(
                        "{func}() got multiple values for keyword argument '{key}'"
                    ))
                });
            }
            merged.insert(key.clone(), value.clone());
        }
    }
    Ok(merged)
}

/// `repr()` of a bool.
pub(crate) fn bool_repr(value: bool) -> &'static str {
    if value { "True" } else { "False" }
}

/// `repr()` of an optional string (`None` or `'text'`).
pub(crate) fn opt_str_repr(value: Option<&str>) -> String {
    value.map_or_else(|| "None".to_string(), str_repr)
}

/// `repr()` of a dict of JSON values.
pub(crate) fn json_map_repr(map: &Map<String, Value>) -> String {
    sem_utils::pycompat::fmt::map_repr(map)
}

/// Python's binding of keyword arguments to a named parameter: a Rust caller passes the
/// parameter explicitly and may also pass it inside `kwargs`; Python would bind the keyword
/// to the parameter, and reject the call when it is given both ways.
pub(crate) fn bind_keyword(
    func: &str,
    explicit: Option<KwargValue>,
    kwargs: &mut Kwargs,
    name: &str,
) -> Result<Option<KwargValue>, SemanticaError> {
    match (explicit, kwargs.shift_remove(name)) {
        (Some(_), Some(_)) => Err(SemanticaError::Type(format!(
            "{func}() got multiple values for keyword argument '{name}'"
        ))),
        (Some(value), None) | (None, Some(value)) => Ok(Some(value)),
        (None, None) => Ok(None),
    }
}

/// `self.config = config or {}; self.config.update(kwargs)`.
pub(crate) fn init_config(config: Option<Kwargs>, kwargs: Kwargs) -> Kwargs {
    let mut merged = config.unwrap_or_default();
    for (key, value) in kwargs {
        merged.insert(key, value);
    }
    merged
}

/// How `Class(**kwargs)` binds to `__init__(self, config=None, **kwargs)`: a `config` entry
/// becomes the `config` argument (a non-dict counts as `None`), the rest stay keyword
/// arguments.
pub(crate) fn split_config(kwargs: &Kwargs) -> (Option<Kwargs>, Kwargs) {
    let mut rest = kwargs.clone();
    let config = match rest.shift_remove("config") {
        Some(KwargValue::Dict(map)) if !map.is_empty() => Some(map),
        _ => None,
    };
    (config, rest)
}
