//! Fixed inCAD source intake. No browser, CAD, model call or second Project ledger.
use anyhow::{Context, Result, ensure};
use oe_contracts::{ArtifactReference, IncadIntakeRequest};
use oe_core::{FileIdentity, no_symlinks, read_regular_bytes, sha256};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
};
pub mod cad;

const MAX_FILE: usize = 16 * 1024 * 1024;
const MAX_TOTAL: usize = 64 * 1024 * 1024;
const PROJECTIONS: [&str; 9] = [
    "input.json",
    "reference.json",
    "study.json",
    "original.json",
    "input.html",
    "reference.html",
    "study.html",
    "original.html",
    "original-page.html",
];

/// Verified source snapshot; a caller-supplied JSON audit cannot construct this.
pub struct VerifiedIntake {
    input_bytes: Vec<u8>,
    task_bytes: Vec<u8>,
    specifications: Value,
    task: Value,
    sources: BTreeMap<PathBuf, FileIdentity>,
    roles: BTreeMap<String, ArtifactReference>,
    audit: Value,
    case_folder: PathBuf,
    case_manifest: Value,
    case_record: Value,
}
impl VerifiedIntake {
    pub fn specifications_bytes(&self) -> &[u8] {
        &self.input_bytes
    }
    pub fn task_bytes(&self) -> &[u8] {
        &self.task_bytes
    }
    pub fn roles(&self) -> &BTreeMap<String, ArtifactReference> {
        &self.roles
    }
    pub fn audit(&self) -> &Value {
        &self.audit
    }
    /// Only original specifications and the user's request enter this read window.
    /// Reference solutions/whole-page prose are retained separately in the archive.
    pub fn design_window(&self) -> Value {
        json!({"schema":"oev2.incad-design-window/1","specifications":self.specifications,
        "current_request":self.task["current_request"],"baseline_exposure":"development_replay",
        "source_claim_status":"supplier_declared","conditions_confirmation":"not_run",
        "design_execution":"not_run","engineering_adoption":"not_run"})
    }
    pub fn ensure_current(&self) -> Result<()> {
        for (path, expected) in &self.sources {
            let raw = read_regular_bytes(path, MAX_FILE)?;
            ensure!(
                raw.len() as u64 == expected.bytes && sha256(&raw) == expected.sha256,
                "incad_source_changed:{}",
                path.display()
            );
        }
        Ok(())
    }
}
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v.get(key)
        .and_then(Value::as_str)
        .context(format!("incad_string_required:{key}"))
}
fn closed(v: &Value, keys: &[&str]) -> Result<()> {
    let map = v.as_object().context("incad_closed_object_required")?;
    ensure!(
        map.len() == keys.len() && keys.iter().all(|k| map.contains_key(*k)),
        "incad_unrecognized_field_or_missing_field"
    );
    Ok(())
}
fn path(root: &Path, name: &str) -> Result<PathBuf> {
    ensure!(
        !name.is_empty()
            && !name.contains(['\\', ':'])
            && !Path::new(name).is_absolute()
            && Path::new(name)
                .components()
                .all(|c| matches!(c, Component::Normal(_))),
        "incad_relative_path_invalid"
    );
    let p = root.join(name);
    no_symlinks(&p)?;
    Ok(p)
}
fn pin(path: &Path, digest: &str) -> ArtifactReference {
    ArtifactReference {
        path: path.to_string_lossy().into_owned(),
        sha256: digest.to_owned(),
    }
}
fn inventory(v: &Value) -> Result<BTreeMap<String, &Value>> {
    let files = v["files"]
        .as_array()
        .context("incad_manifest_files_missing")?;
    ensure!(files.len() <= 8192, "incad_manifest_file_limit");
    let mut out = BTreeMap::new();
    for f in files {
        let name = text(f, "path")?;
        // Validate lexical containment even for CAD files which are not opened here.
        ensure!(
            !name.is_empty()
                && !name.contains(['\\', ':'])
                && !Path::new(name).is_absolute()
                && Path::new(name)
                    .components()
                    .all(|c| matches!(c, Component::Normal(_))),
            "incad_manifest_path_invalid"
        );
        ensure!(
            out.insert(name.to_owned(), f).is_none(),
            "incad_duplicate_manifest_path"
        );
        ensure!(
            f["bytes"].as_u64().is_some(),
            "incad_manifest_bytes_missing"
        );
        let digest = text(f, "sha256")?;
        ensure!(
            digest.len() == 64
                && digest
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "incad_manifest_sha_invalid"
        );
    }
    Ok(out)
}
struct Reader {
    sources: BTreeMap<PathBuf, FileIdentity>,
    total: usize,
}
impl Reader {
    fn read(&mut self, path: &Path, digest: &str, bytes: Option<u64>) -> Result<Vec<u8>> {
        no_symlinks(path)?;
        let remaining = MAX_TOTAL
            .checked_sub(self.total)
            .context("incad_total_byte_limit")?;
        let raw = read_regular_bytes(path, MAX_FILE.min(remaining))?;
        ensure!(
            sha256(&raw) == digest && bytes.is_none_or(|n| n == raw.len() as u64),
            "incad_source_identity_mismatch:{}",
            path.display()
        );
        self.total += raw.len();
        let identity = FileIdentity {
            bytes: raw.len() as u64,
            sha256: digest.into(),
        };
        if let Some(old) = self.sources.insert(path.to_owned(), identity.clone()) {
            ensure!(old == identity, "incad_conflicting_source_identity");
        }
        Ok(raw)
    }
    fn member(
        &mut self,
        root: &Path,
        files: &BTreeMap<String, &Value>,
        name: &str,
    ) -> Result<Vec<u8>> {
        let item = files.get(name).context("incad_manifest_member_missing")?;
        self.read(
            &path(root, name)?,
            text(item, "sha256")?,
            item["bytes"].as_u64(),
        )
    }
}

pub fn verify(request: &IncadIntakeRequest) -> Result<VerifiedIntake> {
    request
        .validate()
        .map_err(|_| anyhow::anyhow!("incad_generated_contract_invalid"))?;
    ensure!(
        request.schema == "oev2.incad-intake/1"
            && request.case_number.len() == 6
            && request.case_number.bytes().all(|b| b.is_ascii_digit()),
        "incad_request_identity_invalid"
    );
    let root = Path::new(&request.archive_root);
    ensure!(
        root.is_absolute() && root.is_dir(),
        "incad_archive_root_invalid"
    );
    no_symlinks(root)?;
    let manifest_path = path(root, &request.projection_manifest.path)?;
    let folder = manifest_path.parent().context("incad_projection_parent")?;
    ensure!(
        manifest_path
            .file_name()
            .is_some_and(|s| s == "manifest.json"),
        "incad_projection_manifest_name"
    );
    let mut reader = Reader {
        sources: BTreeMap::new(),
        total: 0,
    };
    let raw = reader.read(&manifest_path, &request.projection_manifest.sha256, None)?;
    let manifest = oe_core::json::strict_json(&raw)?;
    ensure!(
        manifest["schema"] == "incad.design-projection-manifest/1"
            && manifest["number"] == request.case_number,
        "incad_projection_identity"
    );
    let projection_files = inventory(&manifest)?;
    ensure!(
        projection_files.len() == PROJECTIONS.len()
            && PROJECTIONS
                .iter()
                .all(|n| projection_files.contains_key(*n)),
        "incad_projection_inventory_changed"
    );
    let mut projections = BTreeMap::new();
    for name in PROJECTIONS {
        projections.insert(name, reader.member(folder, &projection_files, name)?);
    }
    let input_bytes = projections.remove("input.json").unwrap();
    let input = oe_core::json::strict_json(&input_bytes)?;
    let source = &manifest["source"];
    closed(
        &input,
        &[
            "schema",
            "number",
            "title",
            "purpose",
            "source",
            "sections",
            "missing_sections",
            "source_claim_status",
            "unit_interpretation",
            "cross_section_consistency",
            "project_binding",
            "engineering_adoption",
            "scope",
        ],
    )?;
    closed(
        source,
        &[
            "id",
            "url",
            "path",
            "offline_page",
            "sha256",
            "bytes",
            "manifest_path",
            "manifest_sha256",
            "case_record_sha256",
            "projection_version",
        ],
    )?;
    for key in ["title", "purpose", "scope"] {
        text(&input, key)?;
    }
    for key in [
        "id",
        "url",
        "path",
        "offline_page",
        "sha256",
        "manifest_path",
        "manifest_sha256",
        "case_record_sha256",
        "projection_version",
    ] {
        text(source, key)?;
    }
    ensure!(source["id"] == "supplier-html", "incad_source_role_changed");
    ensure!(
        input["schema"] == "incad.design-input/1"
            && input["number"] == request.case_number
            && input["source"] == *source,
        "incad_specification_identity"
    );
    ensure!(
        input["project_binding"] == "not_bound"
            && input["engineering_adoption"] == "not_run"
            && input["source_claim_status"] == "supplier_declared"
            && input["unit_interpretation"] == "not_reviewed"
            && input["cross_section_consistency"] == "not_reviewed",
        "incad_specification_authority_changed"
    );
    let sections = input["sections"]
        .as_array()
        .context("incad_specifications_missing")?;
    for section in sections {
        closed(
            section,
            &[
                "heading",
                "html",
                "id",
                "interpretation",
                "kind",
                "locator",
                "source_ref",
                "status",
                "text",
                "text_sha256",
            ],
        )?;
        closed(&section["locator"], &["selector", "ordinal"])?;
        for key in ["heading", "html", "id"] {
            text(section, key)?;
        }
        ensure!(
            section["locator"]["selector"] == "h4.right_ttl"
                && section["locator"]["ordinal"].as_u64().is_some()
                && section["interpretation"] == "not_reviewed",
            "incad_section_locator_or_interpretation_changed"
        );
        ensure!(
            [
                "purpose_action",
                "environment_operations",
                "workpiece",
                "dimensions_motion",
                "precision_load"
            ]
            .contains(&text(section, "kind")?),
            "incad_reference_in_specifications"
        );
        ensure!(
            section["source_ref"] == "supplier-html"
                && section["status"] == "supplier_declared"
                && sha256(text(section, "text")?.as_bytes()) == text(section, "text_sha256")?,
            "incad_section_binding"
        );
    }
    ensure!(
        input["missing_sections"].is_array(),
        "incad_missing_sections_required"
    );
    ensure!(
        input["missing_sections"]
            .as_array()
            .unwrap()
            .iter()
            .all(Value::is_string),
        "incad_missing_sections_type"
    );
    for (name, schema) in [
        ("reference.json", "incad.design-reference/1"),
        ("study.json", "incad.design-study/1"),
        ("original.json", "incad.complete-original/1"),
    ] {
        let v = oe_core::json::strict_json(&projections[name])?;
        ensure!(
            v["schema"] == schema && v["number"] == request.case_number && v["source"] == *source,
            "incad_projection_source_mismatch"
        );
        if name == "original.json" {
            for key in ["primary", "whole_page"] {
                ensure!(
                    sha256(text(&v[key], "verbatim_dom_text")?.as_bytes())
                        == text(&v[key], "text_sha256")?,
                    "incad_complete_text_hash_mismatch"
                );
            }
        }
    }
    let case_manifest_path = path(root, text(source, "manifest_path")?)?;
    let case_folder = case_manifest_path.parent().context("incad_case_folder")?;
    let case_raw = reader.read(&case_manifest_path, text(source, "manifest_sha256")?, None)?;
    let case_manifest = oe_core::json::strict_json(&case_raw)?;
    ensure!(
        case_manifest["schema"] == "incad.full-case-manifest/1"
            && case_manifest["site"] == "cn"
            && case_manifest["number"] == request.case_number,
        "incad_case_manifest_identity"
    );
    ensure!(
        text(source, "url")?
            == format!(
                "https://www.misumi.com.cn/asia/incadlibrary/detail/{}.html",
                request.case_number
            )
            && case_manifest["source_url"] == source["url"],
        "incad_supplier_url_mismatch"
    );
    let files = inventory(&case_manifest)?;
    let html_path = path(root, text(source, "path")?)?;
    ensure!(
        html_path == case_folder.join("source.html"),
        "incad_original_path_mismatch"
    );
    let html = reader.member(case_folder, &files, "source.html")?;
    ensure!(
        sha256(&html) == text(source, "sha256")?
            && source["bytes"].as_u64() == Some(html.len() as u64),
        "incad_original_identity_mismatch"
    );
    let case_bytes = reader.member(case_folder, &files, "case.json")?;
    ensure!(
        sha256(&case_bytes) == text(source, "case_record_sha256")?,
        "incad_case_record_hash_mismatch"
    );
    let case = oe_core::json::strict_json(&case_bytes)?;
    ensure!(
        case["number"] == request.case_number
            && case["site"] == "cn"
            && case["source_sha256"] == source["sha256"],
        "incad_case_record_identity"
    );
    let task_path = Path::new(&request.zero_design_task.path);
    ensure!(task_path.is_absolute(), "incad_task_absolute_path_required");
    let task_bytes = reader.read(task_path, &request.zero_design_task.sha256, None)?;
    let task = oe_core::json::strict_json(&task_bytes)?;
    let keys = [
        "schema",
        "case_number",
        "title",
        "specifications",
        "current_request",
        "preparation_only",
        "project_binding",
        "conditions_confirmation",
        "baseline_exposure",
        "design_execution",
        "engineering_adoption",
        "next_step",
    ];
    let map = task.as_object().context("incad_task_object_required")?;
    ensure!(
        map.len() == keys.len() && keys.iter().all(|k| map.contains_key(*k)),
        "incad_task_shape_changed"
    );
    ensure!(
        task["schema"] == "incad.zero-design-task/1"
            && task["case_number"] == request.case_number
            && task["title"] == input["title"]
            && task["specifications"] == input,
        "incad_task_specifications_changed"
    );
    ensure!(
        task["preparation_only"] == true
            && task["project_binding"] == "not_bound"
            && task["conditions_confirmation"] == "not_run"
            && task["baseline_exposure"] == "development_replay"
            && task["design_execution"] == "not_run"
            && task["engineering_adoption"] == "not_run",
        "incad_task_authority_changed"
    );
    text(&task, "current_request")?;
    text(&task, "next_step")?;
    let roles = BTreeMap::from([
        (
            "supplier_html".into(),
            pin(&html_path, text(source, "sha256")?),
        ),
        (
            "projection_manifest".into(),
            pin(&manifest_path, &request.projection_manifest.sha256),
        ),
        (
            "specifications".into(),
            pin(&folder.join("input.json"), &sha256(&input_bytes)),
        ),
        (
            "task".into(),
            pin(task_path, &request.zero_design_task.sha256),
        ),
    ]);
    let audit = json!({"schema":"oev2.incad-intake-audit/1","case_number":request.case_number,
        "source_integrity":"verified_selected_source_closure","checked_files":reader.sources,
        "roles":roles,"reference_solution":pin(&folder.join("reference.json"),text(projection_files["reference.json"],"sha256")?),
        "complete_original":pin(&folder.join("original.json"),text(projection_files["original.json"],"sha256")?),
        "reference_file_payload_in_design_window":false,"specification_bytes_preserved":true,
        "missing_sections":input["missing_sections"],"archive_status":{"errors":case_manifest["errors"],"offered_formats":case_manifest["offered_formats"],"downloaded_formats":case_manifest["downloaded_formats"],"complete_for_requested_scope":case_manifest["complete_for_requested_scope"],"native_cad_validation":case_manifest["native_cad_validation"]},
        "baseline_exposure":"development_replay","html_extraction_reexecuted":false,"cad_files_revalidated":false,
        "project_mutated":false,"engineering_adoption":"not_assessed"});
    let result = VerifiedIntake {
        input_bytes,
        task_bytes,
        specifications: input,
        task,
        sources: reader.sources,
        roles,
        audit,
        case_folder: case_folder.to_owned(),
        case_manifest,
        case_record: case,
    };
    result.ensure_current()?;
    Ok(result)
}
