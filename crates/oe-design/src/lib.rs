//! Annotated design is the only editable model. No code rendering lives here.
pub mod composition;
pub mod generation;
pub mod inspection;
pub mod migration;
pub mod migration_ledger;
pub mod provenance;
pub mod schema_design;

use anyhow::{Context, Result, ensure};
use oe_wire::json::{canonical_json, strict_json};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub const NS: &str = "urn:oev2:design:";
const MIGRATION_MAPPING_PREFIX: &str =
    "@prefix migration: <urn:oev2:design:semantic-migration-mapping:> .\n";

/// One deterministic project model from the editable design and derived source facts.
pub fn model(root: &Path, design: &Design) -> Result<(String, BTreeMap<String, String>)> {
    let (source, mut inputs) = inspection::rdf(root, design)?;
    let mut model = design.rdf()?;
    model.push_str(&source);
    // A design that undertakes the complete Semantica migration must retain its
    // exact original scope during normalization and actual ggen publication.
    if design
        .sources
        .iter()
        .any(|s| s.id == "semantica_oe7_complete")
    {
        let audit = migration::audit(root)?;
        for path in [
            "runtime/semantica-scope.json",
            "runtime/semantica-scope-lock.json",
        ] {
            let bytes = oe_core::read_regular_bytes(&root.join(path), 16 * 1024 * 1024)?;
            let hash = oe_core::sha256(&bytes);
            inputs.insert(path.into(), hash.clone());
            migration_asset(&mut model, path, &hash)?;
        }
        model.push_str(&format!(
            "<urn:oev2:design:semantic-migration-scope> <urn:oev2:design:scopeSha256> {} ; <urn:oev2:design:migrationStatus> \"incomplete\" ; <urn:oev2:design:behaviorStatus> \"unknown\" .\n",
            serde_json::to_string(audit["scope_sha256"].as_str().ok_or_else(|| anyhow::anyhow!("migration_scope_audit_digest_missing"))?)?,
        ));
        let mappings = "runtime/semantica-migration-mappings.json";
        if root.join(mappings).exists() {
            let bytes = oe_core::read_regular_bytes(&root.join(mappings), 8 * 1024 * 1024)?;
            let hash = oe_core::sha256(&bytes);
            inputs.insert(mappings.into(), hash.clone());
            migration_asset(&mut model, mappings, &hash)?;
        }
        model.push_str(MIGRATION_MAPPING_PREFIX);
        for mapping in audit["declared_mappings"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("migration_scope_mappings_missing"))?
        {
            let identity = oe_core::sha256(canonical_json(mapping)?);
            let subject = format!("migration:{identity}");
            for (property, value) in [
                ("module", &mapping["module"]),
                ("rustPath", &mapping["rust_path"]),
                ("mappingStatus", &mapping["status"]),
                ("behaviorStatus", &mapping["behavior_status"]),
            ] {
                model.push_str(&format!(
                    "{subject} <urn:oev2:design:{property}> {} .\n",
                    serde_json::to_string(
                        value
                            .as_str()
                            .ok_or_else(|| anyhow::anyhow!("migration_mapping_value_missing"))?
                    )?
                ));
            }
            for id in mapping["scope_ids"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("migration_mapping_scope_ids_missing"))?
            {
                model.push_str(&format!(
                    "{subject} <urn:oev2:design:originalScopeId> {} .\n",
                    serde_json::to_string(
                        id.as_str()
                            .ok_or_else(|| anyhow::anyhow!("migration_mapping_scope_id_missing"))?
                    )?
                ));
            }
        }
    }
    ensure!(
        model.len() as u64 <= oe_core::source_limits::DERIVED_MODEL_BYTES,
        "derived_design_model_byte_limit"
    );
    Ok((model, inputs))
}

fn migration_asset(model: &mut String, path: &str, hash: &str) -> Result<()> {
    model.push_str(&format!(
        "<urn:oev2:design:migration-asset:{hash}> <urn:oev2:design:kind> \"SemanticaMigrationAsset\" ; <urn:oev2:design:path> {} ; <urn:oev2:design:sha256> {} .\n",
        serde_json::to_string(path)?, serde_json::to_string(hash)?
    ));
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Design {
    pub schema: String,
    pub version: String,
    pub problem: String,
    pub purpose: String,
    pub requirements: Vec<Requirement>,
    pub modules: Vec<Module>,
    pub types: Vec<Type>,
    #[serde(default)]
    pub constraints: Vec<Constraint>,
    #[serde(default)]
    pub machines: Vec<Machine>,
    #[serde(default)]
    pub interfaces: Vec<Interface>,
    #[serde(default)]
    pub validations: Vec<Validation>,
    #[serde(default)]
    pub sources: Vec<Source>,
    #[serde(default)]
    pub errors: Vec<ErrorDefinition>,
    #[serde(default)]
    pub contract_cases: Vec<ContractCase>,
    #[serde(default)]
    pub semantic_models: Vec<schema_design::SemanticModelSpec>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    pub id: String,
    pub function: String,
    pub condition: String,
    pub acceptance: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Module {
    pub id: String,
    #[serde(default = "engineering_contract_group")]
    pub contract_group: String,
    pub kind: String,
    pub rust_path: String,
    pub purpose: String,
    pub realizes: Vec<String>,
    pub platform: String,
    pub effects: String,
    pub failures: String,
    pub status: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub source_refs: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Type {
    pub name: String,
    #[serde(default = "engineering_contract_group")]
    pub contract_group: String,
    pub meaning: String,
    pub identity: String,
    pub fields: Vec<Field>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub name: String,
    pub meaning: String,
    pub rust_type: String,
    pub required: bool,
    pub format: String,
    pub unit: String,
    pub precision: String,
    pub write: String,
    pub applicability: String,
    pub acceptance: String,
    #[serde(default)]
    pub allowed_values: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorDefinition {
    pub name: String,
    pub code: String,
    pub meaning: String,
    pub phase: String,
    pub retryable: bool,
    pub acceptance: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractCase {
    pub id: String,
    pub target_type: String,
    pub input_json: String,
    pub expected_valid: bool,
    pub oracle: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Constraint {
    pub id: String,
    pub requirement: String,
    pub module: String,
    pub check: String,
    pub rust_path: String,
    pub statement: String,
    pub severity: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Machine {
    pub name: String,
    pub module: String,
    pub states: Vec<String>,
    pub initial: String,
    pub transitions: Vec<Transition>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transition {
    pub from: String,
    pub event: String,
    pub to: String,
    pub guard: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Interface {
    pub name: String,
    pub module: String,
    pub method: String,
    pub input: String,
    pub output: String,
    pub error: String,
    pub effects: String,
    #[serde(default)]
    pub rust_path: Option<String>,
    #[serde(default)]
    pub implementation: Option<String>,
}

fn engineering_contract_group() -> String {
    "engineering".into()
}

fn referenced_contract<'a>(raw: &'a str, names: &BTreeMap<&str, &str>) -> Option<&'a str> {
    if raw == "std::collections::BTreeMap<String, ArtifactReference>" {
        return names
            .contains_key("ArtifactReference")
            .then_some("ArtifactReference");
    }
    let inner = raw
        .strip_prefix("Option<")
        .or_else(|| raw.strip_prefix("Vec<"))
        .and_then(|s| s.strip_suffix('>'));
    if let Some(inner) = inner {
        referenced_contract(inner, names)
    } else {
        names.contains_key(raw).then_some(raw)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Validation {
    pub id: String,
    pub module: String,
    pub behavior: String,
    pub constraint: String,
    pub rust_path: String,
    pub oracle: String,
    pub scope: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub id: String,
    pub identity: String,
    pub role: String,
    pub applicability: String,
}

fn meaningful(s: &str) -> bool {
    !s.trim().is_empty() && !s.contains('\0')
}
pub fn identifier(s: &str) -> bool {
    s != "_"
        && !s.is_empty()
        && s.bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        && ![
            "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn",
            "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
            "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
            "unsafe", "use", "where", "while", "async", "await", "dyn", "abstract", "become",
            "box", "do", "final", "macro", "override", "priv", "typeof", "unsized", "virtual",
            "yield", "try", "gen",
        ]
        .contains(&s)
}
fn unique<'a>(what: &str, values: impl Iterator<Item = &'a str>) -> Result<BTreeSet<String>> {
    let mut set = BTreeSet::new();
    for value in values {
        ensure!(
            identifier(value) && set.insert(value.to_owned()),
            "duplicate_or_invalid_{what}:{value}"
        );
    }
    Ok(set)
}
pub fn safe_relative(s: &str) -> Result<()> {
    ensure!(
        !s.is_empty()
            && !s.starts_with('/')
            && !s.contains(['\\', ':', '\0'])
            && s.split('/').all(|p| !p.is_empty() && p != "." && p != ".."),
        "design_unsafe_path"
    );
    Ok(())
}
/// Finite Rust type grammar used by the generated Value identity hook.
/// Named contracts are handled by their own generated fields, never by a name prefix.
pub fn json_value_type(s: &str) -> bool {
    if s.len() > 256 {
        return false;
    }
    if let Some(inner) = s.strip_prefix("Vec<").and_then(|s| s.strip_suffix('>')) {
        return json_value_type(inner);
    }
    if let Some(inner) = s.strip_prefix("Option<").and_then(|s| s.strip_suffix('>')) {
        return json_value_type(inner);
    }
    s == "serde_json::Value"
}
fn rust_type(s: &str, names: &BTreeSet<String>) -> bool {
    if s.len() > 256 {
        return false;
    }
    // Map contracts currently have direct-field generated validators. Refuse
    // wrappers until the corresponding recursive validation is implemented.
    let wrapped = s.strip_prefix("Vec<").or_else(|| s.strip_prefix("Option<"));
    if let Some(t) = wrapped.and_then(|s| s.strip_suffix('>')) {
        if t.contains("std::collections::BTreeMap<String, Vec<String>>")
            || t.contains("std::collections::BTreeMap<String, ArtifactReference>")
        {
            return false;
        }
        return rust_type(t, names);
    }
    // Native byte transport remains one explicit neutral map shape, rather
    // than admitting arbitrary Rust syntax into generated contracts.
    if s == "std::collections::BTreeMap<String, Vec<u8>>"
        || s == "std::collections::BTreeMap<String, String>"
        || s == "std::collections::BTreeMap<String, u64>"
    {
        return true;
    }
    if s == "std::collections::BTreeMap<String, Vec<String>>" {
        return true;
    }
    if s == "std::collections::BTreeMap<String, ArtifactReference>" {
        return names.contains("ArtifactReference");
    }
    [
        "String",
        "bool",
        "u8",
        "u64",
        "i64",
        "f64",
        "serde_json::Value",
    ]
    .contains(&s)
        || names.contains(s)
}
impl Design {
    /// Contract layers are declared, never inferred from a type-name prefix.
    pub fn validate_contract_groups(&self) -> Result<()> {
        let mut groups: BTreeMap<&str, &str> = self
            .types
            .iter()
            .map(|t| (t.name.as_str(), t.contract_group.as_str()))
            .collect();
        if !self.errors.is_empty() {
            groups.insert("ErrorCode", "engineering");
        }
        for t in &self.types {
            ensure!(
                ["engineering", "semantic"].contains(&t.contract_group.as_str()),
                "contract_group_unsupported:{}",
                t.name
            );
            for f in &t.fields {
                ensure!(
                    f.format != "raw_utf8" || f.rust_type == "String",
                    "raw_utf8_contract_requires_string"
                );
                if let Some(target) = referenced_contract(&f.rust_type, &groups) {
                    ensure!(
                        t.contract_group != "semantic" || groups[target] == "semantic",
                        "semantic_contract_depends_on_engineering:{}.{}",
                        t.name,
                        f.name
                    );
                }
            }
        }
        for i in &self.interfaces {
            self.interface_group(i)?;
        }
        fn visit<'a>(
            id: &'a str,
            modules: &BTreeMap<&'a str, &'a Module>,
            open: &mut BTreeSet<&'a str>,
            done: &mut BTreeSet<&'a str>,
        ) -> Result<()> {
            if done.contains(id) {
                return Ok(());
            }
            ensure!(open.insert(id), "module_dependency_cycle:{id}");
            for dependency in &modules[id].depends_on {
                ensure!(
                    modules.contains_key(dependency.as_str()),
                    "module_dependency_unbound:{id}"
                );
                visit(dependency, modules, open, done)?;
            }
            open.remove(id);
            done.insert(id);
            Ok(())
        }
        let modules: BTreeMap<_, _> = self.modules.iter().map(|m| (m.id.as_str(), m)).collect();
        let mut done = BTreeSet::new();
        for id in modules.keys() {
            visit(id, &modules, &mut BTreeSet::new(), &mut done)?;
        }
        for m in &self.modules {
            ensure!(
                ["engineering", "semantic"].contains(&m.contract_group.as_str()),
                "module_contract_group_unsupported:{}",
                m.id
            );
            if m.contract_group == "semantic" {
                ensure!(
                    m.depends_on
                        .iter()
                        .all(|d| modules[d.as_str()].contract_group == "semantic"),
                    "semantic_module_depends_on_engineering:{}",
                    m.id
                );
            }
        }
        Ok(())
    }
    pub fn interface_group(&self, i: &Interface) -> Result<String> {
        let mut groups: BTreeMap<&str, &str> = self
            .types
            .iter()
            .map(|t| (t.name.as_str(), t.contract_group.as_str()))
            .collect();
        if !self.errors.is_empty() {
            groups.insert("ErrorCode", "engineering");
        }
        let referenced: BTreeSet<_> = [&i.input, &i.output, &i.error]
            .into_iter()
            .filter_map(|raw| referenced_contract(raw, &groups).map(|name| groups[name]))
            .collect();
        ensure!(
            referenced.len() <= 1,
            "interface_contract_group_mixed:{}",
            i.name
        );
        Ok(referenced
            .into_iter()
            .next()
            .unwrap_or("engineering")
            .into())
    }
    pub fn validate(&self) -> Result<()> {
        schema_design::validate(&self.semantic_models)?;
        ensure!(self.schema == "oev2.design/1", "design_schema_unsupported");
        ensure!(
            [&self.version, &self.problem, &self.purpose]
                .into_iter()
                .all(|s| meaningful(s)),
            "design_problem_or_purpose_missing"
        );
        ensure!(
            !self.requirements.is_empty() && !self.modules.is_empty(),
            "design_empty_scope"
        );
        let reqs = unique(
            "requirement",
            self.requirements.iter().map(|r| r.id.as_str()),
        )?;
        let modules = unique("module", self.modules.iter().map(|m| m.id.as_str()))?;
        let mut types = unique("type", self.types.iter().map(|t| t.name.as_str()))?;
        if !self.errors.is_empty() {
            ensure!(
                types.insert("ErrorCode".into()),
                "reserved_error_type_collision"
            );
        }
        let sources = unique("source", self.sources.iter().map(|s| s.id.as_str()))?;
        let constraints = unique("constraint", self.constraints.iter().map(|c| c.id.as_str()))?;
        let mut entities = BTreeSet::new();
        for id in std::iter::once("model".to_owned())
            .chain(
                reqs.iter()
                    .chain(modules.iter())
                    .chain(sources.iter())
                    .chain(constraints.iter())
                    .cloned(),
            )
            .chain(self.types.iter().map(|t| format!("type_{}", t.name)))
            .chain(self.types.iter().flat_map(|t| {
                t.fields
                    .iter()
                    .map(move |f| format!("field_{}_{}", t.name, f.name))
            }))
            .chain(self.machines.iter().map(|m| format!("machine_{}", m.name)))
            .chain(
                self.interfaces
                    .iter()
                    .map(|i| format!("interface_{}", i.name)),
            )
            .chain(self.errors.iter().map(|e| format!("error_{}", e.name)))
            .chain(self.contract_cases.iter().map(|c| format!("case_{}", c.id)))
            .chain(self.machines.iter().flat_map(|m| {
                m.transitions
                    .iter()
                    .enumerate()
                    .map(move |(n, _)| format!("transition_{}_{}", m.name, n))
            }))
            .chain(self.validations.iter().map(|v| v.id.clone()))
        {
            ensure!(entities.insert(id.clone()), "rdf_identity_collision:{id}");
        }
        for r in &self.requirements {
            ensure!(
                [&r.function, &r.condition, &r.acceptance]
                    .into_iter()
                    .all(|s| meaningful(s)),
                "requirement_annotations_missing:{}",
                r.id
            );
        }
        for m in &self.modules {
            safe_relative(&m.rust_path)?;
            ensure!(
                [
                    "calculation",
                    "state_machine",
                    "pipeline",
                    "adapter",
                    "contract",
                    "governance"
                ]
                .contains(&m.kind.as_str()),
                "module_kind_unsupported:{}",
                m.id
            );
            ensure!(
                !m.realizes.is_empty() && m.realizes.iter().all(|r| reqs.contains(r)),
                "module_function_unbound:{}",
                m.id
            );
            ensure!(
                m.depends_on
                    .iter()
                    .all(|r| modules.contains(r) && r != &m.id),
                "module_dependency_unbound:{}",
                m.id
            );
            ensure!(
                m.source_refs.iter().all(|r| sources.contains(r)),
                "module_source_unbound:{}",
                m.id
            );
            ensure!(
                [&m.purpose, &m.platform, &m.effects, &m.failures]
                    .into_iter()
                    .all(|s| meaningful(s)),
                "module_annotations_missing:{}",
                m.id
            );
            ensure!(
                [
                    "source_only",
                    "behavior_verified",
                    "native_pending",
                    "retired"
                ]
                .contains(&m.status.as_str()),
                "module_status_unsupported"
            );
        }
        for t in &self.types {
            ensure!(
                identifier(&t.name)
                    && meaningful(&t.meaning)
                    && !t.meaning.contains(['\n', '\r'])
                    && meaningful(&t.identity)
                    && !t.fields.is_empty(),
                "type_annotations_invalid:{}",
                t.name
            );
            unique("field", t.fields.iter().map(|f| f.name.as_str()))?;
            for f in &t.fields {
                ensure!(
                    identifier(&f.name) && rust_type(&f.rust_type, &types),
                    "field_rust_mapping_invalid:{}.{}",
                    t.name,
                    f.name
                );
                ensure!(
                    [
                        &f.meaning,
                        &f.format,
                        &f.unit,
                        &f.precision,
                        &f.write,
                        &f.applicability,
                        &f.acceptance
                    ]
                    .into_iter()
                    .all(|s| meaningful(s)),
                    "field_annotations_missing:{}.{}",
                    t.name,
                    f.name
                );
                ensure!(
                    f.required != f.rust_type.starts_with("Option<"),
                    "field_required_mapping_conflict:{}.{}",
                    t.name,
                    f.name
                );
                ensure!(
                    !f.meaning.contains(['\n', '\r']) && !f.acceptance.contains(['\n', '\r']),
                    "field_annotation_multiline"
                );
                ensure!(
                    f.format != "status"
                        || (!f.allowed_values.is_empty()
                            && matches!(f.rust_type.as_str(), "String" | "Option<String>")),
                    "status_values_missing"
                );
                ensure!(
                    f.format != "sha256"
                        || matches!(f.rust_type.as_str(), "String" | "Option<String>"),
                    "sha256_mapping_invalid"
                );
                ensure!(
                    f.allowed_values.iter().all(|v| meaningful(v))
                        && f.allowed_values.iter().collect::<BTreeSet<_>>().len()
                            == f.allowed_values.len(),
                    "field_allowed_values_invalid"
                );
                ensure!(
                    f.allowed_values.is_empty()
                        || matches!(f.rust_type.as_str(), "String" | "Option<String>"),
                    "field_allowed_values_mapping_invalid"
                );
            }
        }
        for c in &self.constraints {
            ensure!(
                reqs.contains(&c.requirement) && modules.contains(&c.module),
                "constraint_binding_missing:{}",
                c.id
            );
            safe_relative(&c.rust_path)?;
            let module = self
                .modules
                .iter()
                .find(|m| m.id == c.module)
                .expect("validated module");
            ensure!(
                c.rust_path
                    .starts_with(&format!("{}/src/", module.rust_path)),
                "constraint_checker_path_outside_production_module:{}",
                c.id
            );
            ensure!(
                identifier(&c.check)
                    && meaningful(&c.statement)
                    && ["error", "unknown", "warning"].contains(&c.severity.as_str()),
                "constraint_check_invalid:{}",
                c.id
            );
        }
        for v in &self.validations {
            safe_relative(&v.rust_path)?;
            ensure!(
                modules.contains(&v.module)
                    && constraints.contains(&v.constraint)
                    && identifier(&v.behavior)
                    && meaningful(&v.oracle)
                    && meaningful(&v.scope),
                "validation_binding_invalid:{}",
                v.id
            );
        }
        unique("validation", self.validations.iter().map(|v| v.id.as_str()))?;
        unique("interface", self.interfaces.iter().map(|v| v.name.as_str()))?;
        for i in &self.interfaces {
            ensure!(
                identifier(&i.name)
                    && identifier(&i.method)
                    && modules.contains(&i.module)
                    && rust_type(&i.input, &types)
                    && rust_type(&i.output, &types)
                    && types.contains(&i.error)
                    && meaningful(&i.effects),
                "interface_invalid:{}",
                i.name
            );
            ensure!(
                i.rust_path.is_some() == i.implementation.is_some(),
                "interface_implementation_mapping_incomplete:{}",
                i.name
            );
            if let (Some(path), Some(implementation)) = (&i.rust_path, &i.implementation) {
                safe_relative(path)?;
                let owner = self
                    .modules
                    .iter()
                    .find(|m| m.id == i.module)
                    .expect("validated owner");
                ensure!(
                    path.starts_with(&format!("{}/src/", owner.rust_path))
                        && identifier(implementation),
                    "interface_implementation_owner_invalid:interface_implementation_outside_owner:{}",
                    i.name
                );
            }
        }
        unique("machine", self.machines.iter().map(|m| m.name.as_str()))?;
        for m in &self.machines {
            let states = unique("state", m.states.iter().map(String::as_str))?;
            ensure!(
                identifier(&m.name)
                    && modules.contains(&m.module)
                    && states.contains(&m.initial)
                    && m.states.iter().all(|s| identifier(s)),
                "machine_invalid:{}",
                m.name
            );
            let mut keys = BTreeSet::new();
            for t in &m.transitions {
                ensure!(
                    states.contains(&t.from)
                        && states.contains(&t.to)
                        && identifier(&t.event)
                        && meaningful(&t.guard)
                        && keys.insert((&t.from, &t.event)),
                    "transition_ambiguous_or_invalid:{}",
                    m.name
                );
            }
        }
        for s in &self.sources {
            ensure!(
                [&s.identity, &s.role, &s.applicability]
                    .into_iter()
                    .all(|s| meaningful(s)),
                "source_annotations_missing"
            );
        }
        unique("error_name", self.errors.iter().map(|e| e.name.as_str()))?;
        unique("error_code", self.errors.iter().map(|e| e.code.as_str()))?;
        for e in &self.errors {
            ensure!(
                [&e.meaning, &e.phase, &e.acceptance]
                    .into_iter()
                    .all(|s| meaningful(s) && !s.contains(['\n', '\r'])),
                "error_annotations_invalid"
            );
        }
        unique(
            "contract_case",
            self.contract_cases.iter().map(|c| c.id.as_str()),
        )?;
        for c in &self.contract_cases {
            ensure!(
                self.types.iter().any(|t| t.name == c.target_type)
                    && meaningful(&c.oracle)
                    && !c.oracle.contains(['\n', '\r']),
                "contract_case_type_or_oracle_invalid"
            );
            strict_json(c.input_json.as_bytes()).context("contract_case_json_invalid")?;
        }
        self.validate_contract_groups()?;
        Ok(())
    }
    pub fn normalize(mut self) -> Result<Self> {
        self.validate()?;
        self.requirements.sort_by(|a, b| a.id.cmp(&b.id));
        self.modules.sort_by(|a, b| a.id.cmp(&b.id));
        self.types.sort_by(|a, b| a.name.cmp(&b.name));
        self.constraints.sort_by(|a, b| a.id.cmp(&b.id));
        self.machines.sort_by(|a, b| a.name.cmp(&b.name));
        self.interfaces.sort_by(|a, b| a.name.cmp(&b.name));
        self.validations.sort_by(|a, b| a.id.cmp(&b.id));
        self.sources.sort_by(|a, b| a.id.cmp(&b.id));
        self.errors.sort_by(|a, b| a.code.cmp(&b.code));
        self.contract_cases.sort_by(|a, b| a.id.cmp(&b.id));
        for m in &mut self.modules {
            m.realizes.sort();
            m.depends_on.sort();
            m.source_refs.sort();
        }
        for t in &mut self.types {
            t.fields.sort_by(|a, b| a.name.cmp(&b.name));
            for f in &mut t.fields {
                f.allowed_values.sort();
            }
        }
        for m in &mut self.machines {
            m.states.sort();
            m.transitions
                .sort_by(|a, b| (&a.from, &a.event).cmp(&(&b.from, &b.event)));
        }
        Ok(self)
    }
    pub fn canonical(&self) -> Result<Vec<u8>> {
        Ok(canonical_json(&serde_json::to_value(
            self.clone().normalize()?,
        )?)?)
    }
    pub fn rdf(&self) -> Result<String> {
        self.clone().normalize()?.normalized_rdf()
    }
    fn normalized_rdf(&self) -> Result<String> {
        self.validate()?;
        let mut lines = BTreeSet::new();
        let mut triple = |s: &str, p: &str, o: String| {
            lines.insert(format!("<{NS}{s}> <{NS}{p}> {o} ."));
        };
        let lit = |s: &str| serde_json::to_string(s).expect("string JSON");
        let iri = |s: &str| format!("<{NS}{s}>");
        triple("model", "version", lit(&self.version));
        triple("model", "purpose", lit(&self.purpose));
        triple("model", "problem", lit(&self.problem));
        for r in &self.requirements {
            triple(&r.id, "kind", lit("Requirement"));
            triple(&r.id, "function", lit(&r.function));
            triple(&r.id, "condition", lit(&r.condition));
            triple(&r.id, "acceptance", lit(&r.acceptance));
        }
        for m in &self.modules {
            for (p, s) in [
                ("kind", "Module"),
                ("name", &m.id),
                ("contractGroup", &m.contract_group),
                ("moduleKind", &m.kind),
                ("rustPath", &m.rust_path),
                ("purpose", &m.purpose),
                ("platform", &m.platform),
                ("effects", &m.effects),
                ("failures", &m.failures),
                ("status", &m.status),
            ] {
                triple(&m.id, p, lit(s));
            }
            for r in &m.realizes {
                triple(&m.id, "realizes", iri(r));
            }
            for d in &m.depends_on {
                triple(&m.id, "dependsOn", iri(d));
            }
            for d in &m.source_refs {
                triple(&m.id, "source", iri(d));
            }
        }
        for t in &self.types {
            let id = format!("type_{}", t.name);
            triple(&id, "kind", lit("Type"));
            triple(&id, "name", lit(&t.name));
            triple(&id, "meaning", lit(&t.meaning));
            triple(&id, "identity", lit(&t.identity));
            triple(&id, "contractGroup", lit(&t.contract_group));
            for f in &t.fields {
                let fid = format!("field_{}_{}", t.name, f.name);
                triple(&id, "field", iri(&fid));
                triple(&fid, "owner", iri(&id));
                let groups: BTreeMap<_, _> = self
                    .types
                    .iter()
                    .map(|t| (t.name.as_str(), t.contract_group.as_str()))
                    .collect();
                if let Some(target) = referenced_contract(&f.rust_type, &groups) {
                    triple(&fid, "referencedType", iri(&format!("type_{target}")));
                }
                for (p, s) in [
                    ("kind", "Field"),
                    ("name", &f.name),
                    ("rustType", &f.rust_type),
                    ("meaning", &f.meaning),
                    ("format", &f.format),
                    ("unit", &f.unit),
                    ("precision", &f.precision),
                    ("write", &f.write),
                    ("applicability", &f.applicability),
                    ("acceptance", &f.acceptance),
                ] {
                    triple(&fid, p, lit(s));
                }
                triple(
                    &fid,
                    "jsonIdentity",
                    lit(if json_value_type(&f.rust_type) {
                        "preserved"
                    } else {
                        "ordinary"
                    }),
                );
                triple(
                    &fid,
                    "jsonOptional",
                    lit(if f.rust_type.starts_with("Option<") {
                        "true"
                    } else {
                        "false"
                    }),
                );
                triple(
                    &fid,
                    "required",
                    lit(if f.required { "true" } else { "false" }),
                );
                for value in &f.allowed_values {
                    triple(&fid, "allowedValueRust", lit(&format!("{value:?}")));
                }
            }
        }
        if !self.semantic_models.is_empty() {
            let catalog = schema_design::catalog(&self.semantic_models)?;
            let text = serde_json::to_string(&catalog)?;
            triple(
                "semantic_model_catalog",
                "kind",
                lit("SemanticModelCatalog"),
            );
            triple(
                "semantic_model_catalog",
                "catalogRustLiteral",
                lit(&format!("{text:?}")),
            );
        }
        for model in &self.semantic_models {
            let id = format!("semantic_model_{}", model.rust_name);
            for (p, s) in [
                ("kind", "SemanticSnapshot"),
                ("name", model.rust_name.as_str()),
                ("modelId", model.id.as_str()),
                ("meaning", model.meaning.as_str()),
                ("sourceSha", model.source_sha256.as_str()),
                ("classSha", model.class_sha256.as_str()),
            ] {
                triple(&id, p, lit(s));
            }
            for (index, field) in model.fields.iter().enumerate() {
                let fid = format!("{id}_field_{}", field.rust_name);
                triple(&fid, "snapshotOwner", iri(&id));
                for (p, s) in [
                    ("name", field.rust_name.clone()),
                    ("originalNameRust", format!("{:?}", field.name)),
                    ("meaning", field.meaning.clone()),
                    ("rustType", field.rust_type.clone()),
                    ("ordinal", format!("{index:05}")),
                    (
                        "jsonIdentity",
                        if field.rust_type.contains("serde_json::Value")
                            || field.rust_type.contains("serde_json::Number")
                        {
                            "preserved".into()
                        } else {
                            "ordinary".into()
                        },
                    ),
                    (
                        "jsonOptional",
                        if field.rust_type.starts_with("Option<") {
                            "true".into()
                        } else {
                            "false".into()
                        },
                    ),
                ] {
                    triple(&fid, p, lit(&s));
                }
            }
        }
        for c in &self.constraints {
            for (p, s) in [
                ("kind", "Constraint"),
                ("name", &c.id),
                ("check", &c.check),
                ("checkerPath", &c.rust_path),
                ("statement", &c.statement),
                ("severity", &c.severity),
            ] {
                triple(&c.id, p, lit(s));
            }
            triple(&c.id, "module", iri(&c.module));
            triple(&c.id, "requirement", iri(&c.requirement));
        }
        for v in &self.validations {
            for (p, s) in [
                ("kind", "Validation"),
                ("name", &v.id),
                ("behavior", &v.behavior),
                ("rustPath", &v.rust_path),
                ("oracle", &v.oracle),
                ("scope", &v.scope),
            ] {
                triple(&v.id, p, lit(s));
            }
            triple(&v.id, "module", iri(&v.module));
            triple(&v.id, "validates", iri(&v.constraint));
        }
        for m in &self.machines {
            let id = format!("machine_{}", m.name);
            triple(&id, "kind", lit("Machine"));
            triple(&id, "name", lit(&m.name));
            triple(&id, "initial", lit(&m.initial));
            triple(&id, "module", iri(&m.module));
            for s in &m.states {
                triple(&id, "state", lit(s));
            }
            for (n, t) in m.transitions.iter().enumerate() {
                let tid = format!("transition_{}_{}", m.name, n);
                triple(&id, "transition", iri(&tid));
                for (p, s) in [
                    ("from", &t.from),
                    ("event", &t.event),
                    ("to", &t.to),
                    ("guard", &t.guard),
                ] {
                    triple(&tid, p, lit(s));
                }
                triple(&tid, "guardRust", lit(&format!("{:?}", t.guard)));
            }
        }
        for i in &self.interfaces {
            let id = format!("interface_{}", i.name);
            for (p, s) in [
                ("kind", "Interface"),
                ("name", &i.name),
                ("method", &i.method),
                ("input", &i.input),
                ("output", &i.output),
                ("error", &i.error),
                ("effects", &i.effects),
            ] {
                triple(&id, p, lit(s));
            }
            triple(&id, "module", iri(&i.module));
            triple(&id, "contractGroup", lit(&self.interface_group(i)?));
            if let Some(path) = &i.rust_path {
                triple(&id, "implementationPath", lit(path));
            }
            if let Some(implementation) = &i.implementation {
                triple(&id, "implementation", lit(implementation));
            }
        }
        for s in &self.sources {
            for (p, v) in [
                ("kind", "Source"),
                ("identity", &s.identity),
                ("role", &s.role),
                ("applicability", &s.applicability),
            ] {
                triple(&s.id, p, lit(v));
            }
        }
        for e in &self.errors {
            let id = format!("error_{}", e.name);
            for (p, v) in [
                ("kind", "Error"),
                ("name", &e.name),
                ("code", &e.code),
                ("meaning", &e.meaning),
                ("phase", &e.phase),
                ("acceptance", &e.acceptance),
            ] {
                triple(&id, p, lit(v));
            }
            triple(
                &id,
                "retryable",
                lit(if e.retryable { "true" } else { "false" }),
            );
        }
        for c in &self.contract_cases {
            let id = format!("case_{}", c.id);
            for (p, v) in [
                ("kind", "ContractCase"),
                ("name", &c.id),
                ("targetType", &c.target_type),
                ("oracle", &c.oracle),
            ] {
                triple(&id, p, lit(v));
            }
            triple(&id, "inputRust", lit(&format!("{:?}", c.input_json)));
            triple(
                &id,
                "expectedValid",
                lit(if c.expected_valid { "true" } else { "false" }),
            );
        }
        Ok(lines.into_iter().collect::<Vec<_>>().join("\n") + "\n")
    }
    pub fn impact(&self, changed: &BTreeSet<String>) -> BTreeSet<String> {
        // Compile the same monotone propagation rules once for this immutable
        // design value. No cross-version cache or secondary state authority.
        let mut reverse: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut edge = |from: String, to: String| {
            reverse.entry(from).or_default().insert(to);
        };
        let types: BTreeMap<&str, String> = self
            .types
            .iter()
            .map(|t| (t.name.as_str(), format!("type_{}", t.name)))
            .collect();
        for model in &self.semantic_models {
            for target in ["semantic_schema", "semantic_typed", "semantic_contracts"] {
                edge(format!("semantic_model_{}", model.rust_name), target.into());
            }
        }
        for t in &self.types {
            let target = format!("type_{}", t.name);
            for field in &t.fields {
                for token in field
                    .rust_type
                    .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                {
                    if let Some(from) = types.get(token) {
                        edge(from.clone(), target.clone());
                    }
                }
            }
        }
        for interface in &self.interfaces {
            let target = format!("interface_{}", interface.name);
            edge(target.clone(), interface.module.clone());
            for ty in [&interface.input, &interface.output, &interface.error] {
                for token in ty.split(|c: char| !c.is_ascii_alphanumeric() && c != '_') {
                    if let Some(from) = types.get(token) {
                        edge(from.clone(), target.clone());
                    }
                }
            }
        }
        for machine in &self.machines {
            edge(format!("machine_{}", machine.name), machine.module.clone());
        }
        for module in &self.modules {
            edge("model".into(), module.id.clone());
            for from in module
                .depends_on
                .iter()
                .chain(module.realizes.iter())
                .chain(module.source_refs.iter())
            {
                edge(from.clone(), module.id.clone());
            }
        }
        let mut affected = changed.clone();
        let mut pending: std::collections::VecDeque<String> = changed.iter().cloned().collect();
        while let Some(from) = pending.pop_front() {
            if let Some(targets) = reverse.get(&from) {
                for target in targets {
                    if affected.insert(target.clone()) {
                        pending.push_back(target.clone());
                    }
                }
            }
        }
        // These are deliberately ordered post-closure passes, just as before.
        // New constraint/validation IDs do not restart module propagation.
        for c in &self.constraints {
            if affected.contains(&c.module) || affected.contains(&c.requirement) {
                affected.insert(c.id.clone());
            }
        }
        for v in &self.validations {
            if affected.contains(&v.constraint) || affected.contains(&v.module) {
                affected.insert(v.id.clone());
            }
        }
        affected
    }
}
pub fn load(path: &Path) -> Result<Design> {
    let bytes = oe_core::read_regular_bytes(path, 4 * 1024 * 1024)?;
    if composition::transport::schema(&bytes)
        .ok()
        .as_deref()
        .is_some_and(composition::transport::is_composed)
    {
        return Ok(composition::load(
            path.parent().context("design_composition_parent_missing")?,
            path,
        )?
        .design);
    }
    toml::from_str::<Design>(std::str::from_utf8(&bytes)?)
        .context("invalid_annotated_design")?
        .normalize()
}

/// Formal validation of editable input requires a matching executing implementation.
pub fn load_current(root: &oe_core::SkillRoot, path: &Path) -> Result<Design> {
    provenance::require(root)?;
    let bytes = oe_core::read_regular_bytes(path, 4 * 1024 * 1024)?;
    if composition::transport::schema(&bytes)
        .ok()
        .as_deref()
        .is_some_and(composition::transport::is_composed)
    {
        let loaded = composition::load(root.path(), path)?;
        composition::require_current(root, &loaded)?;
        return Ok(loaded.design);
    }
    load(path)
}
pub fn load_json(path: &Path) -> Result<Design> {
    serde_json::from_value::<Design>(strict_json(&oe_core::read_regular_bytes(
        path,
        4 * 1024 * 1024,
    )?)?)?
    .normalize()
}
pub fn changes(before: &Design, after: &Design) -> Result<BTreeSet<String>> {
    let mut a = BTreeMap::new();
    let mut b = BTreeMap::new();
    for (target, design) in [(&mut a, before), (&mut b, after)] {
        for m in &design.modules {
            target.insert(m.id.clone(), serde_json::to_value(m)?);
        }
        for r in &design.requirements {
            target.insert(r.id.clone(), serde_json::to_value(r)?);
        }
        for s in &design.sources {
            target.insert(s.id.clone(), serde_json::to_value(s)?);
        }
        for model in &design.semantic_models {
            target.insert(
                format!("semantic_model_{}", model.rust_name),
                serde_json::to_value(model)?,
            );
        }
        for t in &design.types {
            target.insert(format!("type_{}", t.name), serde_json::to_value(t)?);
        }
        for c in &design.constraints {
            target.insert(c.id.clone(), serde_json::to_value(c)?);
        }
        for m in &design.machines {
            target.insert(format!("machine_{}", m.name), serde_json::to_value(m)?);
        }
        for i in &design.interfaces {
            target.insert(format!("interface_{}", i.name), serde_json::to_value(i)?);
        }
        for v in &design.validations {
            target.insert(v.id.clone(), serde_json::to_value(v)?);
        }
        for e in &design.errors {
            target.insert(format!("error_{}", e.name), serde_json::to_value(e)?);
        }
        for c in &design.contract_cases {
            target.insert(format!("case_{}", c.id), serde_json::to_value(c)?);
        }
        target.insert(
            "model".into(),
            serde_json::json!([
                design.schema,
                design.version,
                design.problem,
                design.purpose
            ]),
        );
    }
    Ok(a.keys()
        .chain(b.keys())
        .filter(|k| a.get(*k) != b.get(*k))
        .cloned()
        .collect())
}

#[cfg(test)]
#[test]
fn promoted_adoption_map_shapes_are_closed_and_declared() {
    let names = BTreeSet::from(["ArtifactReference".to_owned()]);
    assert!(rust_type(
        "std::collections::BTreeMap<String, Vec<String>>",
        &names
    ));
    assert!(rust_type(
        "std::collections::BTreeMap<String, ArtifactReference>",
        &names
    ));
    assert!(!rust_type(
        "std::collections::BTreeMap<String, ArtifactReference>",
        &BTreeSet::new()
    ));
    for bad in [
        "std::collections::BTreeMap<String, Unknown>",
        "std::collections::BTreeMap<String, Vec<Unknown>>",
        "std::collections::HashMap<String, ArtifactReference>",
        "std::collections::BTreeMap<String, ArtifactReference>; panic!()",
        "Vec<std::collections::BTreeMap<String, ArtifactReference>>",
        "Option<std::collections::BTreeMap<String, ArtifactReference>>",
        "Vec<Option<std::collections::BTreeMap<String, Vec<String>>>>",
    ] {
        assert!(!rust_type(bad, &names));
    }
    let groups = BTreeMap::from([("ArtifactReference", "engineering")]);
    assert_eq!(
        referenced_contract(
            "std::collections::BTreeMap<String, ArtifactReference>",
            &groups
        ),
        Some("ArtifactReference")
    );
    assert_eq!(
        referenced_contract(
            "Vec<std::collections::BTreeMap<String, ArtifactReference>>",
            &groups
        ),
        Some("ArtifactReference")
    );
}

#[cfg(test)]
mod model_transport_tests {
    use super::*;
    #[test]
    fn migration_prefix_preserves_rdf_terms_and_literals() {
        use oxigraph::io::{RdfFormat, RdfParser};
        let id = "a".repeat(64);
        let literal = serde_json::to_string(
            "migration:abc <urn:oev2:design:semantic-migration-mapping:x> \"中文\"",
        )
        .unwrap();
        let expanded = format!(
            "<urn:oev2:design:semantic-migration-mapping:{id}> <urn:oev2:design:originalScopeId> {literal} .\n"
        );
        let compact = format!(
            "{MIGRATION_MAPPING_PREFIX}migration:{id} <urn:oev2:design:originalScopeId> {literal} .\n"
        );
        let parse = |s: &str| {
            RdfParser::from_format(RdfFormat::Turtle)
                .for_slice(s.as_bytes())
                .collect::<std::result::Result<Vec<_>, _>>()
                .unwrap()
        };
        assert_eq!(parse(&expanded), parse(&compact));
        assert!(compact.contains(&literal));
        // Digit-leading digest remains a legal PN_LOCAL; no ID normalization.
        let digit = compact.replace(&id, &"0".repeat(64));
        assert_eq!(
            parse(&digit),
            parse(&expanded.replace(&id, &"0".repeat(64)))
        );
    }
}
