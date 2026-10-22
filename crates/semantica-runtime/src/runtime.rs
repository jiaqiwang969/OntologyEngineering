//! A generated neutral port connects ontology-guided design to actual Rust execution.
use oxigraph::io::{RdfFormat, RdfParser};
use semantica_contracts::{
    SemanticNamedGraphValidationPort, SemanticNamedGraphValidationRequest,
    SemanticNamedGraphValidationResult, SemanticQueryOptionsPortV2, SemanticQueryOptionsPortV3,
    SemanticQueryOptionsPortV4, SemanticQueryOptionsRequestV2, SemanticQueryOptionsRequestV3,
    SemanticQueryOptionsRequestV4, SemanticQueryOptionsResultV2, SemanticQueryOptionsResultV3,
    SemanticQueryOptionsResultV4, SemanticQueryPort, SemanticQueryRequest, SemanticQueryResult,
    SemanticQueryShadowBudgetV3, SemanticQueryShadowGuardV3, SemanticReasonPort,
    SemanticReasonRequest, SemanticReasonResult, SemanticRuntimeFailure,
};
use std::collections::BTreeMap;

/// Explicit named-graph selectors retain the existing generic UTF-8 byte bound.
pub const MAX_NAMED_GRAPH_NAME_BYTES: usize = crate::MAX_BYTES;
/// Each original data/shape/selector byte can require up to six JSON wire bytes.
/// The envelope allowance covers fixed field names, schemas and SHA256 strings.
/// Decoded source and evaluation limits remain the existing 16 MiB / 100k profile.
pub const MAX_NAMED_GRAPH_VALIDATION_WIRE_BYTES: usize =
    6 * (2 * crate::MAX_BYTES + MAX_NAMED_GRAPH_NAME_BYTES) + 4096;

#[derive(Debug, Default, Clone, Copy)]
pub struct RustSemanticRuntime;

fn failure(code: &str, phase: &str, message: impl ToString) -> SemanticRuntimeFailure {
    SemanticRuntimeFailure {
        code: code.into(),
        phase: phase.into(),
        message: message.to_string(),
        retryable: false,
    }
}

/// Borrow the complete original request fields without materializing another DTO.
struct QueryOptionsInput<'a> {
    data: &'a str,
    data_sha256: &'a str,
    rdf_format: &'a str,
    query: &'a str,
    namespaces: &'a BTreeMap<String, String>,
    bindings: &'a BTreeMap<String, String>,
    budget: &'a SemanticQueryShadowBudgetV3,
    guard: &'a SemanticQueryShadowGuardV3,
}

impl RustSemanticRuntime {
    /// Validate an explicit named graph from complete source-bound inputs.
    /// Loading creates this call's ordinary DatasetSession; the subsequent
    /// validation borrows it immutably and retains the post-load revision.
    pub fn execute_named_graph_validation(
        &self,
        input: &SemanticNamedGraphValidationRequest,
    ) -> Result<SemanticNamedGraphValidationResult, SemanticRuntimeFailure> {
        input
            .validate()
            .map_err(|errors| failure("invalid_contract", "contract", errors.join("; ")))?;
        if input.data.len() > crate::MAX_BYTES
            || input.shapes.len() > crate::MAX_BYTES
            || input.graph_name.len() > MAX_NAMED_GRAPH_NAME_BYTES
        {
            return Err(failure(
                "input_budget_exceeded",
                "contract",
                "Decoded data, shapes or graph name exceed their explicit byte limits",
            ));
        }
        let request = serde_json::to_value(input)
            .map_err(|error| failure("invalid_contract", "contract", error))?;
        let ordered = semantica_json::ordered_json(&request)
            .map_err(|error| failure("invalid_contract", "contract", error))?;
        if ordered.len() > MAX_NAMED_GRAPH_VALIDATION_WIRE_BYTES {
            return Err(failure(
                "input_budget_exceeded",
                "contract",
                "Complete named-graph validation request exceeds its transport byte limit",
            ));
        }
        if crate::sha256(input.data.as_bytes()) != input.data_sha256
            || crate::sha256(input.shapes.as_bytes()) != input.shapes_sha256
        {
            return Err(failure(
                "source_identity_mismatch",
                "source",
                "Original dataset or shape bytes differ from the declared identity",
            ));
        }
        oxigraph::model::NamedNode::new(input.graph_name.as_str())
            .map_err(|error| failure("invalid_graph_name", "contract", error))?;
        let format = match input.dataset_format.as_str() {
            "trig" => RdfFormat::TriG,
            "nquads" => RdfFormat::NQuads,
            _ => {
                return Err(failure(
                    "unsupported_rdf_format",
                    "contract",
                    "Named-graph validation requires explicit trig or nquads input",
                ));
            }
        };
        let canonical = semantica_json::canonical_json(&request)
            .map_err(|error| failure("invalid_contract", "contract", error))?;
        let mut session = crate::dataset::DatasetSession::new();
        session
            .load(input.data.as_bytes(), format, None, None)
            .map_err(|error| failure("dataset_load_failed", "parse", error))?;
        let revision_before = session.revision();
        let (selected_quad_count, report) = session
            .validate_named_graph(&input.graph_name, input.shapes.as_bytes())
            .map_err(|error| failure("named_graph_validation_failed", "validate", error))?;
        let response = SemanticNamedGraphValidationResult {
            schema: "semantica.named-graph-validation-result/1".into(),
            request_sha256: crate::sha256(canonical),
            data_sha256: input.data_sha256.clone(),
            shapes_sha256: input.shapes_sha256.clone(),
            graph_name: input.graph_name.clone(),
            selected_quad_count,
            revision_before,
            revision_after: session.revision(),
            report,
        };
        response
            .validate()
            .map_err(|errors| failure("result_contract_invalid", "result", errors.join("; ")))?;
        Ok(response)
    }

    /// Stateless source-rule reasoning shares the BookRunner implementation.
    /// The request identity retains explicit metadata and the original array
    /// order; defaults and stable priority ordering belong to normalization.
    pub fn execute_reason(
        &self,
        input: &SemanticReasonRequest,
    ) -> Result<SemanticReasonResult, SemanticRuntimeFailure> {
        input
            .validate()
            .map_err(|errors| failure("invalid_contract", "contract", errors.join("; ")))?;
        let request = serde_json::to_value(input)
            .map_err(|error| failure("invalid_contract", "contract", error))?;
        let ordered = semantica_json::ordered_json(&request)
            .map_err(|error| failure("invalid_contract", "contract", error))?;
        if ordered.len() > crate::reasoner::MAX_INPUT_BYTES {
            return Err(failure(
                "input_budget_exceeded",
                "contract",
                "Complete reasoning request exceeds the existing reasoner byte budget",
            ));
        }
        let canonical = semantica_json::canonical_json(&request)
            .map_err(|error| failure("invalid_contract", "contract", error))?;
        let data =
            crate::reasoner::forward_chain_values(&input.facts, &input.rules, input.max_iterations)
                .map_err(|error| failure("reason_execution_failed", "reason", error))?;
        let response = SemanticReasonResult {
            schema: "semantica.reason-result/1".into(),
            request_sha256: crate::sha256(canonical),
            data,
        };
        response
            .validate()
            .map_err(|errors| failure("result_contract_invalid", "result", errors.join("; ")))?;
        Ok(response)
    }

    /// Explicit profile2 delegates to the same graph owner after complete input
    /// identity and parameter checks. It never changes neutral1 defaults.
    pub fn execute_query_with_options_v2(
        &self,
        input: &SemanticQueryOptionsRequestV2,
    ) -> Result<SemanticQueryOptionsResultV2, SemanticRuntimeFailure> {
        input
            .validate()
            .map_err(|errors| failure("invalid_contract", "contract", errors.join("; ")))?;
        if input.data.len() > crate::MAX_BYTES
            || input.query.len() > 1024 * 1024
            || input.namespaces.len() > 128
            || input.bindings.len() > 128
        {
            return Err(failure(
                "input_budget_exceeded",
                "contract",
                "RDF, query or parameter count exceeds the original local runtime budget",
            ));
        }
        let parameter_bytes: usize = input
            .namespaces
            .iter()
            .map(|(k, v)| k.len() + v.len())
            .sum::<usize>()
            + input
                .bindings
                .iter()
                .map(|(k, v)| k.len() + v.len())
                .sum::<usize>();
        if parameter_bytes > crate::MAX_BYTES {
            return Err(failure(
                "input_budget_exceeded",
                "contract",
                "Query parameter bytes exceed the original budget",
            ));
        }
        if crate::sha256(input.data.as_bytes()) != input.data_sha256 {
            return Err(failure(
                "source_identity_mismatch",
                "source",
                "RDF source bytes differ from the declared identity",
            ));
        }
        let format = match input.rdf_format.as_str() {
            "turtle" => RdfFormat::Turtle,
            "rdfxml" => RdfFormat::RdfXml,
            "ntriples" => RdfFormat::NTriples,
            _ => {
                return Err(failure(
                    "unsupported_rdf_format",
                    "parse",
                    &input.rdf_format,
                ));
            }
        };
        let mut bindings = BTreeMap::new();
        for (name, value) in &input.bindings {
            let line = format!("<urn:parameter> <urn:value> {value} .");
            let mut parser =
                RdfParser::from_format(RdfFormat::NTriples).for_reader(line.as_bytes());
            let term = parser
                .next()
                .ok_or_else(|| {
                    failure(
                        "binding_parse_failed",
                        "parse",
                        "One original NTriples RDF term is required",
                    )
                })?
                .map_err(|error| failure("binding_parse_failed", "parse", error))?
                .object;
            if parser.next().is_some() {
                return Err(failure(
                    "binding_parse_failed",
                    "parse",
                    "Additional triples or input are not a single RDF binding term",
                ));
            }
            bindings.insert(name.clone(), term);
        }
        let data = crate::graph::load_format(input.data.as_bytes(), format)
            .map_err(|error| failure("rdf_parse_failed", "parse", error))?;
        let (query_kind, result) = crate::graph::query_rdflib_with_options_and_kind(
            &data,
            &input.query,
            &input.namespaces,
            &bindings,
        )
        .map_err(|error| failure("query_execution_failed", "query", error))?;
        let query_result = SemanticQueryResult {
            execution: "completed".into(),
            query_kind,
            data_sha256: input.data_sha256.clone(),
            result,
        };
        query_result
            .validate()
            .map_err(|errors| failure("result_contract_invalid", "result", errors.join("; ")))?;
        let request = serde_json::to_value(input)
            .map_err(|error| failure("invalid_contract", "contract", error))?;
        let canonical = semantica_json::canonical_json(&request)
            .map_err(|error| failure("invalid_contract", "contract", error))?;
        let response = SemanticQueryOptionsResultV2 {
            schema: "semantica.query-options-result/2".into(),
            profile: input.profile.clone(),
            request_sha256: crate::sha256(canonical),
            query_result,
        };
        response
            .validate()
            .map_err(|errors| failure("result_contract_invalid", "result", errors.join("; ")))?;
        Ok(response)
    }
    /// Explicit profile3; all execution and owned budgets use the shared path.
    pub fn execute_query_with_options_v3(
        &self,
        input: &SemanticQueryOptionsRequestV3,
    ) -> Result<SemanticQueryOptionsResultV3, SemanticRuntimeFailure> {
        input
            .validate()
            .map_err(|errors| failure("invalid_contract", "contract", errors.join("; ")))?;
        let query_result = self.execute_bounded_query_options(
            QueryOptionsInput {
                data: &input.data,
                data_sha256: &input.data_sha256,
                rdf_format: &input.rdf_format,
                query: &input.query,
                namespaces: &input.namespaces,
                bindings: &input.bindings,
                budget: &input.budget,
                guard: &input.guard,
            },
            false,
        )?;
        let request = serde_json::to_value(input)
            .map_err(|error| failure("invalid_contract", "contract", error))?;
        let canonical = semantica_json::canonical_json(&request)
            .map_err(|error| failure("invalid_contract", "contract", error))?;
        let response = SemanticQueryOptionsResultV3 {
            schema: "semantica.query-options-result/3".into(),
            profile: input.profile.clone(),
            request_sha256: crate::sha256(canonical),
            query_result,
        };
        response
            .validate()
            .map_err(|errors| failure("result_contract_invalid", "result", errors.join("; ")))?;
        Ok(response)
    }
    /// Explicit profile4; all execution and owned budgets use the shared path.
    pub fn execute_query_with_options_v4(
        &self,
        input: &SemanticQueryOptionsRequestV4,
    ) -> Result<SemanticQueryOptionsResultV4, SemanticRuntimeFailure> {
        input
            .validate()
            .map_err(|errors| failure("invalid_contract", "contract", errors.join("; ")))?;
        let query_result = self.execute_bounded_query_options(
            QueryOptionsInput {
                data: &input.data,
                data_sha256: &input.data_sha256,
                rdf_format: &input.rdf_format,
                query: &input.query,
                namespaces: &input.namespaces,
                bindings: &input.bindings,
                budget: &input.budget,
                guard: &input.guard,
            },
            true,
        )?;
        let request = serde_json::to_value(input)
            .map_err(|error| failure("invalid_contract", "contract", error))?;
        let canonical = semantica_json::canonical_json(&request)
            .map_err(|error| failure("invalid_contract", "contract", error))?;
        let response = SemanticQueryOptionsResultV4 {
            schema: "semantica.query-options-result/4".into(),
            profile: input.profile.clone(),
            request_sha256: crate::sha256(canonical),
            query_result,
        };
        response
            .validate()
            .map_err(|errors| failure("result_contract_invalid", "result", errors.join("; ")))?;
        Ok(response)
    }
    fn execute_bounded_query_options(
        &self,
        input: QueryOptionsInput<'_>,
        group_null_projection: bool,
    ) -> Result<SemanticQueryResult, SemanticRuntimeFailure> {
        input
            .guard
            .validate()
            .map_err(|errors| failure("invalid_contract", "contract", errors.join("; ")))?;
        input
            .budget
            .validate()
            .map_err(|errors| failure("invalid_contract", "contract", errors.join("; ")))?;
        let b = input.budget;
        let bounded = [
            (b.data_bytes, crate::MAX_BYTES as u64),
            (b.query_bytes, 1024 * 1024),
            (b.parameter_count, 128),
            (b.parameter_bytes, crate::MAX_BYTES as u64),
            (b.quads, 100_000),
            (b.rows, 10_000),
            (b.triples, 100_000),
            (b.ast_depth, 96),
            (b.ast_nodes, 100_000),
        ]
        .iter()
        .all(|(actual, cap)| *actual > 0 && actual <= cap);
        if !bounded || b.stack_bytes != crate::graph::QUERY_STACK_BYTES as u64 {
            return Err(failure(
                "invalid_owned_budget",
                "contract",
                "Budgets may only tighten owned limits; stack is exactly8MiB",
            ));
        }
        let parameter_bytes: usize = input
            .namespaces
            .iter()
            .map(|(k, v)| k.len() + v.len())
            .sum::<usize>()
            + input
                .bindings
                .iter()
                .map(|(k, v)| k.len() + v.len())
                .sum::<usize>();
        if input.data.len() as u64 > b.data_bytes
            || input.query.len() as u64 > b.query_bytes
            || input.namespaces.len() as u64 > b.parameter_count
            || input.bindings.len() as u64 > b.parameter_count
            || parameter_bytes as u64 > b.parameter_bytes
        {
            return Err(failure(
                "input_budget_exceeded",
                "contract",
                "Complete source/query/parameter bytes exceed the explicit local budget",
            ));
        }
        if crate::sha256(input.data.as_bytes()) != input.data_sha256 {
            return Err(failure(
                "source_identity_mismatch",
                "source",
                "RDF source bytes differ from the declared identity",
            ));
        }
        let format = match input.rdf_format {
            "turtle" => RdfFormat::Turtle,
            "rdfxml" => RdfFormat::RdfXml,
            "ntriples" => RdfFormat::NTriples,
            _ => {
                return Err(failure("unsupported_rdf_format", "parse", input.rdf_format));
            }
        };
        let mut bindings = BTreeMap::new();
        for (name, value) in input.bindings {
            let line = format!("<urn:parameter> <urn:value> {value} .");
            let mut parser =
                RdfParser::from_format(RdfFormat::NTriples).for_reader(line.as_bytes());
            let term = parser
                .next()
                .ok_or_else(|| {
                    failure(
                        "binding_parse_failed",
                        "parse",
                        "One original NTriples RDF term is required",
                    )
                })?
                .map_err(|error| failure("binding_parse_failed", "parse", error))?
                .object;
            if parser.next().is_some() {
                return Err(failure(
                    "binding_parse_failed",
                    "parse",
                    "Additional input is not a single RDF binding term",
                ));
            }
            bindings.insert(name.clone(), term);
        }
        let data = crate::graph::load_format_with_quad_budget(
            input.data.as_bytes(),
            format,
            b.quads as usize,
        )
        .map_err(|error| failure("rdf_parse_failed", "parse", error))?;
        let (query_kind, result) = crate::graph::query_shadow_with_options_and_kind(
            &data,
            input.query,
            input.namespaces,
            &bindings,
            crate::graph::QueryScopeLimits {
                initial_shadows: true,
                group_null_projection,
                ast_depth: b.ast_depth as usize,
                ast_nodes: b.ast_nodes as usize,
                rows: b.rows as usize,
                triples: b.triples as usize,
            },
        )
        .map_err(|error| failure("query_execution_failed", "query", error))?;
        let query_result = SemanticQueryResult {
            execution: "completed".into(),
            query_kind,
            data_sha256: input.data_sha256.to_owned(),
            result,
        };
        query_result
            .validate()
            .map_err(|errors| failure("result_contract_invalid", "result", errors.join("; ")))?;
        Ok(query_result)
    }
    /// Validate the generated contract and its original byte identity before parsing.
    /// An empty RDF graph is valid; an empty or unsupported query is not successful execution.
    pub fn execute_query(
        &self,
        input: &SemanticQueryRequest,
    ) -> Result<SemanticQueryResult, SemanticRuntimeFailure> {
        input
            .validate()
            .map_err(|errors| failure("invalid_contract", "contract", errors.join("; ")))?;
        if input.data.len() > crate::MAX_BYTES || input.query.len() > 1024 * 1024 {
            return Err(failure(
                "input_budget_exceeded",
                "contract",
                "RDF or query exceeds its declared runtime byte budget",
            ));
        }
        if crate::sha256(input.data.as_bytes()) != input.data_sha256 {
            return Err(failure(
                "source_identity_mismatch",
                "source",
                "RDF source bytes differ from the declared identity",
            ));
        }
        let format = match input.rdf_format.as_str() {
            "turtle" => RdfFormat::Turtle,
            "rdfxml" => RdfFormat::RdfXml,
            "ntriples" => RdfFormat::NTriples,
            _ => {
                return Err(failure(
                    "unsupported_rdf_format",
                    "parse",
                    &input.rdf_format,
                ));
            }
        };
        let data = crate::graph::load_format(input.data.as_bytes(), format)
            .map_err(|error| failure("rdf_parse_failed", "parse", error))?;
        let (query_kind, result) = crate::graph::query_with_kind(&data, &input.query)
            .map_err(|error| failure("query_execution_failed", "query", error))?;
        let response = SemanticQueryResult {
            execution: "completed".into(),
            query_kind,
            data_sha256: input.data_sha256.clone(),
            result,
        };
        response
            .validate()
            .map_err(|errors| failure("result_contract_invalid", "result", errors.join("; ")))?;
        Ok(response)
    }
}

impl SemanticQueryPort for RustSemanticRuntime {
    fn query(
        &self,
        input: &SemanticQueryRequest,
    ) -> Result<SemanticQueryResult, SemanticRuntimeFailure> {
        self.execute_query(input)
    }
}

impl SemanticReasonPort for RustSemanticRuntime {
    fn reason(
        &self,
        input: &SemanticReasonRequest,
    ) -> Result<SemanticReasonResult, SemanticRuntimeFailure> {
        self.execute_reason(input)
    }
}

impl SemanticQueryOptionsPortV2 for RustSemanticRuntime {
    fn query_with_options_v2(
        &self,
        input: &SemanticQueryOptionsRequestV2,
    ) -> Result<SemanticQueryOptionsResultV2, SemanticRuntimeFailure> {
        self.execute_query_with_options_v2(input)
    }
}

impl SemanticQueryOptionsPortV3 for RustSemanticRuntime {
    fn query_with_options_v3(
        &self,
        input: &SemanticQueryOptionsRequestV3,
    ) -> Result<SemanticQueryOptionsResultV3, SemanticRuntimeFailure> {
        self.execute_query_with_options_v3(input)
    }
}

impl SemanticNamedGraphValidationPort for RustSemanticRuntime {
    fn validate_named_graph(
        &self,
        input: &SemanticNamedGraphValidationRequest,
    ) -> Result<SemanticNamedGraphValidationResult, SemanticRuntimeFailure> {
        self.execute_named_graph_validation(input)
    }
}

impl SemanticQueryOptionsPortV4 for RustSemanticRuntime {
    fn query_with_options_v4(
        &self,
        input: &SemanticQueryOptionsRequestV4,
    ) -> Result<SemanticQueryOptionsResultV4, SemanticRuntimeFailure> {
        self.execute_query_with_options_v4(input)
    }
}
