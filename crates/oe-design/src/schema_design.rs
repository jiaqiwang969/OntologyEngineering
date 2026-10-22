//! Design declarations for semantic model behavior. Specs are the sole owner;
//! actual model validation remains in the neutral Rust capability library.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSpec {
    pub id: String,
    pub name: String,
    pub rust_name: String,
    pub meaning: String,
    pub source: String,
    pub source_sha256: String,
    pub class_sha256: String,
    pub profile: String,
    pub schema_json: String,
    pub config_json: String,
    pub validators: Vec<String>,
    pub aliases_json: String,
    pub properties_json: String,
    pub fields: Vec<SemanticModelFieldSpec>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelFieldSpec {
    pub name: String,
    pub rust_name: String,
    pub meaning: String,
    pub annotation: String,
    pub rust_type: String,
    pub required: bool,
    pub alias: Option<String>,
    pub default_json: String,
    pub source_line: u64,
    pub validators: Vec<String>,
}
pub fn validate(models: &[SemanticModelSpec]) -> Result<()> {
    let mut ids = BTreeSet::new();
    let rust_names = models
        .iter()
        .map(|m| m.rust_name.as_str())
        .collect::<BTreeSet<_>>();
    ensure!(
        rust_names.len() == models.len(),
        "semantic_model_rust_name_duplicate"
    );
    for model in models {
        ensure!(
            super::identifier(&model.rust_name),
            "semantic_model_rust_name_invalid"
        );
        ensure!(
            ids.insert(&model.id),
            "semantic_model_duplicate:{}",
            model.id
        );
        super::safe_relative(&model.source)?;
        ensure!(
            model.source.ends_with(".py")
                && model.id
                    == format!(
                        "{}::{}",
                        model.source.trim_end_matches(".py").replace('/', "."),
                        model.name
                    )
                && model
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || [b'_', b'.', b':'].contains(&b))
                && super::identifier(&model.name),
            "semantic_model_source_identity_invalid"
        );
        ensure!(
            !model.id.is_empty()
                && !model.name.is_empty()
                && !model.meaning.is_empty()
                && !model.meaning.contains(['\n', '\r']),
            "semantic_model_identity_missing"
        );
        for sha in [&model.source_sha256, &model.class_sha256] {
            ensure!(
                sha.len() == 64
                    && sha
                        .bytes()
                        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
                "semantic_model_source_sha_invalid"
            );
        }
        ensure!(
            model.profile == "semantica.pydantic-json/1",
            "semantic_model_profile_unsupported"
        );
        let schema: Value = oe_core::json::strict_json(model.schema_json.as_bytes())?;
        let config: Value = oe_core::json::strict_json(model.config_json.as_bytes())?;
        let properties: Value = oe_core::json::strict_json(model.properties_json.as_bytes())?;
        let aliases: Value = oe_core::json::strict_json(model.aliases_json.as_bytes())?;
        let root = if let Some(reference) = schema["$ref"].as_str() {
            let pointer = reference
                .strip_prefix('#')
                .ok_or_else(|| anyhow::anyhow!("semantic_model_external_ref_unsupported"))?;
            schema
                .pointer(pointer)
                .ok_or_else(|| anyhow::anyhow!("semantic_model_root_ref_unknown"))?
        } else {
            &schema
        };
        ensure!(
            root["type"] == "object"
                && config.is_object()
                && properties.is_object()
                && aliases.is_object(),
            "semantic_model_schema_invalid"
        );
        let mut fields = BTreeSet::new();
        let mut rust_fields = BTreeSet::new();
        for field in &model.fields {
            ensure!(
                super::identifier(&field.rust_name) && rust_fields.insert(&field.rust_name),
                "semantic_model_rust_field_invalid"
            );
            check_rust_type(&field.rust_type, &rust_names)?;
            // Original model keys are data names, including Python's "type";
            // they are not interpolated as Rust field identifiers.
            ensure!(
                !field.name.is_empty()
                    && field.name.bytes().enumerate().all(|(i, b)| b == b'_'
                        || b.is_ascii_alphabetic()
                        || (i > 0 && b.is_ascii_digit()))
                    && fields.insert(&field.name),
                "semantic_model_field_invalid"
            );
            ensure!(
                !field.meaning.is_empty()
                    && !field.meaning.contains(['\n', '\r'])
                    && !field.annotation.is_empty()
                    && field.source_line > 0,
                "semantic_model_field_semantics_missing"
            );
            let default: Value = oe_core::json::strict_json(field.default_json.as_bytes())?;
            ensure!(
                default["kind"] == "required" || default["kind"] == "value",
                "semantic_model_default_kind_invalid"
            );
            ensure!(
                (default["kind"] == "required") == field.required,
                "semantic_model_default_required_mismatch"
            );
            let key = field.alias.as_deref().unwrap_or(&field.name);
            ensure!(
                root["properties"].get(key).is_some(),
                "semantic_model_shape_missing"
            );
        }
        ensure!(
            model.fields.len() == root["properties"].as_object().map_or(0, |m| m.len()),
            "semantic_model_fields_shape_disagree"
        );
    }
    Ok(())
}
/// JSON declaration only. Literal escaping produces RDF data, never code
/// rendering. Locked ggen renders the catalog consumed by contracts/runtime.
pub fn catalog(models: &[SemanticModelSpec]) -> Result<Value> {
    Ok(Value::Array(models.iter().map(|m| {
        Ok(serde_json::json!({
            "id":m.id,"name":m.name,"meaning":m.meaning,"source":m.source,
            "source_sha256":m.source_sha256,"class_sha256":m.class_sha256,"profile":m.profile,
            "json_schema":oe_core::json::strict_json(m.schema_json.as_bytes())?,
            "config":oe_core::json::strict_json(m.config_json.as_bytes())?,
            "validators":m.validators,
            "aliases":oe_core::json::strict_json(m.aliases_json.as_bytes())?,
            "properties":oe_core::json::strict_json(m.properties_json.as_bytes())?,
            "fields":m.fields.iter().map(|f| Ok(serde_json::json!({
                "name":f.name,"meaning":f.meaning,"annotation":f.annotation,"required":f.required,
                "alias":f.alias,"default":oe_core::json::strict_json(f.default_json.as_bytes())?,
                "source_line":f.source_line,"validators":f.validators
            }))).collect::<Result<Vec<Value>>>()?
        }))
    }).collect::<Result<Vec<Value>>>()?))
}

fn check_rust_type(text: &str, names: &BTreeSet<&str>) -> Result<()> {
    struct Guard<'a> {
        names: &'a BTreeSet<&'a str>,
        valid: bool,
    }
    impl<'ast> syn::visit::Visit<'ast> for Guard<'_> {
        fn visit_type_path(&mut self, path: &'ast syn::TypePath) {
            let spelling = path
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");
            if path.qself.is_some()
                || !(self.names.contains(spelling.as_str())
                    || [
                        "String",
                        "bool",
                        "f64",
                        "Vec",
                        "Option",
                        "serde_json::Value",
                        "serde_json::Number",
                        "crate::json::OrderedObject",
                    ]
                    .contains(&spelling.as_str()))
            {
                self.valid = false;
            }
            syn::visit::visit_type_path(self, path);
        }
        fn visit_type(&mut self, ty: &'ast syn::Type) {
            if !matches!(ty, syn::Type::Path(_) | syn::Type::Tuple(_)) {
                self.valid = false;
            }
            syn::visit::visit_type(self, ty);
        }
    }
    let ty = syn::parse_str::<syn::Type>(text)?;
    let mut guard = Guard { names, valid: true };
    syn::visit::Visit::visit_type(&mut guard, &ty);
    ensure!(guard.valid, "semantic_model_rust_type_unowned");
    Ok(())
}
