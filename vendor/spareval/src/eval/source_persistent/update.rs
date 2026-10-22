//! Phase-aware source UPDATE boundary. A supplied loader is the only resource
//! authority; no parsed LOAD/USING string can open a file or network endpoint.
use super::*;
use crate::eval::source_update::{self,PreparedUpdateBindings};
use crate::numeric::QueryNumericSession;
use std::cell::RefCell;

pub trait SourceUpdateLoader {
    /// True means the authorized parser reached success; false is an explicitly
    /// observed unavailable resource. Err is a guard/refusal, never SILENT-able.
    fn load(&mut self,iri:&str,emit:&mut dyn FnMut([SourceTermInput<'_>;3])->Result<(),QueryEvaluationError>)->Result<bool,QueryEvaluationError>;
}
#[derive(Debug)]
pub struct SourceUpdateError {
    pub before_backend:bool,
    pub error:QueryEvaluationError,
}
impl std::fmt::Display for SourceUpdateError {fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {self.error.fmt(f)}}
impl std::error::Error for SourceUpdateError {fn source(&self)->Option<&(dyn std::error::Error+'static)> {Some(&self.error)}}
fn phase(error:QueryEvaluationError,before_backend:bool)->SourceUpdateError {SourceUpdateError{before_backend,error}}

impl SourceUpdateError {
    /// Source exception identity only when the existing typed owner proves it.
    /// Resource/projection/authority errors and unsupported rendering return None.
    pub fn original_exception(&self) -> Option<(&'static str, &'static str, String)> {
        fn query(error: &QueryEvaluationError) -> Option<(&'static str, &'static str, String)> {
            let QueryEvaluationError::Unexpected(error) = error else { return None; };
            if let Some(error) = error.downcast_ref::<crate::eval::source_compvalue::SourceCompValueError>() {
                return Some((error.module, error.class, error.argument.clone()));
            }
            let prepared = error.downcast_ref::<crate::eval::numeric_query::NumericPrepareError>()?;
            use crate::eval::numeric_query::NumericPrepareError as P;
            match prepared {
                P::LiteralConstruction(RawNumericFault::SourceParserLiteralType { argument }) =>
                    Some(("builtins", "TypeError", argument.clone())),
                P::SourceSyntax(spargebra::SourceParserError::ValueError { message }) =>
                    Some(("builtins", "ValueError", message.clone())),
                P::SourceSyntax(spargebra::SourceParserError::ParseException { input, location, message }) =>
                    parser_message_ascii(input, *location, message)
                        .map(|message| ("pyparsing.exceptions", "ParseException", message)),
                P::Query(error) => query(error),
                _ => None,
            }
        }
        query(&self.error)
    }
}

impl QueryEvaluationError {
    /// Reuse the received typed source parser and ASCII formatter for QUERY.
    /// This seam exposes only ParseException, never maps guard text to source.
    #[doc(hidden)]
    pub fn source_parser_exception_ascii(self) -> Result<String, Self> {
        let reached = SourceUpdateError { before_backend: false, error: self };
        match reached.original_exception() {
            Some(("pyparsing.exceptions", "ParseException", message)) => Ok(message),
            _ => Err(reached.error),
        }
    }
}

/// Pyparsing's source exception formatting for the admitted ASCII input domain.
/// Unicode exception word classes remain unreceived; no native Display change.
/// Uses the actual prepared input and parser checkpoint, never action IDs.
fn parser_message_ascii(input: &str, location: usize, message: &str) -> Option<String> {
    if !input.is_ascii() || location > input.len() { return None; }
    let before = &input[..location];
    let line = 1 + before.bytes().filter(|value| *value == b'\n').count();
    let column = before.rfind('\n').map_or(location + 1, |last| location - last);
    let found = if input.is_empty() {
        String::new()
    } else if location == input.len() {
        ", found end of text".into()
    } else {
        let rest = &input[location..];
        let count = rest.bytes().take(16).take_while(u8::is_ascii_alphanumeric).count().max(1);
        let word = &rest[..count];
        let quoted = semantica_json::python_repr(&serde_json::Value::String(word.into())).ok()?;
        format!(", found {}", quoted.replace("\\\\", "\\"))
    };
    Some(format!("{message}{found}  (at char {location}), (line:{line}, col:{column})"))
}

fn initial_failure(fault: RawNumericFault, input: SourceTermInput<'_>) -> SourceUpdateError {
    if let (RawNumericFault::SourceLiteralLanguageValueError,
        SourceTermInput::Literal { language: Some(language), .. }) = (&fault, input)
    {
        // RDFLib uses str(lang) inside literal single quotes, not repr(lang).
        // Preserve embedded quotes, backslashes and newlines verbatim.
        return phase(QueryEvaluationError::Unexpected(Box::new(
            crate::eval::source_compvalue::SourceCompValueError {
                module: "builtins", class: "ValueError",
                argument: format!("'{language}' is not a valid language tag!"),
            },
        )), true);
    }
    phase(QueryEvaluationError::Unexpected(Box::new(fault)), true)
}

/// The same original wrapper constructs QUERY and UPDATE bindings before its
/// backend try. Retain the actual constructed values; do not validate twice.
pub(super) fn construct_initial_bindings(
    session: &QueryNumericSession,
    bindings: &BTreeMap<String, SourceTermInput<'_>>,
) -> Result<PreparedUpdateBindings, SourceUpdateError> {
    let mut initial = Vec::new();
    let mut values = Vec::new();
    for (name, input) in bindings {
        let variable = Variable::new(name).map_err(|e| phase(error(e), true))?;
        let value = input.initial(true).map_err(|e| phase(e, true))?;
        let raw = session.construct_source_graph_initial(value.clone())
            .map_err(|e| initial_failure(e, *input))?;
        initial.push(NumericInitialBinding { variable, value });
        values.push(Some(raw));
    }
    Ok(PreparedUpdateBindings { initial, values })
}
impl SourceDatasetState {
    /// The input binding constructors run before the original backend try.
    /// Once execution starts, publish reached state even after a later failure.
    pub fn update_full(&mut self,text:&str,namespaces:&BTreeMap<String,String>,bindings:&BTreeMap<String,SourceTermInput<'_>>,loader:&mut dyn SourceUpdateLoader,max_quads:usize,max_bytes:usize)->Result<(),SourceUpdateError> {
        self.update_full_with_constructor(text,namespaces,bindings,loader,max_quads,max_bytes,None)
    }

    /// Explicit source UPDATE construction input for the private admitted lane.
    /// Constructor errors are reached-backend failures, with original partial
    /// state publication. The ordinary entry supplies None and keeps defaults.
    #[doc(hidden)]
    pub fn update_full_with_constructor(&mut self,text:&str,namespaces:&BTreeMap<String,String>,bindings:&BTreeMap<String,SourceTermInput<'_>>,loader:&mut dyn SourceUpdateLoader,max_quads:usize,max_bytes:usize,constructor:Option<spargebra::SourceBlankNodeConstructor>)->Result<(),SourceUpdateError> {
        let graph=self.graph(None).map_err(|e|phase(e,true))?;
        let session=graph.session().clone();
        let bindings=construct_initial_bindings(&session,bindings)?;
        let prepared=source_update::prepare_with_update_constructor(text,namespaces,session,constructor).map_err(|e|phase(QueryEvaluationError::Unexpected(Box::new(e)),false))?;
        let owner=Rc::new(RefCell::new(graph));
        let result=prepared.execute_all(owner.clone(),&bindings,loader);
        let graph=Rc::try_unwrap(owner).map_err(|_|phase(error("source_update_live_reader_retained"),false))?.into_inner();
        self.commit(graph,max_quads,max_bytes).map_err(|e|phase(e,false))?;
        result.map_err(|e|phase(e,false))
    }
}

#[cfg(test)]
mod typed_compatibility_receiving {
    use super::*;
    use crate::eval::numeric_query::NumericPrepareError;

    fn prepared(error: NumericPrepareError) -> SourceUpdateError {
        phase(QueryEvaluationError::Unexpected(Box::new(error)), false)
    }

    #[test]
    fn source_update_typed_ascii_exception_format_exact() {
        // Independent expectations follow the pinned Pyparsing exceptions.py
        // and util.py rules. They are not rendered by the candidate helper.
        let cases = [
            ("", 0, "Expected end of text  (at char 0), (line:1, col:1)"),
            (
                "a",
                1,
                "Expected end of text, found end of text  (at char 1), (line:1, col:2)",
            ),
            (
                "INVALID_more",
                0,
                "Expected end of text, found 'INVALID'  (at char 0), (line:1, col:1)",
            ),
            (
                "ABCDEFGHIJKLMNOPQRSTUVW",
                0,
                "Expected end of text, found 'ABCDEFGHIJKLMNOP'  (at char 0), (line:1, col:1)",
            ),
            (
                "_invalid",
                0,
                "Expected end of text, found '_'  (at char 0), (line:1, col:1)",
            ),
            (
                "!invalid",
                0,
                "Expected end of text, found '!'  (at char 0), (line:1, col:1)",
            ),
            (
                "\"",
                0,
                "Expected end of text, found '\"'  (at char 0), (line:1, col:1)",
            ),
            (
                "'",
                0,
                "Expected end of text, found \"'\"  (at char 0), (line:1, col:1)",
            ),
            (
                "\\",
                0,
                "Expected end of text, found '\\'  (at char 0), (line:1, col:1)",
            ),
            (
                "\n",
                0,
                "Expected end of text, found '\\n'  (at char 0), (line:1, col:1)",
            ),
            (
                "x\nBAD",
                2,
                "Expected end of text, found 'BAD'  (at char 2), (line:2, col:1)",
            ),
            (
                "x\n",
                2,
                "Expected end of text, found end of text  (at char 2), (line:2, col:1)",
            ),
            (
                "x\rBAD",
                2,
                "Expected end of text, found 'BAD'  (at char 2), (line:1, col:3)",
            ),
        ];
        for (input, location, expected) in cases {
            assert_eq!(
                parser_message_ascii(input, location, "Expected end of text").as_deref(),
                Some(expected)
            );
            let actual = prepared(NumericPrepareError::SourceSyntax(
                spargebra::SourceParserError::ParseException {
                    input: input.into(),
                    location,
                    message: "Expected end of text",
                },
            ))
            .original_exception();
            assert_eq!(
                actual,
                Some(("pyparsing.exceptions", "ParseException", expected.into()))
            );
            println!(
                "OE_SOURCE_UPDATE_TYPED_FORMAT {}",
                serde_json::json!({"input":input,"location":location,"expected":expected,"matched":true})
            );
        }
    }

    #[test]
    fn source_update_typed_nonascii_or_bad_location_remains_unreceived() {
        for (input, location) in [("éINVALID", 0), ("xé", 0), ("abc", 4)] {
            assert!(parser_message_ascii(input, location, "Expected end of text").is_none());
            let error = prepared(NumericPrepareError::SourceSyntax(
                spargebra::SourceParserError::ParseException {
                    input: input.into(),
                    location,
                    message: "Expected end of text",
                },
            ));
            assert!(error.original_exception().is_none());
        }
    }

    #[test]
    fn source_update_typed_actual_exception_carriers_are_not_text_classification() {
        let parser = prepared(NumericPrepareError::LiteralConstruction(
            RawNumericFault::SourceParserLiteralType {
                argument: "constructor's actual payload".into(),
            },
        ));
        assert_eq!(
            parser.original_exception(),
            Some((
                "builtins",
                "TypeError",
                "constructor's actual payload".into()
            ))
        );
        let unicode = prepared(NumericPrepareError::SourceSyntax(
            spargebra::SourceParserError::ValueError {
                message: "Invalid unicode code point: 00110000".into(),
            },
        ));
        assert_eq!(
            unicode.original_exception(),
            Some((
                "builtins",
                "ValueError",
                "Invalid unicode code point: 00110000".into()
            ))
        );
        let namespace = prepared(NumericPrepareError::Query(
            QueryEvaluationError::Unexpected(Box::new(
                crate::eval::source_compvalue::SourceCompValueError {
                    module: "builtins",
                    class: "Exception",
                    argument: "Unknown namespace prefix : missing".into(),
                },
            )),
        ));
        assert_eq!(
            namespace.original_exception(),
            Some((
                "builtins",
                "Exception",
                "Unknown namespace prefix : missing".into()
            ))
        );
    }

    #[test]
    fn source_update_typed_guards_and_lookalike_messages_remain_rust_errors() {
        let errors = [
            phase(
                QueryEvaluationError::Unexpected("Create not implemented!".into()),
                false,
            ),
            phase(
                QueryEvaluationError::Unexpected(Box::new(RawNumericFault::InvalidSourceOrigin)),
                false,
            ),
            phase(
                QueryEvaluationError::Unexpected(Box::new(
                    RawNumericFault::SourceLiteralLanguageValueError,
                )),
                true,
            ),
            prepared(NumericPrepareError::SourceSyntax(
                spargebra::SourceParserError::Unreceived("isolated Unicode surrogate"),
            )),
            prepared(NumericPrepareError::InitialLiteralConstruction(
                RawNumericFault::SourceParserLiteralType {
                    argument: "lookalike prepared constructor".into(),
                },
            )),
        ];
        for error in errors {
            assert!(
                error.original_exception().is_none(),
                "guard was relabeled: {error}"
            );
        }
    }
}
