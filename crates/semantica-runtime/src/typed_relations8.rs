//! Original typed relation consumers in an explicit finite domain. Source,
//! calls, credentials, deadlines and engineering adoption remain host-owned.
use crate::{schema::RustSchemaRuntime, typed::RustTypedRuntime};
use semantica_contracts::*;
use serde_json::{Value, json};

pub const PROFILE8: &str = "semantica.original-typed-relations-exact-entities/8";
const PROMPTS: &str = include_str!("typed-relation-prompts8.json");
pub const RELATIONS_MODEL8: &str = "semantica.semantic_extract.schemas::RelationsResponse";
pub const TEMPORAL_MODEL8: &str =
    "semantica.semantic_extract.schemas::RelationsWithTemporalResponse";
fn fail(code: &str) -> SemanticProviderFailure {
    crate::providers::failure(code, "validate")
}
fn render(parts: &Value, fields: &[(&str, &str)]) -> Result<String, SemanticProviderFailure> {
    let mut out = String::new();
    for p in parts
        .as_array()
        .ok_or_else(|| fail("typed_relations8_template"))?
    {
        if let Some(s) = p["literal"].as_str() {
            out.push_str(s)
        } else {
            let key = p["field"]
                .as_str()
                .ok_or_else(|| fail("typed_relations8_template"))?;
            out.push_str(
                fields
                    .iter()
                    .find(|(k, _)| *k == key)
                    .ok_or_else(|| fail("typed_relations8_template_field"))?
                    .1,
            );
        }
    }
    Ok(out)
}
/// Preserve exact original AST f-string parts. This domain rejects >80 entities
/// and chunking rather than silently discarding original necessary content.
pub fn prepare_relations8(
    input: &SemanticTypedRelationRequest8,
) -> Result<(SemanticProviderRequest, String), SemanticProviderFailure> {
    if input.validate().is_err()
        || input.schema != "semantica.typed-relation-request/8"
        || input.text.trim().is_empty()
        || input.text.len() > crate::MAX_BYTES
        || input.text.chars().count() > 64000
        || input.max_retries > 4096
        || !input.generation.prompt.is_empty()
        || input.generation.operation != "generate"
        || !matches!(input.generation.provider.as_str(), "openai" | "groq")
    {
        return Err(fail("typed_relations8_request_domain"));
    }
    let entities = input
        .entities
        .as_array()
        .ok_or_else(|| fail("typed_relations8_entities"))?;
    if entities.is_empty() || entities.len() > 80 || input.relation_types.len() > 128 {
        return Err(fail("typed_relations8_entity_or_type_domain"));
    }
    let mut names = Vec::new();
    for e in entities {
        let text = e["text"]
            .as_str()
            .ok_or_else(|| fail("typed_relations8_entity_text"))?;
        let label = e["label"]
            .as_str()
            .ok_or_else(|| fail("typed_relations8_entity_label"))?;
        if text.is_empty() || text.len() > 4096 || label.len() > 512 {
            return Err(fail("typed_relations8_entity_domain"));
        }
        names.push(format!("{text} ({label})"));
    }
    let templates: Value = crate::providers::parse_protocol_json(PROMPTS)
        .map_err(|_| fail("typed_relations8_template"))?;
    let instructions = templates["instructions"]
        .as_array()
        .ok_or_else(|| fail("typed_relations8_template"))?;
    let instruction = if input.relation_types.is_empty() {
        instructions
            .iter()
            .find(|x| x["kind"] == "default")
            .and_then(|x| x["literal"].as_str())
            .ok_or_else(|| fail("typed_relations8_template"))?
            .to_owned()
    } else {
        let selected = instructions
            .iter()
            .find(|x| x["kind"] == "preferred")
            .ok_or_else(|| fail("typed_relations8_template"))?;
        render(
            &selected["parts"],
            &[("relation_types_str", &input.relation_types.join(", "))],
        )?
    };
    let prompt = render(
        &templates[if input.temporal {
            "temporal"
        } else {
            "ordinary"
        }],
        &[
            ("text", &input.text),
            ("entities_str", &names.join(", ")),
            ("relation_types_instruction", &instruction),
        ],
    )?;
    let mut generation = input.generation.clone();
    generation.prompt = prompt;
    Ok((
        generation,
        if input.temporal {
            TEMPORAL_MODEL8
        } else {
            RELATIONS_MODEL8
        }
        .into(),
    ))
}
fn exact_entity8(text: &str, entities: &[Value]) -> Option<Value> {
    // The original first-match fast path lowercases/strips both strings.
    // Exact raw Unicode and ASCII folded matches are the explicit finite domain.
    entities
        .iter()
        .find(|e| {
            e["text"].as_str().is_some_and(|s| {
                s == text
                    || (s.is_ascii()
                        && text.is_ascii()
                        && s.trim().eq_ignore_ascii_case(text.trim()))
            })
        })
        .cloned()
}
pub fn convert_relations8(
    input: &SemanticTypedRelationRequest8,
    value: &Value,
) -> Result<Value, SemanticProviderFailure> {
    let entities = input
        .entities
        .as_array()
        .ok_or_else(|| fail("typed_relations8_entities"))?;
    let rows = value["relations"]
        .as_array()
        .ok_or_else(|| fail("typed_relations8_output_shape"))?;
    let mut out = Vec::new();
    for row in rows {
        let subject = row["subject"]
            .as_str()
            .ok_or_else(|| fail("typed_relations8_output_subject"))?;
        let object = row["object"]
            .as_str()
            .ok_or_else(|| fail("typed_relations8_output_object"))?;
        if subject.is_empty() || object.is_empty() {
            continue;
        }
        let Some(subject) = exact_entity8(subject, entities) else {
            return Err(fail("typed_relations8_hybrid_match_unmigrated"));
        };
        let Some(object) = exact_entity8(object, entities) else {
            return Err(fail("typed_relations8_hybrid_match_unmigrated"));
        };
        let mut metadata = json!({"provider":input.generation.provider,"model":input.generation.model,"extraction_method":"llm_typed"});
        if input.temporal {
            for key in ["valid_from", "valid_until", "temporal_source_text"] {
                metadata[key] = row[key].clone();
            }
            metadata["temporal_confidence"] = row
                .get("temporal_confidence")
                .cloned()
                .unwrap_or(json!(0.0));
        }
        out.push(json!({"subject":subject,"predicate":row.get("predicate").cloned().unwrap_or(json!("related_to")),
            "object":object,"confidence":row.get("confidence").cloned().unwrap_or(json!(0.9)),"context":input.text,"metadata":metadata}));
    }
    Ok(Value::Array(out))
}
pub struct RustTypedRelationExtraction8<'a> {
    pub schema: &'a RustSchemaRuntime,
    pub dispatch: &'a dyn SemanticTypedDispatchPort,
}
impl SemanticTypedRelationPort8 for RustTypedRelationExtraction8<'_> {
    fn extract_typed_relations8(
        &self,
        input: &SemanticTypedRelationRequest8,
    ) -> Result<SemanticTypedRelationResult8, SemanticProviderFailure> {
        let (generation, model_id) = prepare_relations8(input)?;
        let mut result=SemanticTypedRelationResult8{schema:"semantica.typed-relation-result/8".into(),execution:"failed".into(),profile:PROFILE8.into(),
            source_utf8_sha256:crate::sha256(&input.text),typed:None,fallback:None,relations:json!([]),failure:None,
            unknown:vec!["original hybrid matching, synthetic entities, >80 filtering, chunking/cache and Python object domains remain required/unmigrated".into(),
                "entity offsets/transcription/temporal calibration/causality/Chinese and engineering applicability are unknown".into(),
                "original13494/19modules/19protocols/22owners and whole-code RRSI eligibility/use/feedback/recovery remain required/unknown".into()]};
        let runtime = RustTypedRuntime::new(self.schema, self.dispatch);
        let typed = runtime.generate_typed(&SemanticTypedGenerationRequest {
            schema: "semantica.typed-generation-request/1".into(),
            generation: generation.clone(),
            model_id: model_id.clone(),
            profile: "semantica.pydantic-json/1".into(),
            max_retries: input.max_retries,
        })?;
        let mut converted = if typed.execution == "completed" {
            convert_relations8(input, &typed.output)
        } else {
            Err(typed
                .failure
                .clone()
                .unwrap_or_else(|| fail("typed_relations8_generation_failed")))
        };
        result.typed = Some(typed);
        if converted
            .as_ref()
            .is_ok_and(|v| v.as_array().is_some_and(Vec::is_empty))
        {
            // Original empty typed result invokes structured generation through
            // the same dispatch. Each actual call is still host checked.
            match self.dispatch.dispatch_typed(&SemanticTypedDispatchRequest {
                schema: "semantica.typed-dispatch-request/1".into(),
                generation: generation.clone(),
                mode: "structured".into(),
            }) {
                Ok(fallback) => {
                    if fallback.execution == "completed" && fallback.failure.is_none() {
                        let mut actual = generation.clone();
                        actual.operation = "generate".into();
                        actual.prompt = crate::providers::base_structured_prompt(&actual.prompt);
                        let bound = fallback.response.as_ref().is_some_and(|r| {
                            fallback.reserved
                                && r.operation == "generate"
                                && r.provider == actual.provider
                                && serde_json::to_vec(&actual)
                                    .is_ok_and(|b| r.request_sha256 == crate::sha256(b))
                                && r.output.as_str().is_some_and(|s| {
                                    crate::providers::parse_structured_json(s)
                                        .is_ok_and(|v| v == fallback.output)
                                })
                        });
                        converted = if !bound {
                            Err(fail("typed_relations8_fallback_source_mismatch"))
                        } else {
                            self.schema
                                .validate_model(&model_id, &fallback.output)
                                .map_err(|_| fail("typed_relations8_fallback_schema"))
                                .and_then(|validated| {
                                    convert_relations8(input, &validated["output"])
                                })
                        };
                    } else {
                        converted = Err(fallback
                            .failure
                            .clone()
                            .unwrap_or_else(|| fail("typed_relations8_fallback_failed")));
                    }
                    result.fallback = Some(fallback);
                }
                Err(e) => converted = Err(e),
            }
        }
        match converted {
            Ok(relations) => {
                result.relations = relations;
                result.execution = "completed".into();
            }
            Err(e) => result.failure = Some(e),
        }
        Ok(result)
    }
}
