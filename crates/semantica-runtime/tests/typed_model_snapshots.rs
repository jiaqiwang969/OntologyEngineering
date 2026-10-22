use semantica_contracts::*;
use semantica_runtime::schema::RustSchemaRuntime;
use serde_json::{Value, json};
fn roundtrip<T: SemanticModelSnapshot>(
    runtime: &RustSchemaRuntime,
    input: &Value,
) -> Result<Value, SemanticSchemaFailure> {
    let snapshot = runtime.validate_typed::<T>(input)?;
    Ok(serde_json::to_value(snapshot).unwrap())
}
#[test]
fn generated_strong_views_keep_all_original_model_validation_and_json_identity() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../ORIGINAL106-FROZEN-EXPECTATIONS.json");
    let frozen: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let runtime = RustSchemaRuntime::new().unwrap();
    let mut valid = 0;
    let mut invalid = 0;
    for case in frozen["cases"].as_array().unwrap() {
        let actual = match case["model"].as_str().unwrap() {
            "integrations.crewai.decision_tool::SemanticaDecisionToolInput" => {
                roundtrip::<SemanticModelIntegrationsCrewaiDecisionToolSemanticaDecisionToolInput>(
                    &runtime,
                    &case["input"],
                )
            }
            "integrations.crewai.kg_tool::SemanticaKGToolInput" => roundtrip::<
                SemanticModelIntegrationsCrewaiKgToolSemanticaKGToolInput,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.routes.ontology::OntologyEntry" => roundtrip::<
                SemanticModelSemanticaExplorerRoutesOntologyOntologyEntry,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.routes.ontology::OntologyPreview" => roundtrip::<
                SemanticModelSemanticaExplorerRoutesOntologyOntologyPreview,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.routes.ontology::LoadOntologyRequest" => roundtrip::<
                SemanticModelSemanticaExplorerRoutesOntologyLoadOntologyRequest,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.routes.ontology::PreviewOntologyRequest" => {
                roundtrip::<SemanticModelSemanticaExplorerRoutesOntologyPreviewOntologyRequest>(
                    &runtime,
                    &case["input"],
                )
            }
            "semantica.explorer.routes.ontology::CreateOntologyRequest" => {
                roundtrip::<SemanticModelSemanticaExplorerRoutesOntologyCreateOntologyRequest>(
                    &runtime,
                    &case["input"],
                )
            }
            "semantica.explorer.routes.ontology::OntologySearchResult" => {
                roundtrip::<SemanticModelSemanticaExplorerRoutesOntologyOntologySearchResult>(
                    &runtime,
                    &case["input"],
                )
            }
            "semantica.explorer.routes.ontology::EntityDetailResponse" => {
                roundtrip::<SemanticModelSemanticaExplorerRoutesOntologyEntityDetailResponse>(
                    &runtime,
                    &case["input"],
                )
            }
            "semantica.explorer.routes.ontology::SKOSScheme" => roundtrip::<
                SemanticModelSemanticaExplorerRoutesOntologySKOSScheme,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.routes.ontology::SKOSConceptDetail" => roundtrip::<
                SemanticModelSemanticaExplorerRoutesOntologySKOSConceptDetail,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.routes.ontology::SKOSConceptSearchRequest" => {
                roundtrip::<SemanticModelSemanticaExplorerRoutesOntologySKOSConceptSearchRequest>(
                    &runtime,
                    &case["input"],
                )
            }
            "semantica.explorer.routes.ontology::LoadOntologyResponse" => {
                roundtrip::<SemanticModelSemanticaExplorerRoutesOntologyLoadOntologyResponse>(
                    &runtime,
                    &case["input"],
                )
            }
            "semantica.explorer.routes.ontology::ToggleResponse" => roundtrip::<
                SemanticModelSemanticaExplorerRoutesOntologyToggleResponse,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.routes.ontology::RefreshResponse" => roundtrip::<
                SemanticModelSemanticaExplorerRoutesOntologyRefreshResponse,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.routes.ontology::OntologyAlignment" => roundtrip::<
                SemanticModelSemanticaExplorerRoutesOntologyOntologyAlignment,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.routes.ontology::OntologyAlignmentRequest" => {
                roundtrip::<SemanticModelSemanticaExplorerRoutesOntologyOntologyAlignmentRequest>(
                    &runtime,
                    &case["input"],
                )
            }
            "semantica.explorer.routes.ontology::AlignmentSuggestionRequest" => {
                roundtrip::<SemanticModelSemanticaExplorerRoutesOntologyAlignmentSuggestionRequest>(
                    &runtime,
                    &case["input"],
                )
            }
            "semantica.explorer.routes.ontology::AlignmentSuggestion" => roundtrip::<
                SemanticModelSemanticaExplorerRoutesOntologyAlignmentSuggestion,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.routes.ontology::HealthDimension" => roundtrip::<
                SemanticModelSemanticaExplorerRoutesOntologyHealthDimension,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.routes.ontology::HealthIssue" => roundtrip::<
                SemanticModelSemanticaExplorerRoutesOntologyHealthIssue,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.routes.ontology::OntologyHealthResponse" => {
                roundtrip::<SemanticModelSemanticaExplorerRoutesOntologyOntologyHealthResponse>(
                    &runtime,
                    &case["input"],
                )
            }
            "semantica.explorer.routes.ontology::ShaclGenerateRequest" => {
                roundtrip::<SemanticModelSemanticaExplorerRoutesOntologyShaclGenerateRequest>(
                    &runtime,
                    &case["input"],
                )
            }
            "semantica.explorer.routes.ontology::ShaclValidateRequest" => {
                roundtrip::<SemanticModelSemanticaExplorerRoutesOntologyShaclValidateRequest>(
                    &runtime,
                    &case["input"],
                )
            }
            "semantica.explorer.routes.ontology::ShaclViolation" => roundtrip::<
                SemanticModelSemanticaExplorerRoutesOntologyShaclViolation,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.routes.ontology::ShaclShapeSummary" => roundtrip::<
                SemanticModelSemanticaExplorerRoutesOntologyShaclShapeSummary,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.routes.ontology::ShaclGenerateResponse" => {
                roundtrip::<SemanticModelSemanticaExplorerRoutesOntologyShaclGenerateResponse>(
                    &runtime,
                    &case["input"],
                )
            }
            "semantica.explorer.routes.ontology::ShaclShapesResponse" => roundtrip::<
                SemanticModelSemanticaExplorerRoutesOntologyShaclShapesResponse,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.routes.ontology::ShaclValidationResponse" => {
                roundtrip::<SemanticModelSemanticaExplorerRoutesOntologyShaclValidationResponse>(
                    &runtime,
                    &case["input"],
                )
            }
            "semantica.explorer.routes.ontology::DraftDiff" => roundtrip::<
                SemanticModelSemanticaExplorerRoutesOntologyDraftDiff,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.routes.ontology::DraftRequest" => roundtrip::<
                SemanticModelSemanticaExplorerRoutesOntologyDraftRequest,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.routes.ontology::DraftResponse" => roundtrip::<
                SemanticModelSemanticaExplorerRoutesOntologyDraftResponse,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.routes.ontology::ProposalState" => roundtrip::<
                SemanticModelSemanticaExplorerRoutesOntologyProposalState,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.routes.ontology::ProposalRequest" => roundtrip::<
                SemanticModelSemanticaExplorerRoutesOntologyProposalRequest,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.routes.ontology::ProposalResponse" => roundtrip::<
                SemanticModelSemanticaExplorerRoutesOntologyProposalResponse,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.routes.ontology::CommentRequest" => roundtrip::<
                SemanticModelSemanticaExplorerRoutesOntologyCommentRequest,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.routes.ontology::VersionEntry" => roundtrip::<
                SemanticModelSemanticaExplorerRoutesOntologyVersionEntry,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.routes.ontology::VersionCompareRequest" => {
                roundtrip::<SemanticModelSemanticaExplorerRoutesOntologyVersionCompareRequest>(
                    &runtime,
                    &case["input"],
                )
            }
            "semantica.explorer.routes.ontology::VersionCompareResponse" => {
                roundtrip::<SemanticModelSemanticaExplorerRoutesOntologyVersionCompareResponse>(
                    &runtime,
                    &case["input"],
                )
            }
            "semantica.explorer.routes.ontology::AlignmentRequest" => roundtrip::<
                SemanticModelSemanticaExplorerRoutesOntologyAlignmentRequest,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.routes.ontology::AlignmentResponse" => roundtrip::<
                SemanticModelSemanticaExplorerRoutesOntologyAlignmentResponse,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.routes.sparql::SparqlRequest" => roundtrip::<
                SemanticModelSemanticaExplorerRoutesSparqlSparqlRequest,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.routes.sparql::SparqlResponse" => roundtrip::<
                SemanticModelSemanticaExplorerRoutesSparqlSparqlResponse,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.routes.temporal::TemporalSnapshotFastResponse" => {
                roundtrip::<SemanticModelSemanticaExplorerRoutesTemporalTemporalSnapshotFastResponse>(
                    &runtime,
                    &case["input"],
                )
            }
            "semantica.explorer.routes.temporal::TemporalBoundsResponse" => {
                roundtrip::<SemanticModelSemanticaExplorerRoutesTemporalTemporalBoundsResponse>(
                    &runtime,
                    &case["input"],
                )
            }
            "semantica.explorer.schemas::ErrorResponse" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasErrorResponse,
            >(&runtime, &case["input"]),
            "semantica.explorer.schemas::NodeResponse" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasNodeResponse,
            >(&runtime, &case["input"]),
            "semantica.explorer.schemas::EdgeResponse" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasEdgeResponse,
            >(&runtime, &case["input"]),
            "semantica.explorer.schemas::NodeListResponse" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasNodeListResponse,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.schemas::EdgeListResponse" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasEdgeListResponse,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.schemas::NeighborResponse" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasNeighborResponse,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.schemas::PathResponse" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasPathResponse,
            >(&runtime, &case["input"]),
            "semantica.explorer.schemas::GraphStatsResponse" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasGraphStatsResponse,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.schemas::SearchRequest" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasSearchRequest,
            >(&runtime, &case["input"]),
            "semantica.explorer.schemas::SearchResultItem" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasSearchResultItem,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.schemas::SearchResultResponse" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasSearchResultResponse,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.schemas::AnalyticsResponse" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasAnalyticsResponse,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.schemas::ValidationIssue" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasValidationIssue,
            >(&runtime, &case["input"]),
            "semantica.explorer.schemas::ValidationReportResponse" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasValidationReportResponse,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.schemas::DecisionResponse" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasDecisionResponse,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.schemas::CausalChainResponse" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasCausalChainResponse,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.schemas::ComplianceResponse" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasComplianceResponse,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.schemas::TemporalSnapshotResponse" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasTemporalSnapshotResponse,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.schemas::TemporalDiffResponse" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasTemporalDiffResponse,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.schemas::TemporalPatternResponse" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasTemporalPatternResponse,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.schemas::EnrichExtractRequest" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasEnrichExtractRequest,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.schemas::EnrichExtractResponse" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasEnrichExtractResponse,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.schemas::LinkPredictionRequest" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasLinkPredictionRequest,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.schemas::LinkPredictionResponse" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasLinkPredictionResponse,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.schemas::DedupRequest" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasDedupRequest,
            >(&runtime, &case["input"]),
            "semantica.explorer.schemas::DedupResponse" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasDedupResponse,
            >(&runtime, &case["input"]),
            "semantica.explorer.schemas::ReasoningRequest" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasReasoningRequest,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.schemas::ReasoningResponse" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasReasoningResponse,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.schemas::ExportRequest" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasExportRequest,
            >(&runtime, &case["input"]),
            "semantica.explorer.schemas::ExportResponse" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasExportResponse,
            >(&runtime, &case["input"]),
            "semantica.explorer.schemas::ImportResponse" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasImportResponse,
            >(&runtime, &case["input"]),
            "semantica.explorer.schemas::StandardMessageResponse" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasStandardMessageResponse,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.schemas::AnnotationCreate" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasAnnotationCreate,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.schemas::AnnotationResponse" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasAnnotationResponse,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.schemas::VocabularyScheme" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasVocabularyScheme,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.schemas::ConceptSummary" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasConceptSummary,
            >(&runtime, &case["input"]),
            "semantica.explorer.schemas::ConceptNode" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasConceptNode,
            >(&runtime, &case["input"]),
            "semantica.explorer.schemas::VocabularyImportResponse" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasVocabularyImportResponse,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.schemas::MergeRequest" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasMergeRequest,
            >(&runtime, &case["input"]),
            "semantica.explorer.schemas::MergeResponse" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasMergeResponse,
            >(&runtime, &case["input"]),
            "semantica.explorer.schemas::ProvenanceNode" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasProvenanceNode,
            >(&runtime, &case["input"]),
            "semantica.explorer.schemas::ProvenanceEdge" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasProvenanceEdge,
            >(&runtime, &case["input"]),
            "semantica.explorer.schemas::ProvenanceResponse" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasProvenanceResponse,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.schemas::DistanceMatrixRequest" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasDistanceMatrixRequest,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.schemas::DistanceMatrixResponse" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasDistanceMatrixResponse,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.schemas::SemanticNeighborItem" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasSemanticNeighborItem,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.schemas::SemanticNeighborhoodResponse" => {
                roundtrip::<SemanticModelSemanticaExplorerSchemasSemanticNeighborhoodResponse>(
                    &runtime,
                    &case["input"],
                )
            }
            "semantica.explorer.schemas::CausalDistanceReport" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasCausalDistanceReport,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.schemas::DistanceSnapshot" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasDistanceSnapshot,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.schemas::DistanceEvent" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasDistanceEvent,
            >(&runtime, &case["input"]),
            "semantica.explorer.schemas::DistanceHistoryResponse" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasDistanceHistoryResponse,
            >(
                &runtime, &case["input"]
            ),
            "semantica.explorer.schemas::DistanceExportRequest" => roundtrip::<
                SemanticModelSemanticaExplorerSchemasDistanceExportRequest,
            >(
                &runtime, &case["input"]
            ),
            "semantica.semantic_extract.schemas::EntityOut" => roundtrip::<
                SemanticModelSemanticaSemanticExtractSchemasEntityOut,
            >(
                &runtime, &case["input"]
            ),
            "semantica.semantic_extract.schemas::RelationOut" => roundtrip::<
                SemanticModelSemanticaSemanticExtractSchemasRelationOut,
            >(
                &runtime, &case["input"]
            ),
            "semantica.semantic_extract.schemas::TripletOut" => roundtrip::<
                SemanticModelSemanticaSemanticExtractSchemasTripletOut,
            >(
                &runtime, &case["input"]
            ),
            "semantica.semantic_extract.schemas::EntitiesResponse" => roundtrip::<
                SemanticModelSemanticaSemanticExtractSchemasEntitiesResponse,
            >(
                &runtime, &case["input"]
            ),
            "semantica.semantic_extract.schemas::RelationsResponse" => roundtrip::<
                SemanticModelSemanticaSemanticExtractSchemasRelationsResponse,
            >(
                &runtime, &case["input"]
            ),
            "semantica.semantic_extract.schemas::TripletsResponse" => roundtrip::<
                SemanticModelSemanticaSemanticExtractSchemasTripletsResponse,
            >(
                &runtime, &case["input"]
            ),
            "semantica.semantic_extract.schemas::RelationWithTemporalOut" => {
                roundtrip::<SemanticModelSemanticaSemanticExtractSchemasRelationWithTemporalOut>(
                    &runtime,
                    &case["input"],
                )
            }
            "semantica.semantic_extract.schemas::RelationsWithTemporalResponse" => {
                roundtrip::<SemanticModelSemanticaSemanticExtractSchemasRelationsWithTemporalResponse>(
                    &runtime,
                    &case["input"],
                )
            }
            "semantica.server::BuildRequest" => {
                roundtrip::<SemanticModelSemanticaServerBuildRequest>(&runtime, &case["input"])
            }
            other => panic!("unmapped original model {}", other),
        };
        if case["original"]["status"] == "validated" {
            valid += 1;
            assert_eq!(
                actual.unwrap(),
                case["original"]["output"],
                "{}",
                case["id"]
            );
        } else {
            invalid += 1;
            assert_eq!(
                actual.unwrap_err().code,
                "schema_model_validation_failed",
                "{}",
                case["id"]
            );
        }
    }
    assert_eq!(valid, 378);
    assert_eq!(invalid, 1122);
}
#[test]
fn strong_dictionary_views_keep_original_order_and_reserved_literal_objects() {
    let input=semantica_contracts::json::protocol_json(r#"{"id":"n","type":"entity","properties":{"z":{"$serde_json::private::Number":"42"},"a":{"$serde_json::private::RawValue":"true"},"b":-0.0}}"#,4096).unwrap();
    let snapshot = RustSchemaRuntime::new()
        .unwrap()
        .validate_typed::<SemanticModelSemanticaExplorerSchemasNodeResponse>(&input)
        .unwrap();
    assert_eq!(
        snapshot
            .properties
            .iter()
            .map(|(k, _)| k)
            .collect::<Vec<_>>(),
        vec!["z", "a", "b"]
    );
    assert_eq!(
        snapshot.properties.get("z").unwrap(),
        &json!({"$serde_json::private::Number":"42"})
    );
    let output = serde_json::to_value(snapshot).unwrap();
    assert_eq!(output["properties"], input["properties"]);
}
