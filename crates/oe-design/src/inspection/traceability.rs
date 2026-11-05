//! Join existing design relations and parsed sources without creating evidence.
use super::CodeFile;
use crate::Design;
use anyhow::Result;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

fn symbol(files: &BTreeMap<&str, &CodeFile>, path: &str, name: &str, test: bool) -> Value {
    let file = files.get(path);
    let present = file.is_some_and(|f| {
        if test {
            f.tests.contains(name)
        } else {
            f.functions.contains(name)
        }
    });
    json!({"path":path,"symbol":name,"source_sha256":file.map(|f| &f.sha256),
        "symbol_present":present})
}

pub(super) fn traceability(root: &Path, design: &Design, code: &[CodeFile]) -> Result<Value> {
    let design = design.clone().normalize()?;
    let files: BTreeMap<_, _> = code.iter().map(|f| (f.path.as_str(), f)).collect();
    let mut validations_by_constraint: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut validations = BTreeMap::new();
    for v in &design.validations {
        validations_by_constraint
            .entry(&v.constraint)
            .or_default()
            .push(&v.id);
        validations.insert(
            v.id.as_str(),
            json!({"id":v.id,"constraint":v.constraint,
            "module":v.module,"oracle":v.oracle,"scope":v.scope,
            "test":symbol(&files, &v.rust_path, &v.behavior, true)}),
        );
    }
    let mut constraints_by_requirement: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut constraints = BTreeMap::new();
    for c in &design.constraints {
        constraints_by_requirement
            .entry(&c.requirement)
            .or_default()
            .push(&c.id);
        constraints.insert(c.id.as_str(), json!({"id":c.id,"requirement":c.requirement,
            "module":c.module,"statement":c.statement,"severity":c.severity,
            "checker":symbol(&files, &c.rust_path, &c.check, false),
            "validations":validations_by_constraint.get(c.id.as_str()).cloned().unwrap_or_default()}));
    }
    let mut realizations: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut modules = BTreeMap::new();
    for m in &design.modules {
        for requirement in &m.realizes {
            let owners = realizations.entry(requirement).or_default();
            if !owners.contains(&m.id.as_str()) {
                owners.push(&m.id);
            }
        }
        modules.insert(
            m.id.as_str(),
            json!({"id":m.id,"kind":m.kind,
            "rust_path":m.rust_path,"path_present":root.join(&m.rust_path).exists(),
            "purpose":m.purpose,"platform":m.platform,"effects":m.effects,"failures":m.failures,
            "declared_status":m.status,"realizes":m.realizes,
            "depends_on":m.depends_on,"source_refs":m.source_refs}),
        );
    }
    let mut requirements = Vec::new();
    let mut with_gaps = 0usize;
    for r in &design.requirements {
        let owners = realizations.get(r.id.as_str()).cloned().unwrap_or_default();
        let checks = constraints_by_requirement
            .get(r.id.as_str())
            .cloned()
            .unwrap_or_default();
        let mut gaps = Vec::new();
        let mut gap = |code: &str, subject: &str| {
            gaps.push(json!({"code":code,"subject":subject}));
        };
        if owners.is_empty() {
            gap("requirement_owner_missing", &r.id);
        }
        if checks.is_empty() {
            gap("requirement_constraint_missing", &r.id);
        }
        for owner in &owners {
            if modules[owner]["path_present"] != true {
                gap("implementation_missing", owner);
            }
        }
        for id in &checks {
            let check = &constraints[id];
            if check["checker"]["symbol_present"] != true {
                gap("constraint_execution_missing", id);
            }
            let cases = validations_by_constraint.get(id);
            if cases.is_none_or(Vec::is_empty) {
                gap("constraint_validation_missing", id);
            }
            for case in cases.into_iter().flatten() {
                if validations[case]["test"]["symbol_present"] != true {
                    gap("behavior_case_missing", case);
                }
            }
        }
        if !gaps.is_empty() {
            with_gaps += 1;
        }
        requirements.push(json!({"id":r.id,"function":r.function,
            "condition":r.condition,"acceptance":r.acceptance,
            "realization_modules":owners,"constraints":checks,"static_gaps":gaps}));
    }
    Ok(json!({
        "schema":"oev2.design-traceability/1",
        "design_sha256":oe_core::sha256(&design.canonical()?),
        "scope":"Declared requirement, realization, checker and validation relations with current parsed source bytes. Different roles may have different modules. No call-path, sufficiency, execution or engineering proof; undeclared relations remain outside this view.",
        "counts":{"requirements":requirements.len(),"requirements_with_static_gaps":with_gaps,
            "modules":modules.len(),"constraints":constraints.len(),"validations":validations.len()},
        "requirements":requirements,"modules":modules.into_values().collect::<Vec<_>>(),
        "constraints":constraints.into_values().collect::<Vec<_>>(),
        "validations":validations.into_values().collect::<Vec<_>>(),"sources":design.sources,
        "evidence":{"generation":"not_run","behavior_execution":"not_run",
            "oracle_independence":"not_assessed","engineering_acceptance":"not_established",
            "method_gain":"not_evaluated"}
    }))
}
