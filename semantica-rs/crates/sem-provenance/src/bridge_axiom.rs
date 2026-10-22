//! Bridge axiom translation chain tracking (port of `semantica/provenance/bridge_axiom.py`).
//!
//! A [`BridgeAxiom`] translates a value from one domain to another (ecological → financial,
//! clinical → diagnostic, evidence → legal conclusion, ...) with audit-grade source tracking
//! (DOI + page + quote); a [`TranslationChain`] records a multi-layer translation (L1 → L2 →
//! L3) whose confidence is the minimum of its axioms' confidences.
//!
//! Values stay JSON values (`serde_json::Value`), as in the original where `input_value` and
//! the layer values may be ints or floats: `463 * 0.346` keeps `input_value: 463` and yields
//! `output_value: 160.19799999999998`.

use std::fmt;

use serde_json::{Map, Number, Value};

use sem_utils::SemanticaError;
use sem_utils::pycompat::fmt::{
    float_repr, map_repr, py_repr, py_str, str_repr, truthy, type_name,
};

use crate::clock;
use crate::manager::{ProvenanceManager, TrackEntityOptions};

fn f64_value(value: f64) -> Value {
    Number::from_f64(value)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

/// `input_value * coefficient` with Python's operand rules (`bool` counts as `int`) and its
/// `TypeError` messages for non-numbers.
fn multiply(value: &Value, coefficient: f64) -> Result<f64, SemanticaError> {
    match value {
        Value::Number(n) => Ok(if let Some(i) = n.as_i64() {
            i as f64 * coefficient
        } else if let Some(u) = n.as_u64() {
            u as f64 * coefficient
        } else {
            n.as_f64().unwrap_or(f64::NAN) * coefficient
        }),
        Value::Bool(b) => Ok(if *b { coefficient } else { 0.0 * coefficient }),
        Value::String(_) | Value::Array(_) => Err(SemanticaError::Type(
            "can't multiply sequence by non-int of type 'float'".to_string(),
        )),
        other => Err(SemanticaError::Type(format!(
            "unsupported operand type(s) for *: '{}' and 'float'",
            type_name(other)
        ))),
    }
}

/// Bridge axiom for domain translation with provenance.
#[derive(Clone, PartialEq)]
pub struct BridgeAxiom {
    /// Unique axiom identifier (e.g. `"BA-001"`).
    pub axiom_id: String,
    /// Human-readable axiom name.
    pub name: String,
    /// The rule in natural language.
    pub rule: String,
    /// Numeric translation coefficient.
    pub coefficient: f64,
    /// DOI of the source paper/document.
    pub source_doi: String,
    /// Page/table/figure in the source.
    pub source_page: String,
    /// Direct quote supporting the coefficient.
    pub source_quote: Option<String>,
    /// Confidence score (default `1.0`).
    pub confidence: f64,
    /// Input domain (default `"unknown"`).
    pub input_domain: String,
    /// Output domain (default `"unknown"`).
    pub output_domain: String,
    /// Additional metadata.
    pub metadata: Map<String, Value>,
}

impl BridgeAxiom {
    /// `BridgeAxiom(axiom_id, name, rule, coefficient, source_doi, source_page)`; the other
    /// fields take their defaults (set them on the returned value).
    pub fn new(
        axiom_id: impl Into<String>,
        name: impl Into<String>,
        rule: impl Into<String>,
        coefficient: f64,
        source_doi: impl Into<String>,
        source_page: impl Into<String>,
    ) -> Self {
        BridgeAxiom {
            axiom_id: axiom_id.into(),
            name: name.into(),
            rule: rule.into(),
            coefficient,
            source_doi: source_doi.into(),
            source_page: source_page.into(),
            source_quote: None,
            confidence: 1.0,
            input_domain: "unknown".to_string(),
            output_domain: "unknown".to_string(),
            metadata: Map::new(),
        }
    }

    /// Apply the axiom: `output_value = input_value * coefficient`, output entity
    /// `"{input_entity}_transformed_{axiom_id}"`. With a manager, the application is tracked as
    /// a `bridge_axiom_result` entity (tracking failures are ignored); `kwargs` are added to
    /// its metadata. Raises Python's `TypeError` for a non-numeric `input_value`.
    pub fn apply(
        &self,
        input_entity: impl Into<Value>,
        input_value: impl Into<Value>,
        prov_manager: Option<&ProvenanceManager>,
        kwargs: &Map<String, Value>,
    ) -> Result<Map<String, Value>, SemanticaError> {
        let input_entity = input_entity.into();
        let input_value = input_value.into();
        let output_value = f64_value(multiply(&input_value, self.coefficient)?);
        let output_entity = format!("{}_transformed_{}", py_str(&input_entity), self.axiom_id);

        if let Some(manager) = prov_manager {
            let mut metadata = Map::new();
            metadata.insert("axiom_id".into(), Value::String(self.axiom_id.clone()));
            metadata.insert("axiom_name".into(), Value::String(self.name.clone()));
            metadata.insert("rule".into(), Value::String(self.rule.clone()));
            metadata.insert("coefficient".into(), f64_value(self.coefficient));
            metadata.insert("input_entity".into(), input_entity.clone());
            metadata.insert("input_value".into(), input_value.clone());
            metadata.insert("output_value".into(), output_value.clone());
            metadata.insert(
                "input_domain".into(),
                Value::String(self.input_domain.clone()),
            );
            metadata.insert(
                "output_domain".into(),
                Value::String(self.output_domain.clone()),
            );
            for (key, value) in kwargs {
                metadata.insert(key.clone(), value.clone());
            }
            let options = TrackEntityOptions {
                entity_type: Some("bridge_axiom_result".to_string()),
                activity_id: Some(format!("bridge_axiom_application_{}", self.axiom_id)),
                source_location: Some(self.source_page.clone()),
                source_quote: self.source_quote.clone(),
                confidence: Some(self.confidence),
                ..TrackEntityOptions::default()
            };
            // Graceful failure: the result is returned whether or not tracking succeeded.
            let _ = manager.track_entity(&output_entity, &self.source_doi, Some(metadata), options);
        }

        let mut result = Map::new();
        result.insert("axiom_id".into(), Value::String(self.axiom_id.clone()));
        result.insert("axiom_name".into(), Value::String(self.name.clone()));
        result.insert("input_entity".into(), input_entity);
        result.insert("input_value".into(), input_value);
        result.insert("output_entity".into(), Value::String(output_entity));
        result.insert("output_value".into(), output_value);
        result.insert("coefficient".into(), f64_value(self.coefficient));
        result.insert("confidence".into(), f64_value(self.confidence));
        result.insert("source_doi".into(), Value::String(self.source_doi.clone()));
        result.insert(
            "source_page".into(),
            Value::String(self.source_page.clone()),
        );
        result.insert(
            "input_domain".into(),
            Value::String(self.input_domain.clone()),
        );
        result.insert(
            "output_domain".into(),
            Value::String(self.output_domain.clone()),
        );
        Ok(result)
    }

    /// `to_dict()`.
    pub fn to_dict(&self) -> Map<String, Value> {
        let mut d = Map::new();
        d.insert("axiom_id".into(), Value::String(self.axiom_id.clone()));
        d.insert("name".into(), Value::String(self.name.clone()));
        d.insert("rule".into(), Value::String(self.rule.clone()));
        d.insert("coefficient".into(), f64_value(self.coefficient));
        d.insert("source_doi".into(), Value::String(self.source_doi.clone()));
        d.insert(
            "source_page".into(),
            Value::String(self.source_page.clone()),
        );
        d.insert(
            "source_quote".into(),
            self.source_quote
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
        );
        d.insert("confidence".into(), f64_value(self.confidence));
        d.insert(
            "input_domain".into(),
            Value::String(self.input_domain.clone()),
        );
        d.insert(
            "output_domain".into(),
            Value::String(self.output_domain.clone()),
        );
        d.insert("metadata".into(), Value::Object(self.metadata.clone()));
        d
    }
}

impl fmt::Debug for BridgeAxiom {
    /// The dataclass `repr()`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<String> = self
            .to_dict()
            .iter()
            .map(|(key, value)| match (key.as_str(), value) {
                ("coefficient", _) => format!("coefficient={}", float_repr(self.coefficient)),
                ("confidence", _) => format!("confidence={}", float_repr(self.confidence)),
                (k, v) => format!("{k}={}", py_repr(v)),
            })
            .collect();
        write!(f, "BridgeAxiom({})", parts.join(", "))
    }
}

/// Multi-layer translation chain with complete provenance.
#[derive(Clone, PartialEq)]
pub struct TranslationChain {
    /// Unique chain identifier.
    pub chain_id: String,
    /// The layers (`{"layer", "type", "value", "source", "timestamp", ...}` dicts).
    pub layers: Vec<Map<String, Value>>,
    /// Overall confidence (default `1.0`).
    pub confidence: f64,
    /// Additional metadata.
    pub metadata: Map<String, Value>,
}

impl TranslationChain {
    /// `TranslationChain(chain_id)` with no layers and confidence `1.0`.
    pub fn new(chain_id: impl Into<String>) -> Self {
        TranslationChain {
            chain_id: chain_id.into(),
            layers: Vec::new(),
            confidence: 1.0,
            metadata: Map::new(),
        }
    }

    /// Add a layer `{"layer", "type", "value", "source", "timestamp": utcnow, **kwargs}`
    /// (`kwargs` may override the earlier keys). `source` is `Value::Null` for none.
    pub fn add_layer(
        &mut self,
        layer_name: &str,
        layer_type: &str,
        value: impl Into<Value>,
        source: impl Into<Value>,
        kwargs: Map<String, Value>,
    ) {
        let mut layer = Map::new();
        layer.insert("layer".into(), Value::String(layer_name.to_string()));
        layer.insert("type".into(), Value::String(layer_type.to_string()));
        layer.insert("value".into(), value.into());
        layer.insert("source".into(), source.into());
        layer.insert("timestamp".into(), Value::String(clock::utcnow_isoformat()));
        for (key, value) in kwargs {
            layer.insert(key, value);
        }
        self.layers.push(layer);
    }

    /// The first layer named `layer_name`.
    pub fn get_layer(&self, layer_name: &str) -> Option<&Map<String, Value>> {
        self.layers
            .iter()
            .find(|layer| layer.get("layer").and_then(Value::as_str) == Some(layer_name))
    }

    /// The first layer named `layer_name`, mutably (the original returns the live dict).
    pub fn get_layer_mut(&mut self, layer_name: &str) -> Option<&mut Map<String, Value>> {
        self.layers
            .iter_mut()
            .find(|layer| layer.get("layer").and_then(Value::as_str) == Some(layer_name))
    }

    /// `to_dict()`.
    pub fn to_dict(&self) -> Map<String, Value> {
        let mut d = Map::new();
        d.insert("chain_id".into(), Value::String(self.chain_id.clone()));
        d.insert(
            "layers".into(),
            Value::Array(self.layers.iter().cloned().map(Value::Object).collect()),
        );
        d.insert("confidence".into(), f64_value(self.confidence));
        d.insert("metadata".into(), Value::Object(self.metadata.clone()));
        d
    }
}

impl fmt::Debug for TranslationChain {
    /// The dataclass `repr()`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let layers: Vec<String> = self.layers.iter().map(map_repr).collect();
        write!(
            f,
            "TranslationChain(chain_id={}, layers=[{}], confidence={}, metadata={})",
            str_repr(&self.chain_id),
            layers.join(", "),
            float_repr(self.confidence),
            map_repr(&self.metadata)
        )
    }
}

fn single(key: &str, value: Value) -> Map<String, Value> {
    Map::from_iter([(key.to_string(), value)])
}

/// Build a translation chain by applying `bridge_axioms` in sequence to
/// `input_data = {"entity_id", "value", "source"}`: layer `L1` is the input, `L{i+2}_axiom`
/// records each axiom, `L{n+2}` the output; the chain confidence is the minimum axiom
/// confidence. Each application is tracked when a manager is given. A missing or non-numeric
/// `value` raises Python's `TypeError`.
pub fn create_translation_chain(
    input_data: &Map<String, Value>,
    bridge_axioms: &[BridgeAxiom],
    prov_manager: Option<&ProvenanceManager>,
) -> Result<TranslationChain, SemanticaError> {
    let get = |key: &str| input_data.get(key).cloned().unwrap_or(Value::Null);
    let mut chain = TranslationChain::new(uuid::Uuid::new_v4().to_string());
    chain.add_layer(
        "L1",
        "input",
        get("value"),
        get("source"),
        single("entity_id", get("entity_id")),
    );
    let mut current_value = get("value");
    let mut current_entity = get("entity_id");
    for (i, axiom) in bridge_axioms.iter().enumerate() {
        let result = axiom.apply(
            current_entity.clone(),
            current_value.clone(),
            prov_manager,
            &Map::new(),
        )?;
        let mut extra = Map::new();
        extra.insert("axiom_id".into(), Value::String(axiom.axiom_id.clone()));
        extra.insert("axiom_name".into(), Value::String(axiom.name.clone()));
        extra.insert("rule".into(), Value::String(axiom.rule.clone()));
        chain.add_layer(
            &format!("L{}_axiom", i + 2),
            "bridge_axiom",
            f64_value(axiom.coefficient),
            Value::String(axiom.source_doi.clone()),
            extra,
        );
        current_value = result["output_value"].clone();
        current_entity = result["output_entity"].clone();
        if axiom.confidence < chain.confidence {
            chain.confidence = axiom.confidence;
        }
    }
    chain.add_layer(
        &format!("L{}", bridge_axioms.len() + 2),
        "output",
        current_value,
        Value::Null,
        single("entity_id", current_entity),
    );
    Ok(chain)
}

/// Trace a chain's provenance: per layer `{layer, type, value, source}` plus, for layers with
/// an `entity_id`, its `get_lineage` result under `provenance` (lineage errors are ignored).
pub fn trace_translation_chain(
    chain: &TranslationChain,
    prov_manager: &ProvenanceManager,
) -> Map<String, Value> {
    let mut layers = Vec::new();
    for layer in &chain.layers {
        let field = |key: &str| layer.get(key).cloned().unwrap_or(Value::Null);
        let mut layer_trace = Map::new();
        layer_trace.insert("layer".into(), field("layer"));
        layer_trace.insert("type".into(), field("type"));
        layer_trace.insert("value".into(), field("value"));
        layer_trace.insert("source".into(), field("source"));
        if let Some(entity_id) = layer.get("entity_id").filter(|v| truthy(v)) {
            let lineage = match entity_id {
                Value::String(id) => prov_manager.get_lineage(id).ok(),
                // A non-string id matches no tracked (string) entity.
                _ => Some(Map::new()),
            };
            if let Some(lineage) = lineage {
                layer_trace.insert("provenance".into(), Value::Object(lineage));
            }
        }
        layers.push(Value::Object(layer_trace));
    }
    let mut trace = Map::new();
    trace.insert("chain_id".into(), Value::String(chain.chain_id.clone()));
    trace.insert("layers".into(), Value::Array(layers));
    trace.insert("confidence".into(), f64_value(chain.confidence));
    trace.insert("provenance".into(), Value::Array(Vec::new()));
    trace
}
