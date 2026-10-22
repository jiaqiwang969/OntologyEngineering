//! Explicit /4 transport expansion into the existing Validation owner. Count
//! complete JSON bytes before cloning defaults; all fields retain source paths.
use crate::Validation;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io};

pub(super) const ROW_LIMIT: usize = 10_000;
pub(super) const BYTE_LIMIT: usize = 8 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Group {
    defaults: Defaults,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Defaults {
    module: String,
    constraint: String,
    rust_path: String,
    oracle: String,
    scope: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    behavior: String,
    module: Option<String>,
    constraint: Option<String>,
    rust_path: Option<String>,
    oracle: Option<String>,
    scope: Option<String>,
}
#[derive(Serialize)]
pub(super) struct Origin {
    pub validation_id: String,
    source: String,
    fields: BTreeMap<&'static str, Vec<String>>,
}
pub(super) struct Budget {
    pub rows: usize,
    pub bytes: usize,
}
impl Default for Budget {
    fn default() -> Self {
        Self { rows: 0, bytes: 2 } // Complete array includes both brackets.
    }
}
struct Counter(usize);
impl io::Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .filter(|n| *n <= BYTE_LIMIT)
            .ok_or_else(|| io::Error::other("validation_group_expanded_byte_budget"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Budget {
    fn admit(&mut self, row: &impl Serialize) -> Result<()> {
        ensure!(self.rows < ROW_LIMIT, "validation_group_row_budget");
        let mut count = Counter(self.bytes + usize::from(self.rows != 0));
        // serde_json applies the same UTF-8 and escape rules as Validation;
        // the writer retains no serialized buffer, including on rejection.
        serde_json::to_writer(&mut count, row)?;
        self.bytes = count.0;
        self.rows += 1;
        Ok(())
    }
    pub fn flat(&mut self, rows: &[Validation]) -> Result<()> {
        for row in rows {
            self.admit(row)?;
        }
        Ok(())
    }
}
pub(super) fn expand(
    groups: &[Group],
    source: &str,
    budget: &mut Budget,
    rows: &mut Vec<Validation>,
    origins: &mut Vec<Origin>,
) -> Result<()> {
    ensure!(!groups.is_empty(), "validation_groups_empty");
    for (g, group) in groups.iter().enumerate() {
        ensure!(!group.cases.is_empty(), "validation_group_cases_empty");
        for (c, case) in group.cases.iter().enumerate() {
            let d = &group.defaults;
            let values = BTreeMap::from([
                ("id", case.id.as_str()),
                ("behavior", case.behavior.as_str()),
                ("module", case.module.as_deref().unwrap_or(&d.module)),
                (
                    "constraint",
                    case.constraint.as_deref().unwrap_or(&d.constraint),
                ),
                (
                    "rust_path",
                    case.rust_path.as_deref().unwrap_or(&d.rust_path),
                ),
                ("oracle", case.oracle.as_deref().unwrap_or(&d.oracle)),
                ("scope", case.scope.as_deref().unwrap_or(&d.scope)),
            ]);
            budget.admit(&values)?;
            rows.push(Validation {
                id: values["id"].into(),
                module: values["module"].into(),
                behavior: values["behavior"].into(),
                constraint: values["constraint"].into(),
                rust_path: values["rust_path"].into(),
                oracle: values["oracle"].into(),
                scope: values["scope"].into(),
            });
            let fields = [
                ("id", true),
                ("behavior", true),
                ("module", case.module.is_some()),
                ("constraint", case.constraint.is_some()),
                ("rust_path", case.rust_path.is_some()),
                ("oracle", case.oracle.is_some()),
                ("scope", case.scope.is_some()),
            ]
            .into_iter()
            .map(|(name, local)| {
                let mut path = vec!["validation_groups".into(), g.to_string()];
                if local {
                    path.extend(["cases".into(), c.to_string()]);
                } else {
                    path.push("defaults".into());
                }
                path.push(name.into());
                (name, path)
            })
            .collect();
            origins.push(Origin {
                validation_id: case.id.clone(),
                source: source.into(),
                fields,
            });
        }
    }
    Ok(())
}
