//! Original manual typed repair loop. The injected upper dispatch authority
//! owns every model credential and budget; this capability owns no model pool.
use crate::schema::RustSchemaRuntime;
use semantica_contracts::*;
use serde_json::Value;
use std::time::Duration;

pub struct RustTypedRuntime<'a> {
    schema: &'a RustSchemaRuntime,
    dispatch: &'a dyn SemanticTypedDispatchPort,
}
impl<'a> RustTypedRuntime<'a> {
    pub fn new(schema: &'a RustSchemaRuntime, dispatch: &'a dyn SemanticTypedDispatchPort) -> Self {
        Self { schema, dispatch }
    }
    /// Explicit Rust successor of the original omitted max_retries argument.
    /// The wire DTO remains explicit; omitted arguments here use original three.
    pub fn generate_default(
        &self,
        generation: SemanticProviderRequest,
        model_id: &str,
    ) -> Result<SemanticTypedGenerationResult, SemanticProviderFailure> {
        self.generate_typed(&SemanticTypedGenerationRequest {
            schema: "semantica.typed-generation-request/1".into(),
            generation,
            model_id: model_id.into(),
            profile: "semantica.pydantic-json/1".into(),
            max_retries: 3,
        })
    }
    /// Strong Rust consumer of the original typed result. The generated type
    /// supplies its exact model identity; every physical call stays on the same
    /// injected dispatch/budget authority, and the receipt is preserved.
    pub fn generate_snapshot<T: SemanticModelSnapshot>(
        &self,
        generation: SemanticProviderRequest,
        max_retries: i64,
    ) -> Result<(T, SemanticTypedGenerationResult), SemanticProviderFailure> {
        let result = self.generate_typed(&SemanticTypedGenerationRequest {
            schema: "semantica.typed-generation-request/1".into(),
            generation,
            model_id: T::MODEL_ID.into(),
            profile: "semantica.pydantic-json/1".into(),
            max_retries,
        })?;
        if result.execution != "completed" {
            return Err(result
                .failure
                .unwrap_or_else(|| crate::providers::failure("typed_generation_failed", "typed")));
        }
        let snapshot = self
            .schema
            .validate_typed::<T>(&result.output)
            .map_err(|x| crate::providers::failure(&x.code, "schema"))?;
        Ok((snapshot, result))
    }
    pub fn generate_registered_snapshot(
        &self,
        generation: SemanticProviderRequest,
        model_id: &str,
        max_retries: i64,
    ) -> Result<
        (
            SemanticRegisteredModelSnapshot,
            SemanticTypedGenerationResult,
        ),
        SemanticProviderFailure,
    > {
        let result = self.generate_typed(&SemanticTypedGenerationRequest {
            schema: "semantica.typed-generation-request/1".into(),
            generation,
            model_id: model_id.into(),
            profile: "semantica.pydantic-json/1".into(),
            max_retries,
        })?;
        if result.execution != "completed" {
            return Err(result
                .failure
                .unwrap_or_else(|| crate::providers::failure("typed_generation_failed", "typed")));
        }
        let snapshot = self
            .schema
            .validate_snapshot(model_id, &result.output)
            .map_err(|x| crate::providers::failure(&x.code, "schema"))?;
        Ok((snapshot, result))
    }
    pub fn generate_with_delay<F: FnMut(Duration)>(
        &self,
        input: &SemanticTypedGenerationRequest,
        mut delay: F,
    ) -> Result<SemanticTypedGenerationResult, SemanticProviderFailure> {
        if input.validate().is_err()
            || input.schema != "semantica.typed-generation-request/1"
            || input.profile != "semantica.pydantic-json/1"
            || input.max_retries > 4096
        {
            return Err(crate::providers::failure(
                "typed_request_invalid",
                "validate",
            ));
        }
        self.schema
            .model_json_schema(&input.model_id)
            .map_err(|_| crate::providers::failure("typed_model_unknown", "validate"))?;
        let mut result = SemanticTypedGenerationResult {
            schema: "semantica.typed-generation-result/1".into(),
            execution: "failed".into(),
            model_id: input.model_id.clone(),
            profile: input.profile.clone(),
            output: Value::Null,
            attempts: vec![],
            failure: Some(crate::providers::failure(
                "typed_generation_exhausted",
                "typed",
            )),
        };
        let mut request = input.generation.clone();
        for index in 0..input.max_retries.max(0) as u64 {
            let mut parsed = None;
            let mut last_failure = None;
            for mode in ["structured", "plain"] {
                let dispatched = self.dispatch.dispatch_typed(&SemanticTypedDispatchRequest {
                    schema: "semantica.typed-dispatch-request/1".into(),
                    generation: request.clone(),
                    mode: mode.into(),
                });
                let mut attempt = SemanticTypedAttempt {
                    iteration: index + 1,
                    stage: mode.into(),
                    request: request.clone(),
                    result: None,
                    validation: None,
                    failure: None,
                    reserved: false,
                };
                match dispatched {
                    Ok(observation) => {
                        if observation.validate().is_err()
                            || observation.schema != "semantica.typed-dispatch-result/1"
                        {
                            return Err(crate::providers::failure(
                                "typed_dispatch_result_invalid",
                                "guard",
                            ));
                        }
                        if observation.execution == "completed" && observation.failure.is_none() {
                            let mut expected = request.clone();
                            expected.operation = "generate".into();
                            if mode == "structured" {
                                expected.prompt =
                                    crate::providers::base_structured_prompt(&expected.prompt);
                            }
                            let response = observation.response.as_ref().ok_or_else(|| {
                                crate::providers::failure("typed_dispatch_source_missing", "guard")
                            })?;
                            let source_sha =
                                crate::sha256(serde_json::to_vec(&expected).map_err(|_| {
                                    crate::providers::failure("typed_request_invalid", "validate")
                                })?);
                            let parsed_source = response
                                .output
                                .as_str()
                                .ok_or_else(|| {
                                    crate::providers::failure(
                                        "typed_dispatch_source_invalid",
                                        "guard",
                                    )
                                })
                                .and_then(crate::providers::parse_structured_json)?;
                            if !observation.reserved
                                || response.request_sha256 != source_sha
                                || response.operation != "generate"
                                || response.provider != request.provider
                                || parsed_source != observation.output
                            {
                                return Err(crate::providers::failure(
                                    "typed_dispatch_source_mismatch",
                                    "guard",
                                ));
                            }
                        }
                        attempt.reserved = observation.reserved;
                        attempt.result = observation.response;
                        if observation.execution == "completed" && observation.failure.is_none() {
                            parsed = Some(observation.output);
                        } else {
                            last_failure = Some(observation.failure.unwrap_or_else(|| {
                                crate::providers::failure("typed_dispatch_failed", "dispatch")
                            }));
                        }
                    }
                    Err(error) => last_failure = Some(error),
                }
                if let Some(error) = last_failure.take() {
                    attempt.failure = Some(error.clone());
                    result.failure = Some(error.clone());
                    result.attempts.push(attempt);
                    if matches!(
                        error.phase.as_str(),
                        "budget" | "guard" | "validate" | "configure"
                    ) {
                        return Ok(result);
                    }
                    if mode == "plain" {
                        last_failure = Some(error);
                        break;
                    }
                } else {
                    result.attempts.push(attempt);
                    break;
                }
            }
            if let Some(value) = parsed {
                let raw = serde_json::to_string(&value)
                    .map_err(|_| crate::providers::failure("typed_json_invalid", "validate"))?;
                let validation =
                    match self
                        .schema
                        .validate_schema(&SemanticSchemaValidationRequest {
                            schema: "semantica.schema-request/1".into(),
                            model_id: input.model_id.clone(),
                            profile: input.profile.clone(),
                            operation: "model_validate".into(),
                            input_sha256: crate::sha256(raw.as_bytes()),
                            input_json: raw,
                        }) {
                        Ok(validation) => validation,
                        Err(error) if error.code == "schema_python_float_overflow" => {
                            let failure = crate::providers::failure(&error.code, "schema");
                            if let Some(attempt) = result.attempts.last_mut() {
                                attempt.failure = Some(failure.clone());
                            }
                            result.failure = Some(failure);
                            if index + 1 < input.max_retries as u64 {
                                delay(Duration::from_secs(1));
                            }
                            continue;
                        }
                        Err(error) => return Err(crate::providers::failure(&error.code, "schema")),
                    };
                if let Some(attempt) = result.attempts.last_mut() {
                    attempt.validation = Some(validation.clone());
                }
                if validation.valid {
                    result.execution = "completed".into();
                    result.output = validation.output;
                    result.failure = None;
                    return Ok(result);
                }
                result.failure = Some(crate::providers::failure(
                    "typed_schema_validation_failed",
                    "schema",
                ));
                if index + 1 < input.max_retries as u64 {
                    let summary = self.schema.validation_error_text(&input.model_id, &value)?;
                    request.prompt = format!(
                        "{}\n\nPrevious response was invalid JSON or didn't match schema:\n{}\n\nPlease fix the errors and return valid JSON matching the schema.",
                        input.generation.prompt, summary
                    );
                    delay(Duration::from_secs(index + 1));
                }
            } else if last_failure.is_some() && index + 1 < input.max_retries as u64 {
                delay(Duration::from_secs(1));
            }
        }
        Ok(result)
    }
}
impl SemanticTypedGenerationPort for RustTypedRuntime<'_> {
    fn generate_typed(
        &self,
        input: &SemanticTypedGenerationRequest,
    ) -> Result<SemanticTypedGenerationResult, SemanticProviderFailure> {
        self.generate_with_delay(input, std::thread::sleep)
    }
}
