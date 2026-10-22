//! Original inputs/outcomes frozen before candidate execution. Development
//! replay of the source-callable boundary, not independent method acceptance.
use super::*;
use crate::numeric::source_registry::*;
use crate::numeric::{SourceDatatype, SourceFact, SourceLiteralCacheKind, SourceLiteralMetadata};
use crate::tuple::{
    ErrorValue, SourceException, SourceExceptionArgument, SourceExceptionClass, SparqlErrorKind,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
fn private_fixture() -> &'static str {
    crate::private_test_fixtures::text_ref!(
        "release-inputs-03/vendor/spareval/src/eval/fixtures/source_function_registry_query_cases.json"
    )
}
fn fixture() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(private_fixture().as_bytes())),
        "0a36d7cce09e2dcc88fbf781958003b9d086c376c337e619b8ec0374bc51328d"
    );
    let value: Value = serde_json::from_str(private_fixture()).unwrap();
    assert_eq!(value["candidate_results_used"], false);
    assert_eq!(value["graph_queries"].as_array().unwrap().len(), 75);
    value
}
pub(super) fn decimal_classes_json(signals: &[semantica_numeric::DecimalSignal]) -> Value {
    json!(signals.iter().map(|s| {
        let class=format!("{s:?}");
        json!({"kind":"Class","module":"decimal","class_name":class,"qualified_name":class,"repr":format!("<class 'decimal.{class}'>")})
    }).collect::<Vec<_>>())
}
pub(super) fn sparql_json(error: &ErrorValue) -> Value {
    let source = error
        .source_exception()
        .expect("original error constructor");
    let class = match source.class {
        SourceExceptionClass::Sparql => "SPARQLError",
        SourceExceptionClass::SparqlType => "SPARQLTypeError",
        SourceExceptionClass::NotBound => "NotBoundError",
        SourceExceptionClass::AlreadyBound => "AlreadyBound",
    };
    let argument = match &source.argument {
        SourceExceptionArgument::None => Value::Null,
        SourceExceptionArgument::Text(s) => json!(s),
        SourceExceptionArgument::DecimalSignalClasses(signals) => decimal_classes_json(signals),
    };
    json!({"kind":"Exception","module":"rdflib.plugins.sparql.sparql","class_name":class,"args":[argument]})
}
pub(super) fn exception_json(error: &SourceFunctionException) -> Value {
    let session = source_bgp_receiving::session();
    let args=error.arguments.iter().map(|v|match v {
        SourceFunctionArgument::None=>Value::Null,
        SourceFunctionArgument::Text(v)=>json!(v),
        SourceFunctionArgument::DecimalSignalClasses(signals)=>decimal_classes_json(signals),
        SourceFunctionArgument::DecimalDivisionImpossible=>json!([{"kind":"Class","module":"decimal","class_name":"DivisionImpossible","qualified_name":"DivisionImpossible","repr":"<class 'decimal.DivisionImpossible'>"}]),
        SourceFunctionArgument::Variable(v)=>json!({"kind":"Variable","value":v.as_str(),"hash":session.source_hash().unwrap().variable_hash(v.as_str()),"repr":format!("rdflib.term.Variable({})",semantica_json::python_repr(&json!(v.as_str())).unwrap())}),
    }).collect::<Vec<_>>();
    json!({"kind":"Exception","module":error.kind.module(),"class_name":error.kind.class_name(),"args":args})
}
fn failure_json(error: &SourceFunctionFailure) -> Value {
    match error {
        SourceFunctionFailure::Sparql(v) => sparql_json(v),
        SourceFunctionFailure::Exception(v) => exception_json(v),
        other => panic!("original callback comparison reached host/unreceived failure: {other:?}"),
    }
}
pub(super) fn without_tokens(value: &Value) -> Value {
    match value {
        Value::Object(v) => Value::Object(
            v.iter()
                .filter(|(k, _)| k.as_str() != "object_token")
                .map(|(k, v)| (k.clone(), without_tokens(v)))
                .collect(),
        ),
        Value::Array(v) => Value::Array(v.iter().map(without_tokens).collect()),
        v => v.clone(),
    }
}
fn compare_aliases(
    actual: &Value,
    want: &Value,
    forward: &mut BTreeMap<String, String>,
    reverse: &mut BTreeMap<String, String>,
) {
    match (actual, want) {
        (Value::Object(a), Value::Object(w)) => {
            if let (Some(a), Some(w)) = (
                a.get("object_token").and_then(Value::as_str),
                w.get("object_token").and_then(Value::as_str),
            ) {
                if let Some(old) = forward.insert(a.to_owned(), w.to_owned()) {
                    assert_eq!(old, w, "one actual source object was split in original");
                }
                if let Some(old) = reverse.insert(w.to_owned(), a.to_owned()) {
                    assert_eq!(old, a, "one original source object was split in actual");
                }
            }
            for (k, v) in a {
                if k != "object_token" {
                    compare_aliases(v, &w[k], forward, reverse);
                }
            }
        }
        (Value::Array(a), Value::Array(w)) => {
            assert_eq!(a.len(), w.len());
            for (a, w) in a.iter().zip(w) {
                compare_aliases(a, w, forward, reverse);
            }
        }
        _ => (),
    }
}
struct Scenario {
    input: Value,
    registry: Rc<SourceFunctionRegistry>,
    session: Rc<QueryNumericSession>,
    callbacks: RefCell<BTreeMap<String, SourceFunctionDefinition>>,
    used: RefCell<BTreeSet<usize>>,
    counters: RefCell<BTreeMap<String, usize>>,
    stage: RefCell<String>,
    actions: RefCell<Vec<Value>>,
    events: RefCell<Vec<Value>>,
    objects: RefCell<Vec<RawExpressionValue>>,
    rows: RefCell<Vec<Value>>,
}
impl Scenario {
    fn new(input: &Value) -> Rc<Self> {
        Self::new_with_time_values(input, false)
    }
    fn new_with_time_values(input: &Value, time_values: bool) -> Rc<Self> {
        Self::new_with_time_policy(input, time_values, false)
    }
    fn new_with_time_policy(input: &Value, time_values: bool, time_comparisons: bool) -> Rc<Self> {
        Self::new_with_capabilities(input, time_values, time_comparisons, false)
    }
    fn new_with_capabilities(
        input: &Value,
        time_values: bool,
        time_comparisons: bool,
        calendar: bool,
    ) -> Rc<Self> {
        let registry = Rc::new(SourceFunctionRegistry::stock());
        let mut decimal = semantica_numeric::DecimalContext::default();
        if let Some(context) = input.get("decimal_context") {
            assert_eq!(context.as_object().unwrap().len(), 5);
            assert_eq!(context["rounding"], "ROUND_HALF_EVEN");
            decimal.precision = usize::try_from(context["precision"].as_u64().unwrap()).unwrap();
            decimal.emin = context["emin"].as_i64().unwrap().into();
            decimal.emax = context["emax"].as_i64().unwrap().into();
            decimal.traps = semantica_numeric::DecimalSignals::EMPTY;
            for name in context["traps"].as_array().unwrap() {
                let signal = semantica_numeric::DecimalSignal::ALL
                    .into_iter()
                    .find(|s| format!("{s:?}") == name.as_str().unwrap())
                    .expect("known frozen input signal");
                decimal.traps.insert(signal);
            }
        }
        let session = Rc::try_unwrap(source_bgp_receiving::session_with_decimal(decimal))
            .unwrap()
            .with_source_functions(Rc::clone(&registry))
            .unwrap();
        let session = if calendar {
            session.with_source_calendar_duration_v1().unwrap()
        } else {
            session
        };
        let session = Rc::new(if time_comparisons {
            session.with_source_time_comparisons_v1().unwrap()
        } else if time_values {
            session.with_source_time_values_v1().unwrap()
        } else {
            session
        });
        let scenario = Rc::new(Self {
            input: input.clone(),
            registry,
            session,
            callbacks: RefCell::new(BTreeMap::new()),
            used: RefCell::new(BTreeSet::new()),
            counters: RefCell::new(BTreeMap::new()),
            stage: RefCell::new(String::new()),
            actions: RefCell::new(Vec::new()),
            events: RefCell::new(Vec::new()),
            objects: RefCell::new(Vec::new()),
            rows: RefCell::new(Vec::new()),
        });
        for spec in input["callbacks"].as_array().unwrap() {
            let name = spec["name"].as_str().unwrap();
            let callable = if spec["kind"] == "builtin" {
                SourceCallable::DefaultCast
            } else {
                let weak = Rc::downgrade(&scenario);
                let spec = spec.clone();
                SourceCallable::Native {
                    qualified_name: Rc::from(
                        "__main__.observe.<locals>.make_callback.<locals>.callback",
                    ),
                    callback: Rc::new(move |invocation| {
                        weak.upgrade()
                            .expect("active callback scenario")
                            .invoke(&spec, invocation)
                    }),
                }
            };
            scenario.callbacks.borrow_mut().insert(
                name.to_owned(),
                SourceFunctionDefinition {
                    callable,
                    raw: spec["raw"].as_bool().unwrap(),
                },
            );
        }
        scenario
    }
    fn encode(&self, value: &SolutionValue<RawExpressionValue>) -> Value {
        match value {
            SolutionValue::ExplicitNone => Value::Null,
            SolutionValue::ReturnedError(v) => sparql_json(v),
            SolutionValue::Rdf(raw) => {
                let mut value = source_bgp_receiving::term_json(raw, &self.session);
                let mut objects = self.objects.borrow_mut();
                let index =
                    if let Some(index) = objects.iter().position(|v| v.same_source_object(raw)) {
                        index
                    } else {
                        objects.push(raw.clone());
                        objects.len() - 1
                    };
                value["object_token"] = json!(format!("candidate_{index}"));
                value
            }
        }
    }
    fn values(&self, values: &[SolutionValue<RawExpressionValue>]) -> Value {
        json!(values.iter().map(|v| self.encode(v)).collect::<Vec<_>>())
    }
    fn callable_name(&self, callable: &SourceCallable) -> String {
        self.callbacks
            .borrow()
            .iter()
            .find_map(|(name, v)| v.callable.same_callable(callable).then(|| name.clone()))
            .unwrap_or_else(|| {
                assert!(matches!(callable, SourceCallable::DefaultCast));
                "default_cast".into()
            })
    }
    fn registry_json(&self) -> Value {
        let mut entries = self.registry.entries();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        json!(
            entries
                .iter()
                .map(|(iri, v)| json!([iri.as_str(), self.callable_name(&v.callable), v.raw]))
                .collect::<Vec<_>>()
        )
    }
    fn run_actions(&self, stage: &str) {
        for (index, action) in self.input["actions"].as_array().unwrap().iter().enumerate() {
            if action["stage"] != stage || !self.used.borrow_mut().insert(index) {
                continue;
            }
            let iri = NamedNode::new(action["uri"].as_str().unwrap()).unwrap();
            let mut row = json!({"input":action,"stage":self.stage.borrow().clone(),"warnings":[]});
            let callback = action["callback"]
                .as_str()
                .map(|name| self.callbacks.borrow()[name].clone());
            let result = match action["operation"].as_str().unwrap() {
                "register" => self.registry.register(
                    iri,
                    callback.unwrap(),
                    action["override"].as_bool().unwrap(),
                ),
                "decorate" => {
                    let definition = callback.unwrap();
                    let original = definition.callable.clone();
                    self.registry
                        .decorate(iri, definition, action["override"].as_bool().unwrap())
                        .map(|v| row["return_is_same_callback"] = json!(v.same_callable(&original)))
                }
                "unregister" => {
                    if let Some(warning) = self
                        .registry
                        .unregister(&iri, callback.as_ref().map(|v| &v.callable))
                    {
                        row["warnings"] =
                            json!([{"category":warning.category,"message":warning.message}]);
                    }
                    Ok(())
                }
                other => panic!("unreceived registry action {other}"),
            };
            match result {
                Ok(()) => row["returned"] = Value::Null,
                Err(error) => row["error"] = exception_json(&error),
            }
            row["registry"] = self.registry_json();
            self.actions.borrow_mut().push(row);
        }
    }
    fn lifecycle(&self, stage: &str) {
        *self.stage.borrow_mut() = if stage == "before_parse" {
            "registry before parse".into()
        } else {
            stage.into()
        };
        self.run_actions(stage);
        if let Some(n) = stage.strip_prefix("before_poll_") {
            *self.stage.borrow_mut() = if n == "0" {
                "first_poll".into()
            } else {
                format!("poll_after_{n}_rows")
            };
        }
    }
    fn literal(&self, lexical: &str, datatype: Option<&str>) -> SourceFunctionResult {
        self.session
            .construct_source_direct_string(
                lexical,
                datatype.map(|v| NamedNode::new(v).unwrap()),
                None,
                true,
            )
            .map(SolutionValue::Rdf)
            .map_err(SourceFunctionFailure::Unreceived)
    }
    fn invoke(
        &self,
        spec: &Value,
        invocation: SourceFunctionInvocation<'_>,
    ) -> SourceFunctionResult {
        let name = spec["name"].as_str().unwrap();
        let kind = spec["kind"].as_str().unwrap();
        let count = {
            let mut counters = self.counters.borrow_mut();
            let count = counters.entry(name.into()).or_default();
            *count += 1;
            *count
        };
        let mut event =
            json!({"name":name,"stage":self.stage.borrow().clone(),"call":count,"raw":spec["raw"]});
        match &invocation {
            SourceFunctionInvocation::Raw(context) => {
                event["iri"] = self.encode(&SolutionValue::Rdf(context.iri()));
                event["keys"] = json!(
                    context
                        .syntax()
                        .root_node()
                        .unwrap()
                        .fields
                        .iter()
                        .map(|(key, _)| key.as_str())
                        .collect::<Vec<_>>()
                );
            }
            SourceFunctionInvocation::Arguments { values, .. } => {
                event["arguments"] = self.values(values)
            }
        }
        self.events.borrow_mut().push(event);
        self.run_actions(&format!("callback_{name}"));
        let context = match &invocation {
            SourceFunctionInvocation::Raw(context) => Some(*context),
            _ => None,
        };
        let arguments = || match &invocation {
            SourceFunctionInvocation::Raw(context) => context.arguments(),
            SourceFunctionInvocation::Arguments { values, .. } => Ok(Some(values.to_vec())),
        };
        match kind {
            "raise_type" | "raise_value" => {
                Err(SourceFunctionFailure::Exception(SourceFunctionException {
                    kind: if kind == "raise_type" {
                        SourceFunctionExceptionKind::TypeError
                    } else {
                        SourceFunctionExceptionKind::ValueError
                    },
                    arguments: spec["error_args"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| {
                            if v.is_null() {
                                SourceFunctionArgument::None
                            } else {
                                SourceFunctionArgument::Text(v.as_str().unwrap().into())
                            }
                        })
                        .collect(),
                }))
            }
            "raise_sparql" | "return_sparql" => {
                let args = spec["error_args"].as_array().unwrap();
                assert!(args.len() <= 1);
                let argument = args
                    .first()
                    .and_then(Value::as_str)
                    .map_or(SourceExceptionArgument::None, |v| {
                        SourceExceptionArgument::Text(v.into())
                    });
                let diagnostic = match &argument {
                    SourceExceptionArgument::None => "None",
                    SourceExceptionArgument::Text(v) => v,
                    SourceExceptionArgument::DecimalSignalClasses(_) => {
                        panic!("frozen callback only constructs None/text")
                    }
                };
                let error = ErrorValue::with_source(
                    SparqlErrorKind::OtherSparql,
                    diagnostic,
                    SourceException {
                        class: SourceExceptionClass::Sparql,
                        argument: argument.clone(),
                    },
                );
                if kind == "raise_sparql" {
                    Err(SourceFunctionFailure::Sparql(error))
                } else {
                    Ok(SolutionValue::ReturnedError(error))
                }
            }
            "constant" => {
                self.literal(spec["lexical"].as_str().unwrap(), spec["datatype"].as_str())
            }
            "none" => Ok(SolutionValue::ExplicitNone),
            "counter" => {
                let dt = oxrdf::vocab::xsd::INTEGER.into_owned();
                self.session
                    .known_source_cached(
                        Literal::new_typed_literal(count.to_string(), dt.clone()).into(),
                        semantica_numeric::NumericValue::Integer(semantica_numeric::BigInt::from(
                            count,
                        )),
                        SourceLiteralMetadata {
                            datatype: SourceDatatype::Specified(dt),
                            language_original: SourceFact::Known(None),
                            ill_typed: SourceFact::Known(None),
                            cached_value_kind: SourceFact::Known(SourceLiteralCacheKind::Numeric),
                        },
                    )
                    .map(SolutionValue::Rdf)
                    .map_err(SourceFunctionFailure::Unreceived)
            }
            "raw_binding" => context
                .unwrap()
                .binding(&Variable::new(spec["variable"].as_str().unwrap()).unwrap()),
            "raw_iri" => Ok(SolutionValue::Rdf(context.unwrap().iri())),
            "raw_distinct" => {
                let node = context.unwrap().syntax().root_node().unwrap();
                let text = match node
                    .fields
                    .iter()
                    .find(|(key, _)| key == "distinct")
                    .map(|(_, v)| v)
                {
                    None => "None",
                    Some(spargebra::SourceSyntaxValue::Text(v)) => v,
                    Some(spargebra::SourceSyntaxValue::List(v)) if v.is_empty() => "[]",
                    other => panic!("unreceived distinct metadata {other:?}"),
                };
                self.literal(text, None)
            }
            "raw_catch_decimal" => match arguments() {
                Ok(values) => Ok(values
                    .and_then(|v| v.into_iter().next())
                    .unwrap_or(SolutionValue::ExplicitNone)),
                Err(
                    error @ SourceFunctionFailure::Exception(SourceFunctionException {
                        kind: SourceFunctionExceptionKind::Decimal(_),
                        ..
                    }),
                ) => {
                    let json = failure_json(&error);
                    self.events.borrow_mut().push(
                        json!({"name":name,"stage":self.stage.borrow().clone(),"caught":json}),
                    );
                    self.literal(json["class_name"].as_str().unwrap(), None)
                }
                Err(error) => Err(error),
            },
            "raw_catch" => match arguments() {
                Ok(values) => Ok(values
                    .and_then(|v| v.into_iter().next())
                    .unwrap_or(SolutionValue::ExplicitNone)),
                Err(error @ SourceFunctionFailure::Sparql(_))
                | Err(
                    error @ SourceFunctionFailure::Exception(SourceFunctionException {
                        kind:
                            SourceFunctionExceptionKind::TypeError
                            | SourceFunctionExceptionKind::ValueError
                            | SourceFunctionExceptionKind::Decimal(
                                semantica_numeric::DecimalSignal::FloatOperation,
                            ),
                        ..
                    }),
                ) => {
                    let json = failure_json(&error);
                    self.events.borrow_mut().push(
                        json!({"name":name,"stage":self.stage.borrow().clone(),"caught":json}),
                    );
                    self.literal(json["class_name"].as_str().unwrap(), None)
                }
                Err(error) => Err(error),
            },
            "raw_error_text" | "raw_error_repr" | "raw_error_literal" => {
                let args = arguments()?;
                self.events.borrow_mut().push(json!({"name":name,"stage":self.stage.borrow().clone(),"read":0,"arguments":args.as_ref().map(|v|self.values(v))}));
                let value = args
                    .and_then(|v| v.into_iter().next())
                    .unwrap_or(SolutionValue::ExplicitNone);
                match kind {
                    "raw_error_literal" => self
                        .session
                        .construct_source_extremum_result(&value)
                        .map(|v| v.map_or(SolutionValue::ExplicitNone, SolutionValue::Rdf))
                        .map_err(SourceFunctionFailure::Unreceived),
                    "raw_error_text" => self
                        .session
                        .source_text(&value)
                        .map_err(SourceFunctionFailure::Unreceived)
                        .and_then(|text| self.literal(&text, None)),
                    _ => self
                        .session
                        .source_repr(&value)
                        .map_err(SourceFunctionFailure::Unreceived)
                        .and_then(|text| self.literal(&text, None)),
                }
            }
            "raw_reads" => {
                let mut result = SolutionValue::ExplicitNone;
                for read in 0..spec["reads"].as_u64().unwrap() {
                    let values = arguments()?;
                    self.events.borrow_mut().push(json!({"name":name,"stage":self.stage.borrow().clone(),"read":read,"arguments":values.as_ref().map(|v|self.values(v))}));
                    result = values
                        .and_then(|v| v.into_iter().next())
                        .unwrap_or(SolutionValue::ExplicitNone);
                }
                Ok(result)
            }
            "echo" => Ok(arguments()?
                .and_then(|v| v.into_iter().next())
                .unwrap_or(SolutionValue::ExplicitNone)),
            other => panic!("unreceived callback input program {other}"),
        }
    }
}
thread_local! {static CURRENT:RefCell<Option<Rc<Scenario>>>=const{RefCell::new(None)};}
struct Guard;
impl Guard {
    fn start(scenario: Rc<Scenario>) -> Self {
        CURRENT.with_borrow_mut(|v| {
            assert!(v.is_none());
            *v = Some(scenario);
        });
        Self
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        CURRENT.with_borrow_mut(|v| *v = None);
    }
}
pub(super) fn lifecycle(stage: &str) {
    let scenario = CURRENT.with_borrow(Clone::clone);
    if let Some(scenario) = scenario {
        scenario.lifecycle(stage);
    }
}
pub(super) fn row(ordinal: usize, variable: &Variable, raw: &RawExpressionValue) {
    let scenario = CURRENT.with_borrow(Clone::clone);
    if let Some(scenario) = scenario {
        let value = scenario.encode(&SolutionValue::Rdf(raw.clone()));
        scenario
            .rows
            .borrow_mut()
            .push(json!({"row":ordinal,"variable":variable.as_str(),"value":value}));
    }
}
fn expected_trace(case: &Value) -> Value {
    let mut events = Vec::new();
    let mut stack = Vec::<(String, bool)>::new();
    for event in case["trace"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["name"] == "Function")
    {
        match event["event"].as_str().unwrap() {
            "call" => {
                let iri = event["iri"]["value"].as_str().unwrap().to_owned();
                events.push(json!({"event":"call","iri":iri}));
                stack.push((iri, false));
            }
            "exception" => {
                let (iri, failed) = stack.last_mut().unwrap();
                *failed = true;
                events.push(json!({"event":"error","iri":iri,"value":event["exception"]}));
            }
            "return" => {
                let (iri, failed) = stack.pop().unwrap();
                if failed {
                    assert!(event["result"].is_null());
                } else {
                    events.push(json!({"event":"return","iri":iri,"value":event["result"]}));
                }
            }
            "line" => (),
            other => panic!("unreceived trace kind {other}"),
        }
    }
    assert!(stack.is_empty());
    json!(events)
}
fn compare(
    scenario: &Scenario,
    case: &Value,
    events: Vec<crate::expression::source_functions::observation::Event>,
) {
    use crate::expression::source_functions::observation::Event;
    let trace = events
        .iter()
        .map(|event| match event {
            Event::Call(iri, syntax) => {
                assert_eq!(syntax.root_node().unwrap().name, "Function");
                json!({"event":"call","iri":iri.as_str()})
            }
            Event::Return(iri, value) => {
                json!({"event":"return","iri":iri.as_str(),"value":scenario.encode(value)})
            }
            Event::Raised(iri, error) => {
                json!({"event":"error","iri":iri.as_str(),"value":failure_json(error)})
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        scenario.used.borrow().len(),
        if case["error_stage"] == "initial_construction" {
            0
        } else {
            scenario.input["actions"].as_array().unwrap().len()
        },
        "registry input execution respects actual constructor failure boundary"
    );
    assert_eq!(
        json!(*scenario.actions.borrow()),
        case["registry_actions"],
        "{} registry state, errors, warnings and callable identities",
        scenario.input["id"]
    );
    let mut rows = Vec::new();
    for row in scenario.rows.borrow().iter() {
        let wanted = case["rows"][row["row"].as_u64().unwrap() as usize]
            .as_array()
            .unwrap()
            .iter()
            .find(|pair| pair[0]["value"] == row["variable"])
            .unwrap();
        rows.push(json!({"row":row["row"],"variable":row["variable"],"value":wanted[1]}));
    }
    let actual =
        json!({"callbacks":*scenario.events.borrow(),"trace":trace,"rows":*scenario.rows.borrow()});
    let expected =
        json!({"callbacks":case["callback_events"],"trace":expected_trace(case),"rows":rows});
    assert_eq!(
        without_tokens(&actual),
        without_tokens(&expected),
        "{} actual callback execution",
        scenario.input["id"]
    );
    compare_aliases(
        &actual,
        &expected,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
    );
}
fn receive(id: &str) {
    let fixture = fixture();
    let case = fixture["graph_queries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["input"]["id"] == id)
        .unwrap();
    receive_case(case, &fixture["data_inputs"]);
}
pub(super) fn receive_case(case: &Value, data_inputs: &Value) {
    receive_case_with_time_values(case, data_inputs, false);
}
pub(super) fn receive_case_with_time_values(case: &Value, data_inputs: &Value, time_values: bool) {
    receive_case_with_time_policy(case, data_inputs, time_values, false);
}
pub(super) fn receive_case_with_time_comparisons(case: &Value, data_inputs: &Value) {
    receive_case_with_time_policy(case, data_inputs, true, true);
}
pub(super) fn receive_case_with_calendar_duration(case: &Value, data_inputs: &Value) {
    receive_case_with_capabilities(case, data_inputs, false, false, true);
}
pub(super) fn receive_case_with_temporal_arithmetic(case: &Value, data_inputs: &Value) {
    receive_case_with_capabilities(case, data_inputs, true, false, true);
}
fn receive_case_with_time_policy(
    case: &Value,
    data_inputs: &Value,
    values: bool,
    comparisons: bool,
) {
    receive_case_with_capabilities(case, data_inputs, values, comparisons, false);
}
fn receive_case_with_capabilities(
    case: &Value,
    data_inputs: &Value,
    values: bool,
    comparisons: bool,
    calendar: bool,
) {
    let scenario = Scenario::new_with_capabilities(&case["input"], values, comparisons, calendar);
    let _guard = Guard::start(Rc::clone(&scenario));
    let trace = crate::expression::source_functions::observation::Guard::start();
    source_bgp_receiving::receive_query_case_with_registry(
        case,
        data_inputs,
        Rc::clone(&scenario.session),
    );
    compare(&scenario, case, trace.take());
}

#[test]
fn source_function_registry_fr01() {
    receive("FR01");
}

#[test]
fn source_function_registry_fr02() {
    receive("FR02");
}

#[test]
fn source_function_registry_fr03() {
    receive("FR03");
}

#[test]
fn source_function_registry_fr04() {
    receive("FR04");
}

#[test]
fn source_function_registry_fr05() {
    receive("FR05");
}

#[test]
fn source_function_registry_fr06() {
    receive("FR06");
}

#[test]
fn source_function_registry_fr07() {
    receive("FR07");
}

#[test]
fn source_function_registry_fr08() {
    receive("FR08");
}

#[test]
fn source_function_registry_fr09() {
    receive("FR09");
}

#[test]
fn source_function_registry_fr10() {
    receive("FR10");
}

#[test]
fn source_function_registry_fr11() {
    receive("FR11");
}

#[test]
fn source_function_registry_fr12() {
    receive("FR12");
}

#[test]
fn source_function_registry_fr13() {
    receive("FR13");
}

#[test]
fn source_function_registry_fr14() {
    receive("FR14");
}

#[test]
fn source_function_registry_fr15() {
    receive("FR15");
}

#[test]
fn source_function_registry_fr16() {
    receive("FR16");
}

#[test]
fn source_function_registry_fr17() {
    receive("FR17");
}

#[test]
fn source_function_registry_fr18() {
    receive("FR18");
}

#[test]
fn source_function_registry_fr19() {
    receive("FR19");
}

#[test]
fn source_function_registry_fr20() {
    receive("FR20");
}

#[test]
fn source_function_registry_fr21() {
    receive("FR21");
}

#[test]
fn source_function_registry_fr22() {
    receive("FR22");
}

#[test]
fn source_function_registry_fr23() {
    receive("FR23");
}

#[test]
fn source_function_registry_fr24() {
    receive("FR24");
}

#[test]
fn source_function_registry_fr25() {
    receive("FR25");
}

#[test]
fn source_function_registry_fr26() {
    receive("FR26");
}

#[test]
fn source_function_registry_fr27() {
    receive("FR27");
}

#[test]
fn source_function_registry_fr28() {
    receive("FR28");
}

#[test]
fn source_function_registry_fr29() {
    receive("FR29");
}

#[test]
fn source_function_registry_fr30() {
    receive("FR30");
}

#[test]
fn source_function_registry_fr31() {
    receive("FR31");
}

#[test]
fn source_function_registry_fr32() {
    receive("FR32");
}

#[test]
fn source_function_registry_fr33() {
    receive("FR33");
}

#[test]
fn source_function_registry_fr34() {
    receive("FR34");
}

#[test]
fn source_function_registry_fr35() {
    receive("FR35");
}

#[test]
fn source_function_registry_fr36() {
    receive("FR36");
}

#[test]
fn source_function_registry_fr37() {
    receive("FR37");
}

#[test]
fn source_function_registry_fr38() {
    receive("FR38");
}

#[test]
fn source_function_registry_fr39() {
    receive("FR39");
}

#[test]
fn source_function_registry_fr40() {
    receive("FR40");
}

#[test]
fn source_function_registry_fr41() {
    receive("FR41");
}

#[test]
fn source_function_registry_fr42() {
    receive("FR42");
}

#[test]
fn source_function_registry_fr43() {
    receive("FR43");
}

#[test]
fn source_function_registry_fr44() {
    receive("FR44");
}

#[test]
fn source_function_registry_fr45() {
    receive("FR45");
}

#[test]
fn source_function_registry_fr46() {
    receive("FR46");
}

#[test]
fn source_function_registry_fr47() {
    receive("FR47");
}

#[test]
fn source_function_registry_fr48() {
    receive("FR48");
}

#[test]
fn source_function_registry_fr49() {
    receive("FR49");
}

#[test]
fn source_function_registry_fr50() {
    receive("FR50");
}

#[test]
fn source_function_registry_fr51() {
    receive("FR51");
}

#[test]
fn source_function_registry_fr52() {
    receive("FR52");
}

#[test]
fn source_function_registry_fr53() {
    receive("FR53");
}

#[test]
fn source_function_registry_fr54() {
    receive("FR54");
}

#[test]
fn source_function_registry_fe01() {
    receive("FE01");
}

#[test]
fn source_function_registry_fe02() {
    receive("FE02");
}

#[test]
fn source_function_registry_fe03() {
    receive("FE03");
}

#[test]
fn source_function_registry_fe04() {
    receive("FE04");
}

#[test]
fn source_function_registry_fe05() {
    receive("FE05");
}

#[test]
fn source_function_registry_fe06() {
    receive("FE06");
}

#[test]
fn source_function_registry_fe07() {
    receive("FE07");
}

#[test]
fn source_function_registry_fe08() {
    receive("FE08");
}

#[test]
fn source_function_registry_fe09() {
    receive("FE09");
}

#[test]
fn source_function_registry_fe10() {
    receive("FE10");
}

#[test]
fn source_function_registry_fe11() {
    receive("FE11");
}

#[test]
fn source_function_registry_fe12() {
    receive("FE12");
}

#[test]
fn source_function_registry_fe13() {
    receive("FE13");
}

#[test]
fn source_function_registry_fe14() {
    receive("FE14");
}

#[test]
fn source_function_registry_fe15() {
    receive("FE15");
}

#[test]
fn source_function_registry_fe16() {
    receive("FE16");
}

#[test]
fn source_function_registry_fe17() {
    receive("FE17");
}

#[test]
fn source_function_registry_fe18() {
    receive("FE18");
}

#[test]
fn source_function_registry_fe19() {
    receive("FE19");
}

#[test]
fn source_function_registry_fe20() {
    receive("FE20");
}

#[test]
fn source_function_registry_fe21() {
    receive("FE21");
}

#[test]
fn source_function_registry_stock_keeps_87_original_queries() {
    let data = crate::private_test_fixtures::text_ref!(
        "release-inputs-03/vendor/spareval/src/eval/fixtures/source_function_runtime_query_cases.json"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(data.as_bytes())),
        "476c6ddb222c75b8f86820d2da81b36b6497bfad7ed3277e9df856006d31e59a"
    );
    let root: Value = serde_json::from_str(data).unwrap();
    assert_eq!(root["candidate_results_used"], false);
    assert_eq!(root["graph_queries"].as_array().unwrap().len(), 87);
    for original in root["graph_queries"].as_array().unwrap() {
        let mut case = original.clone();
        case["input"]["callbacks"] = json!([]);
        case["input"]["actions"] = json!([]);
        case["callback_events"] = json!([]);
        case["registry_actions"] = json!([]);
        let scenario = Scenario::new(&case["input"]);
        let _guard = Guard::start(Rc::clone(&scenario));
        let trace = crate::expression::source_functions::observation::Guard::start();
        source_bgp_receiving::receive_query_case_with_registry(
            &case,
            &root["data_inputs"],
            Rc::clone(&scenario.session),
        );
        compare(&scenario, &case, trace.take());
    }
}
fn controlled_session(registry: Rc<SourceFunctionRegistry>) -> Rc<QueryNumericSession> {
    Rc::new(
        Rc::try_unwrap(source_bgp_receiving::session())
            .unwrap()
            .with_source_functions(registry)
            .unwrap(),
    )
}
fn constant_callable(text: &str) -> SourceCallable {
    let text = text.to_owned();
    SourceCallable::Native {
        qualified_name: Rc::from("receiving.constant"),
        callback: Rc::new(move |invocation| {
            let session = match invocation {
                SourceFunctionInvocation::Raw(ctx) => ctx.session(),
                SourceFunctionInvocation::Arguments { session, .. } => session,
            };
            session
                .construct_source_direct_string(&text, None, None, true)
                .map(SolutionValue::Rdf)
                .map_err(SourceFunctionFailure::Unreceived)
        }),
    }
}
#[test]
fn source_function_registry_explicit_sharing_and_fresh_scope() {
    let registry = Rc::new(SourceFunctionRegistry::stock());
    let sessions = [
        controlled_session(Rc::clone(&registry)),
        controlled_session(Rc::clone(&registry)),
        controlled_session(Rc::new(SourceFunctionRegistry::stock())),
    ];
    let prepared = sessions
        .into_iter()
        .map(|session| {
            numeric_query::prepare_source_graph_numeric_query(
                "SELECT (<urn:f>(1) AS ?v) WHERE {}",
                source_bgp_receiving::dataset(&session, None),
                session,
                Vec::new(),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    registry
        .register(
            NamedNode::new("urn:f").unwrap(),
            SourceFunctionDefinition {
                callable: constant_callable("shared"),
                raw: true,
            },
            false,
        )
        .unwrap();
    for (index, query) in prepared.into_iter().enumerate() {
        let mut rows = query.execute_source_presence().unwrap();
        let row = rows.next().unwrap().unwrap();
        if index < 2 {
            let Some(SolutionValue::Rdf(v)) = &row.values[0] else {
                panic!("shared registration not observed")
            };
            assert_eq!(
                v.rdf().unwrap(),
                &Literal::new_simple_literal("shared").into()
            );
        } else {
            assert!(
                row.values[0].is_none(),
                "fresh scope inherited another scope's registration"
            );
        }
        assert!(rows.next().is_none());
    }
}
#[test]
fn source_function_registry_cannot_enable_source_scope_or_change_legacy() {
    let legacy = QueryNumericSession::new(
        semantica_numeric::DecimalContext::default(),
        semantica_numeric::NumericLimits::default(),
        None,
        true,
    )
    .unwrap();
    assert!(matches!(
        legacy.with_source_functions(Rc::new(SourceFunctionRegistry::stock())),
        Err(RawNumericFault::InvalidSourceOrigin)
    ));
    let session = source_bgp_receiving::session();
    assert!(session.source_functions().is_none());
    let query = numeric_query::prepare_source_graph_numeric_query(
        "SELECT (<urn:f>(1) AS ?v) WHERE {}",
        source_bgp_receiving::dataset(&session, None),
        session,
        Vec::new(),
    )
    .unwrap();
    let mut rows = query.execute_source_presence().unwrap();
    assert!(rows.next().unwrap().unwrap().values[0].is_none());
    assert!(rows.next().is_none());
}
#[test]
fn source_function_registry_cannot_swallow_unreceived_argument() {
    let registry = Rc::new(SourceFunctionRegistry::stock());
    let session = controlled_session(Rc::clone(&registry));
    registry
        .register(
            NamedNode::new("urn:f").unwrap(),
            SourceFunctionDefinition {
                raw: true,
                callable: SourceCallable::Native {
                    qualified_name: Rc::from("receiving.swallow"),
                    callback: Rc::new(|invocation| {
                        let SourceFunctionInvocation::Raw(ctx) = invocation else {
                            panic!("raw callback")
                        };
                        assert!(ctx.arguments().is_err());
                        Ok(SolutionValue::ExplicitNone)
                    }),
                },
            },
            false,
        )
        .unwrap();
    let query = numeric_query::prepare_source_graph_numeric_query(
        "SELECT (<urn:f>(CONCAT(\"a\",\"b\")) AS ?v) WHERE {}",
        source_bgp_receiving::dataset(&session, None),
        session,
        Vec::new(),
    )
    .unwrap();
    let mut rows = query.execute_source_presence().unwrap();
    let error = rows
        .next()
        .unwrap()
        .err()
        .expect("callback hid an unreceived operation");
    assert!(matches!(
        crate::numeric::raw_numeric_fault(&error),
        Some(RawNumericFault::UnsupportedSourceLiteralOperator(_))
    ));
    for _ in 0..3 {
        assert!(rows.next().is_none());
    }
}
#[test]
fn source_function_registry_cannot_swallow_cancelled_context_read() {
    let registry = Rc::new(SourceFunctionRegistry::stock());
    let session = controlled_session(Rc::clone(&registry));
    let flag = Arc::new(AtomicBool::new(false));
    let cancel = Arc::clone(&flag);
    registry
        .register(
            NamedNode::new("urn:f").unwrap(),
            SourceFunctionDefinition {
                raw: true,
                callable: SourceCallable::Native {
                    qualified_name: Rc::from("receiving.cancel"),
                    callback: Rc::new(move |invocation| {
                        let SourceFunctionInvocation::Raw(ctx) = invocation else {
                            panic!("raw callback")
                        };
                        cancel.store(true, atomic::Ordering::Relaxed);
                        assert!(matches!(
                            ctx.binding(&Variable::new("x").unwrap()),
                            Err(SourceFunctionFailure::Host)
                        ));
                        Ok(SolutionValue::ExplicitNone)
                    }),
                },
            },
            false,
        )
        .unwrap();
    let query = numeric_query::prepare_source_graph_numeric_query(
        "SELECT (<urn:f>(1) AS ?v) WHERE { VALUES ?x { 1 } }",
        source_bgp_receiving::dataset(&session, Some(flag)),
        session,
        Vec::new(),
    )
    .unwrap();
    let mut rows = query.execute_source_presence().unwrap();
    assert!(matches!(
        rows.next().unwrap(),
        Err(QueryEvaluationError::Cancelled)
    ));
    for _ in 0..3 {
        assert!(rows.next().is_none());
    }
}
