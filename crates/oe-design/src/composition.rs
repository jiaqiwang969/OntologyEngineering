//! /2 source composition reuses the single annotated Design semantic owner.
use crate::{
    Constraint, ContractCase, Design, ErrorDefinition, Interface, Machine, Module, Requirement,
    Source, Type, Validation,
};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};
mod groups;
#[path = "../../../build_support/design_composition.rs"]
pub mod transport;

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Body {
    schema: String,
    #[serde(default)]
    requirements: Vec<Requirement>,
    #[serde(default)]
    modules: Vec<Module>,
    #[serde(default)]
    types: Vec<Type>,
    #[serde(default)]
    constraints: Vec<Constraint>,
    #[serde(default)]
    machines: Vec<Machine>,
    #[serde(default)]
    interfaces: Vec<Interface>,
    #[serde(default)]
    validations: Vec<Validation>,
    // Presence is versioned: even an empty new field must reject under /2.
    validation_groups: Option<Vec<groups::Group>>,
    #[serde(default)]
    sources: Vec<Source>,
    #[serde(default)]
    errors: Vec<ErrorDefinition>,
    #[serde(default)]
    contract_cases: Vec<ContractCase>,
    #[serde(default)]
    semantic_models: Vec<crate::schema_design::SemanticModelSpec>,
}
impl Body {
    fn append(self, d: &mut Design) {
        d.requirements.extend(self.requirements);
        d.modules.extend(self.modules);
        d.types.extend(self.types);
        d.constraints.extend(self.constraints);
        d.machines.extend(self.machines);
        d.interfaces.extend(self.interfaces);
        d.validations.extend(self.validations);
        d.sources.extend(self.sources);
        d.errors.extend(self.errors);
        d.contract_cases.extend(self.contract_cases);
        d.semantic_models.extend(self.semantic_models);
    }
}
pub struct Loaded {
    pub design: Design,
    pub snapshot: transport::Snapshot,
    pub evidence: Value,
}
impl Loaded {
    pub fn input_schema(&self) -> &str {
        &self.snapshot.manifest.schema
    }
    pub fn uses_source_reference(&self) -> bool {
        matches!(
            self.input_schema(),
            transport::REFERENCE_ROOT_SCHEMA | transport::GROUPED_ROOT_SCHEMA
        )
    }
    pub fn normalized_schema(&self) -> &'static str {
        if self.input_schema() == transport::GROUPED_ROOT_SCHEMA {
            "oev2.normalized-design/4"
        } else if self.uses_source_reference() {
            "oev2.normalized-design/3"
        } else {
            "oev2.normalized-design/2"
        }
    }
    pub fn generation_schema(&self) -> &'static str {
        if self.input_schema() == transport::GROUPED_ROOT_SCHEMA {
            "oev2.generation/4"
        } else if self.uses_source_reference() {
            "oev2.generation/3"
        } else {
            "oev2.generation/2"
        }
    }
    /// All normalization/publication consumers preserve the same complete
    /// canonical locator bytes under the existing artifact budget.
    pub fn source_bytes(&self) -> Result<Vec<u8>> {
        let bytes = oe_wire::json::canonical_json(&self.evidence)?;
        ensure!(
            bytes.len() as u64 <= oe_core::source_limits::PUBLIC_FILE_BYTES,
            "composition_source_map_artifact_budget"
        );
        Ok(bytes)
    }
    /// Compact complete-file identity binds the retained full field-location
    /// artifact by SHA; receipts do not duplicate its potentially large bytes.
    pub fn source_identity(&self) -> Result<Value> {
        let files: Vec<_> = self.snapshot.files.iter().map(|(path,bytes)|
            json!({"path":path,"bytes":bytes.len(),"sha256":self.snapshot.hashes[path]})).collect();
        Ok(
            json!({"schema":"oev2.design-composition-identity/2","input_schema":self.input_schema(),
            "root":self.snapshot.root,"files":files,"actual_total_bytes":self.snapshot.total_bytes,
            "source_map_sha256":oe_core::sha256(oe_wire::json::canonical_json(&self.evidence)?),
            "source_map_product":"design-composition-source.json"}),
        )
    }
    pub fn verify(&self, base: &Path) -> Result<()> {
        let now = transport::snapshot(base, &base.join(&self.snapshot.root))
            .map_err(anyhow::Error::msg)?;
        ensure!(
            now.files == self.snapshot.files && now.hashes == self.snapshot.hashes,
            "design_composition_source_changed"
        );
        Ok(())
    }
    pub fn normalized(&self) -> Result<Vec<u8>> {
        if self.uses_source_reference() {
            self.source_bytes()?;
            return Ok(oe_wire::json::canonical_json(&json!({
                "schema":self.normalized_schema(),"input_schema":self.input_schema(),
                "semantic_ast_schema":"oev2.design/1","design":self.design,
                "source_ref":self.source_identity()?
            }))?);
        }
        Ok(oe_wire::json::canonical_json(
            &json!({"schema":"oev2.normalized-design/2","input_schema":transport::ROOT_SCHEMA,
            "semantic_ast_schema":"oev2.design/1","design":self.design,"source":self.evidence}),
        )?)
    }
    /// /3 references actual canonical sidecar bytes, not merely JSON-equivalent
    /// content. /2 keeps its historical publication check and wire unchanged.
    pub fn verify_reference_source(&self, normalized: &Value, source: &[u8]) -> Result<()> {
        if self.uses_source_reference() {
            ensure!(
                normalized["schema"] == self.normalized_schema()
                    && normalized["source_ref"] == self.source_identity()?
                    && source == self.source_bytes()?
                    && normalized["source_ref"]["source_map_sha256"] == oe_core::sha256(source),
                "generation_composition_reference_bytes_mismatch"
            );
        }
        Ok(())
    }
}
/// Only diagnostic loading lacks current compiled source authority. Formal
/// consumers call require_current before producing/publishing current results.
pub fn load(base: &Path, path: &Path) -> Result<Loaded> {
    let base = transport::absolute(base).map_err(anyhow::Error::msg)?;
    let snapshot = transport::snapshot(&base, path).map_err(anyhow::Error::msg)?;
    let m = &snapshot.manifest;
    let grouped = m.schema == transport::GROUPED_ROOT_SCHEMA;
    let mut budget = groups::Budget::default();
    let mut origins = Vec::new();
    let mut design = Design {
        schema: "oev2.design/1".into(),
        version: m.version.clone(),
        problem: m.problem.clone(),
        purpose: m.purpose.clone(),
        requirements: vec![],
        modules: vec![],
        types: vec![],
        constraints: vec![],
        machines: vec![],
        interfaces: vec![],
        validations: vec![],
        sources: vec![],
        errors: vec![],
        contract_cases: vec![],
        semantic_models: vec![],
    };
    let parent = base
        .join(&snapshot.root)
        .parent()
        .context("design_composition_parent_missing")?
        .to_owned();
    for fragment in &m.fragments {
        let path = parent.join(&fragment.path);
        let name = oe_core::portable_source_path(&base, &path)?;
        let bytes = snapshot
            .files
            .get(&name)
            .context("design_composition_unread_declared_input")?;
        let mut body = if fragment.format == "oev2.design/1" {
            let d: Design = toml::from_str(std::str::from_utf8(bytes)?)?;
            ensure!(
                d.schema == "oev2.design/1"
                    && d.version == m.version
                    && d.problem == m.problem
                    && d.purpose == m.purpose,
                "design_composition_foundation_identity_mismatch"
            );
            Body {
                schema: d.schema,
                requirements: d.requirements,
                modules: d.modules,
                types: d.types,
                constraints: d.constraints,
                machines: d.machines,
                interfaces: d.interfaces,
                validations: d.validations,
                validation_groups: None,
                sources: d.sources,
                errors: d.errors,
                contract_cases: d.contract_cases,
                semantic_models: d.semantic_models,
            }
        } else {
            let body: Body = toml::from_str(std::str::from_utf8(bytes)?)?;
            ensure!(
                body.schema == fragment.format,
                "design_composition_fragment_schema_mismatch"
            );
            if body.schema == transport::GROUPED_FRAGMENT_SCHEMA {
                ensure!(
                    grouped && body.validation_groups.is_some(),
                    "validation_groups_required"
                );
            } else {
                ensure!(
                    body.validation_groups.is_none(),
                    "validation_groups_legacy_field_unsupported"
                );
            }
            body
        };
        if grouped {
            budget.flat(&body.validations)?;
            if let Some(groups) = &body.validation_groups {
                groups::expand(
                    groups,
                    &name,
                    &mut budget,
                    &mut body.validations,
                    &mut origins,
                )?;
            }
        }
        body.append(&mut design);
    }
    // One validator checks identities, all full field metadata and cross-file
    // requirement/module/type/check/interface/platform references together.
    let design = design.normalize()?;
    let files=snapshot.files.iter().map(|(name,bytes)|Ok(json!({"path":name,"bytes":bytes.len(),"sha256":snapshot.hashes[name],"field_locators":locators(bytes)?}))).collect::<Result<Vec<_>>>()?;
    let mut evidence = json!({"schema":"oev2.design-composition-source/2","input_schema":snapshot.manifest.schema,"root":snapshot.root,
        "file_byte_limit":transport::FILE_BYTES,"total_byte_limit":transport::TOTAL_BYTES,"file_count_limit":transport::FILE_COUNT,
        "actual_total_bytes":snapshot.total_bytes,"files":files,"scope":if snapshot.manifest.schema == transport::ROOT_SCHEMA {"Exact input2 root/fragment bytes and field positions; one original semantic entity store. No engineering acceptance or unlimited downstream artifacts."} else {"Exact input3 root/fragment bytes and field positions; complete source2 sidecar referenced by normalized3. Original semantic entity store and bounded artifacts; no engineering acceptance."}});
    if grouped {
        origins.sort_by(|a, b| a.validation_id.cmp(&b.validation_id));
        evidence["schema"] = json!("oev2.design-composition-source/3");
        evidence["validation_encoding"] = json!(transport::VALIDATION_ENCODING);
        evidence["grouped_validation_origins"] = serde_json::to_value(origins)?;
        evidence["validation_expansion"] = json!({"row_limit":groups::ROW_LIMIT,
            "json_byte_limit":groups::BYTE_LIMIT,"actual_rows":budget.rows,"actual_json_bytes":budget.bytes});
        evidence["scope"] = json!(
            "Exact input4 bytes, parser locations and grouped field origins; bounded expansion into the original Validation/Design owner. Complete source3 sidecar referenced by normalized4; no engineering acceptance."
        );
    }
    let loaded = Loaded {
        design,
        snapshot,
        evidence,
    };
    loaded.verify(&base)?;
    Ok(loaded)
}
pub fn binding(root: &oe_core::SkillRoot, loaded: &Loaded) -> Result<Value> {
    let compiled = crate::provenance::compiled()?;
    let expected = compiled.get("composition_files").and_then(Value::as_object);
    let current: BTreeMap<_, _> = loaded
        .snapshot
        .files
        .iter()
        .map(|(p, b)| {
            (
                p.clone(),
                json!({"bytes":b.len(),"sha256":loaded.snapshot.hashes[p]}),
            )
        })
        .collect();
    let matched = expected
        .is_some_and(|e| serde_json::to_value(e).ok() == serde_json::to_value(&current).ok());
    loaded.verify(root.path())?;
    Ok(
        json!({"schema":"oev2.compiled-composition-binding/2","status":if matched{"matched"}else{"unmatched_or_uncaptured"},
        "root":loaded.snapshot.root,"input_schema":loaded.input_schema(),"current_files":current,"compiled_files":expected,
        "source":loaded.source_identity()?,"scope":if loaded.input_schema() == transport::GROUPED_ROOT_SCHEMA {"Selected exact root4 and all fragments captured by current compiler; source3 binds original defaults/overrides. Original1/2/3 binding retained."} else if loaded.uses_source_reference() {"Selected exact root3 and all fragments captured by current compiler; source reference does not weaken legacy1/2 binding."} else {"Selected exact root2+all actual fragments captured by current normalizer compilation; legacy1 code binding remains independent."}}),
    )
}
pub fn require_current(root: &oe_core::SkillRoot, loaded: &Loaded) -> Result<Value> {
    crate::provenance::require(root)?;
    let b = binding(root, loaded)?;
    ensure!(
        b["status"] == "matched",
        "design_composition_uncompiled_or_changed_input"
    );
    Ok(b)
}
/// Actual parser byte ranges, including multiline/quoted fields. Key paths are
/// original document locations, not post-normalization fabricated line numbers.
fn locators(bytes: &[u8]) -> Result<Vec<Value>> {
    let text = std::str::from_utf8(bytes)?;
    let doc: toml_edit::ImDocument<String> = text.parse()?;
    let mut out = Vec::new();
    let lines: Vec<_> = text
        .as_bytes()
        .iter()
        .enumerate()
        .filter_map(|(i, b)| (*b == b'\n').then_some(i))
        .collect();
    fn value(v: &toml_edit::Value, path: Vec<String>, lines: &[usize], out: &mut Vec<Value>) {
        if let Some(span) = v.span() {
            out.push(
                json!({"key_path":path,"byte_start":span.start,"byte_end":span.end,
            "line_start":1+lines.partition_point(|i|*i<span.start),
            "line_end":1+lines.partition_point(|i|*i<span.end)}),
            );
        }
        match v {
            toml_edit::Value::Array(a) => {
                for (i, v) in a.iter().enumerate() {
                    let mut p = path.clone();
                    p.push(i.to_string());
                    value(v, p, lines, out)
                }
            }
            toml_edit::Value::InlineTable(t) => {
                for (k, v) in t.iter() {
                    let mut p = path.clone();
                    p.push(k.into());
                    value(v, p, lines, out)
                }
            }
            _ => (),
        }
    }
    fn table(t: &toml_edit::Table, path: Vec<String>, lines: &[usize], out: &mut Vec<Value>) {
        for (k, item) in t.iter() {
            let mut p = path.clone();
            p.push(k.into());
            match item {
                toml_edit::Item::Value(v) => value(v, p, lines, out),
                toml_edit::Item::Table(t) => table(t, p, lines, out),
                toml_edit::Item::ArrayOfTables(a) => {
                    for (i, t) in a.iter().enumerate() {
                        let mut p = p.clone();
                        p.push(i.to_string());
                        table(t, p, lines, out)
                    }
                }
                toml_edit::Item::None => (),
            }
        }
    }
    table(doc.as_table(), vec![], &lines, &mut out);
    Ok(out)
}
