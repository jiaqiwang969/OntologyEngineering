//! Portable, source-bound inCAD case graphs. No Project, task, CAD execution or adoption.
use super::{VerifiedArchive, inventory, path, text, verify_archive};
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    ArtifactReference, IncadSourceGraph, IncadSourceLibraryManifest, IncadSourceLibraryReport,
    IncadSourceSearchResult, IncadSourceViewResult,
};
use oe_core::{
    FileIdentity, SkillRoot, fingerprint_file, json::strict_json, no_symlinks, read_regular_bytes,
    sha256, write_new_bytes,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};

const MAX_CASES: usize = 1024;
const MAX_CASE_FILES: usize = 8192;
const MAX_FILES: usize = 262144;
const MAX_FILE_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_CASE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_HASH_BYTES: u64 = 128 * 1024 * 1024 * 1024;
const MAX_JSON: usize = 16 * 1024 * 1024;
const MAX_OUTPUT_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const NS: &str = "urn:ontology-engineering:incad:";
const MEANING: &str = "Source-bound supplier statements and archive relationships only. Source bytes, projection text and supplier-declared values are not engineering facts, confirmed causal explanations, native CAD verification or adoption. Known cases remain development_replay. No Project is created; selected current Project use must pass the existing incad preparation contract.";

fn limits() -> Value {
    json!({"max_cases":MAX_CASES,"max_case_files":MAX_CASE_FILES,"max_files":MAX_FILES,"max_file_bytes":MAX_FILE_BYTES,"max_case_bytes":MAX_CASE_BYTES,"max_hash_bytes":MAX_HASH_BYTES,"max_archive_metadata_bytes_per_case":super::MAX_TOTAL*2,"max_archive_metadata_bytes_total":(super::MAX_TOTAL as u64)*2*(MAX_CASES as u64),"max_json_bytes":MAX_JSON,"max_output_bytes":MAX_OUTPUT_BYTES,"search_limit":20,"hash_budget_includes_recheck_and_64kib_growth_headroom":true})
}
fn builder() -> String {
    sha256(format!(
        "{}\n{}\n{}",
        include_str!("library.rs"),
        include_str!("../incad.rs"),
        include_str!("../../../../specs/modules/knowledge-query-planning.toml")
    ))
}
fn relative(root: &Path, p: &Path) -> Result<String> {
    let parts = p
        .strip_prefix(root)
        .context("incad_library_path_outside_root")?
        .components()
        .map(|c| match c {
            Component::Normal(n) => n.to_str().context("incad_library_path_not_utf8"),
            _ => anyhow::bail!("incad_library_path_invalid"),
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(!parts.is_empty(), "incad_library_path_invalid");
    Ok(parts.join("/"))
}
fn location(root: &SkillRoot, value: &Path, index: bool) -> Result<PathBuf> {
    let p = root.expand(value)?;
    let rel = relative(root.path(), &p)?;
    let prefix = if index {
        "sources/.indexes/incad"
    } else {
        "sources/misumi"
    };
    ensure!(
        Path::new(&rel).starts_with(prefix) && (index || rel != prefix),
        "incad_library_path_scope"
    );
    no_symlinks(&p)?;
    Ok(p)
}
fn encode(value: &Value) -> Result<Vec<u8>> {
    let mut b = serde_json::to_vec_pretty(value)?;
    b.push(b'\n');
    ensure!(b.len() <= MAX_JSON, "incad_library_json_budget");
    Ok(b)
}
fn number(value: &Value) -> Result<&str> {
    let n = value.as_str().context("incad_library_case_number")?;
    ensure!(
        n.len() == 6 && n.bytes().all(|x| x.is_ascii_digit()),
        "incad_library_case_number"
    );
    Ok(n)
}
fn id(case: &str, kind: &str, key: &str) -> String {
    format!("{case}:{kind}:{}", &sha256(key)[..24])
}
fn array<'a>(value: &'a Value, code: &str) -> Result<&'a Vec<Value>> {
    value.as_array().with_context(|| code.to_owned())
}

struct Budget {
    files: usize,
    bytes: u64,
}
impl Budget {
    fn new() -> Self {
        Self { files: 0, bytes: 0 }
    }
    fn reserve(&mut self, items: &BTreeMap<PathBuf, FileIdentity>) -> Result<()> {
        ensure!(
            items.len() <= MAX_CASE_FILES,
            "incad_library_case_file_budget"
        );
        let bytes = items.values().try_fold(0u64, |a, f| {
            a.checked_add(f.bytes)
                .context("incad_library_byte_overflow")
        })?;
        ensure!(
            bytes <= MAX_CASE_BYTES && items.values().all(|f| f.bytes <= MAX_FILE_BYTES),
            "incad_library_case_byte_budget"
        );
        self.files = self
            .files
            .checked_add(items.len())
            .context("incad_library_file_budget")?;
        let charge = bytes
            .checked_add((items.len() as u64) * 65536)
            .and_then(|n| n.checked_mul(2))
            .context("incad_library_hash_budget")?;
        self.bytes = self
            .bytes
            .checked_add(charge)
            .context("incad_library_hash_budget")?;
        ensure!(
            self.files <= MAX_FILES && self.bytes <= MAX_HASH_BYTES,
            "incad_library_read_budget"
        );
        Ok(())
    }
}
struct Case {
    graph: Value,
    rdf: Vec<u8>,
    files: BTreeMap<PathBuf, FileIdentity>,
}
impl Case {
    fn verify_files(&self) -> Result<()> {
        for (p, want) in &self.files {
            no_symlinks(p)?;
            let got = fingerprint_file(p, want.bytes)?;
            ensure!(got == *want, "incad_library_source_changed:{}", p.display());
        }
        Ok(())
    }
}
struct Graph {
    nodes: Vec<Value>,
    edges: Vec<Value>,
    ids: BTreeSet<String>,
    case: String,
}
impl Graph {
    fn node(
        &mut self,
        kind: &str,
        key: &str,
        properties: Value,
        refs: Value,
        status: &str,
    ) -> Result<String> {
        let node = id(&self.case, kind, key);
        ensure!(
            self.ids.insert(node.clone()),
            "incad_library_duplicate_node"
        );
        self.nodes.push(json!({"id":node,"kind":kind,"properties":properties,"source_refs":refs,"status":status}));
        Ok(node)
    }
    fn edge(
        &mut self,
        source: &str,
        target: &str,
        relation: &str,
        refs: Value,
        status: &str,
    ) -> Result<()> {
        ensure!(
            self.ids.contains(source) && self.ids.contains(target),
            "incad_library_dangling_edge"
        );
        let eid = id(
            &self.case,
            "edge",
            &format!("{source}\n{relation}\n{target}"),
        );
        ensure!(self.ids.insert(eid.clone()), "incad_library_duplicate_edge");
        self.edges.push(json!({"id":eid,"source":source,"target":target,"relation":relation,"source_refs":refs,"status":status}));
        Ok(())
    }
}
fn source_ref(root: &SkillRoot, p: &Path, sha: &str, pointer: &str) -> Result<Value> {
    Ok(json!({"path":relative(root.path(),p)?,"sha256":sha,"json_pointer":pointer}))
}
fn declared_file(
    root: &SkillRoot,
    archive: &VerifiedArchive,
    catalog: &BTreeMap<String, &Value>,
    name: &str,
    files: &mut BTreeMap<PathBuf, FileIdentity>,
) -> Result<Value> {
    let f = catalog
        .get(name)
        .context("incad_library_member_not_in_manifest")?;
    let p = path(&archive.case_folder, name)?;
    let identity = FileIdentity {
        bytes: f["bytes"].as_u64().context("incad_library_member_bytes")?,
        sha256: text(f, "sha256")?.into(),
    };
    ensure!(
        identity.bytes <= MAX_FILE_BYTES,
        "incad_library_member_byte_budget"
    );
    if let Some(old) = files.insert(p.clone(), identity.clone()) {
        ensure!(old == identity, "incad_library_conflicting_file");
    }
    Ok(json!({"path":relative(root.path(),&p)?,"sha256":identity.sha256,"bytes":identity.bytes}))
}
fn graph_case(
    root: &SkillRoot,
    archive_root: &Path,
    number: &str,
    pin: &ArtifactReference,
) -> Result<Case> {
    let archive = verify_archive(archive_root, number, pin)?;
    let input = &archive.specifications;
    let reference = &archive.projections["reference.json"];
    let study = &archive.projections["study.json"];
    let case = &archive.case_record;
    ensure!(
        reference["bom"] == case["official_bom"] && reference["cad"] == case["cad_downloads"],
        "incad_library_reference_case_mismatch"
    );
    let revision = text(&input["source"], "manifest_sha256")?;
    let case_id = format!("urn:misumi:incad:cn:{number}");
    let graph_id = format!("{case_id}:revision:{revision}:projection:{}", pin.sha256);
    let mut g = Graph {
        nodes: Vec::new(),
        edges: Vec::new(),
        ids: BTreeSet::new(),
        case: graph_id.clone(),
    };
    let input_path = path(archive_root, &pin.path)?
        .parent()
        .context("incad_library_projection_parent")?
        .join("input.json");
    let input_sha = sha256(&archive.input_bytes);
    let case_path = archive.case_folder.join("case.json");
    let case_sha = text(&input["source"], "case_record_sha256")?;
    let root_ref = source_ref(root, &input_path, &input_sha, "")?;
    let case_ref = source_ref(root, &case_path, case_sha, "")?;
    let cn=g.node("SupplierCase","case",json!({"case_id":case_id,"case_number":number,"title":input["title"],"purpose":input["purpose"],"source_revision":revision,"source_url":input["source"]["url"]}),json!([root_ref]),"supplier_declared")?;
    let mut section_ids = BTreeMap::new();
    let mut section_hashes = BTreeMap::new();
    for (i, s) in array(&input["sections"], "incad_library_sections")?
        .iter()
        .enumerate()
    {
        let sid = text(s, "id")?;
        ensure!(
            !section_ids.contains_key(sid),
            "incad_library_duplicate_section"
        );
        let r = source_ref(root, &input_path, &input_sha, &format!("/sections/{i}"))?;
        let kind = if s["kind"] == "purpose_action" {
            "FunctionStatement"
        } else {
            "ConditionStatement"
        };
        let n=g.node(kind,sid,json!({"section_id":sid,"kind":s["kind"],"heading":s["heading"],"text":s["text"],"text_sha256":s["text_sha256"],"html_locator":s["locator"],"original_source":input["source"]}),json!([r]),"supplier_declared")?;
        g.edge(&cn, &n, "hasStatement", json!([r]), "source_relationship")?;
        section_ids.insert(sid.to_owned(), n);
        section_hashes.insert(sid.to_owned(), s["text_sha256"].clone());
    }
    let projection_dir = input_path.parent().expect("input parent");
    let reference_path = projection_dir.join("reference.json");
    let reference_sha = &archive.sources[&reference_path].sha256;
    for (field, kind) in [
        ("sections", "ReferenceStatement"),
        ("idea_notes", "ReferenceStatement"),
        ("component_selection", "ComponentSelectionStatement"),
    ] {
        if let Some(rows) = reference.get(field) {
            for (i, row) in array(rows, "incad_library_reference_rows")?
                .iter()
                .enumerate()
            {
                let r = source_ref(
                    root,
                    &reference_path,
                    reference_sha,
                    &format!("/{field}/{i}"),
                )?;
                if field == "sections" {
                    let body = text(row, "text")?;
                    ensure!(
                        row["text_sha256"] == sha256(body),
                        "incad_library_reference_text_hash"
                    );
                    if let Some(sid) = row["id"].as_str() {
                        ensure!(
                            !section_ids.contains_key(sid),
                            "incad_library_duplicate_section"
                        );
                        section_hashes.insert(sid.into(), row["text_sha256"].clone());
                    }
                }
                let mut props = row.clone();
                if let Some(obj) = props.as_object_mut() {
                    obj.remove("html");
                }
                let n = g.node(
                    kind,
                    &format!("{field}/{i}"),
                    props,
                    json!([r]),
                    "supplier_declared",
                )?;
                if field == "sections" {
                    if let Some(sid) = row["id"].as_str() {
                        section_ids.insert(sid.into(), n.clone());
                    }
                }
                g.edge(&cn, &n, "hasReference", json!([r]), "source_relationship")?;
            }
        }
    }
    let bom = &case["official_bom"];
    let mut bom_models = BTreeMap::<String, Vec<String>>::new();
    if !bom.is_null() {
        let headers = array(&bom["headers"], "incad_library_bom_headers")?;
        ensure!(
            headers.iter().all(Value::is_string) && headers.len() <= 256,
            "incad_library_bom_headers"
        );
        let rows = array(&bom["rows"], "incad_library_bom_rows")?;
        ensure!(
            rows.len() <= 10000 && bom["rows_count"].as_u64() == Some(rows.len() as u64),
            "incad_library_bom_count"
        );
        for (i, row) in rows.iter().enumerate() {
            let cells = array(row, "incad_library_bom_row")?;
            ensure!(
                cells.len() == headers.len() && cells.iter().all(Value::is_string),
                "incad_library_bom_row_shape"
            );
            ensure!(
                cells.first().and_then(Value::as_str) == Some(number),
                "incad_library_bom_case_identity"
            );
            let r = source_ref(
                root,
                &case_path,
                case_sha,
                &format!("/official_bom/rows/{i}"),
            )?;
            let bn = g.node(
                "BomRow",
                &format!("bom/{i}"),
                json!({"row_index":i,"headers":headers,"cells":cells}),
                json!([r]),
                "supplier_declared",
            )?;
            g.edge(&cn, &bn, "hasBomRow", json!([r]), "source_relationship")?;
            // Column meaning is declared by the retained Chinese supplier header.
            if let Some(column) = headers
                .iter()
                .position(|h| matches!(h.as_str(), Some("形式" | "型号" | "型式")))
            {
                let model = cells[column].as_str().expect("string cells");
                let mn = g.node(
                    "ModelDesignation",
                    &format!("bom/{i}/model"),
                    json!({"text":model,"column":column,"entity_equivalence":"not_assessed"}),
                    json!([r]),
                    "supplier_declared",
                )?;
                g.edge(&bn, &mn, "declaresModel", json!([r]), "source_relationship")?;
                bom_models.entry(model.into()).or_default().push(bn);
            }
        }
    }
    let catalog = inventory(&archive.case_manifest)?;
    let mut files = archive.sources.clone();
    for name in catalog.keys().filter(|p| p.starts_with("bom/")) {
        declared_file(root, &archive, &catalog, name, &mut files)?;
    }
    let mut format_ids = BTreeSet::new();
    for (i, cad) in array(&case["cad_downloads"], "incad_library_cad_inventory")?
        .iter()
        .enumerate()
    {
        let code = text(cad, "code")?;
        ensure!(
            !code.is_empty()
                && code.len() <= 64
                && code
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                && format_ids.insert(code),
            "incad_library_cad_format"
        );
        let r = source_ref(root, &case_path, case_sha, &format!("/cad_downloads/{i}"))?;
        let format=g.node("CadFormat",code,json!({"code":code,"supplier_label":cad["label"],"status":cad["status"],"native_validation":"not_run"}),json!([r]),"supplier_declared")?;
        g.edge(
            &cn,
            &format,
            "hasCadFormat",
            json!([r]),
            "source_relationship",
        )?;
        if cad["status"] != "downloaded" {
            continue;
        }
        let original = text(cad, "original")?;
        ensure!(
            original.starts_with(&format!("cad/{code}/")),
            "incad_library_cad_path"
        );
        let f = declared_file(root, &archive, &catalog, original, &mut files)?;
        ensure!(
            f["sha256"] == cad["original_sha256"] && f["bytes"] == cad["original_bytes"],
            "incad_library_cad_archive_identity"
        );
        let an = g.node(
            "CadArchive",
            original,
            f,
            json!([r]),
            "source_identity_declared",
        )?;
        g.edge(
            &format,
            &an,
            "hasArchive",
            json!([r]),
            "source_relationship",
        )?;
        let mut members = BTreeSet::new();
        for (j, member) in array(&cad["members"], "incad_library_cad_members")?
            .iter()
            .enumerate()
        {
            let name = text(member, "path")?;
            ensure!(
                name.starts_with(&format!("cad/{code}/")) && members.insert(name),
                "incad_library_cad_member_path"
            );
            let f = declared_file(root, &archive, &catalog, name, &mut files)?;
            ensure!(
                f["sha256"] == member["sha256"] && f["bytes"] == member["bytes"],
                "incad_library_cad_member_identity"
            );
            let mr = source_ref(
                root,
                &case_path,
                case_sha,
                &format!("/cad_downloads/{i}/members/{j}"),
            )?;
            let mn=g.node("CadMember",name,json!({"file":f,"archive_name":member["archive_name"],"native_geometry":"not_run","bom_correspondence":"unknown"}),json!([mr]),"source_identity_declared")?;
            g.edge(
                &an,
                &mn,
                "declaresMember",
                json!([mr]),
                "source_relationship",
            )?;
        }
    }
    if let Some(paths) = study.get("causal_paths") {
        let study_path = projection_dir.join("study.json");
        let study_sha = &archive.sources[&study_path].sha256;
        for (i, p) in array(paths, "incad_library_teaching_paths")?
            .iter()
            .enumerate()
        {
            ensure!(
                p["relation_status"] == "teaching_reconstruction"
                    && p["causal_validation"] == "not_run"
                    && p["verification_status"] == "not_run",
                "incad_library_teaching_authority"
            );
            let r = source_ref(root, &study_path, study_sha, &format!("/causal_paths/{i}"))?;
            let n = g.node(
                "TeachingRelation",
                &format!("teaching/{i}"),
                p.clone(),
                json!([r]),
                "teaching_reconstruction",
            )?;
            g.edge(
                &cn,
                &n,
                "hasTeachingRelation",
                json!([r]),
                "source_relationship",
            )?;
            for source in array(&p["source_refs"], "incad_library_teaching_sources")? {
                let section = text(source, "section_id")?;
                let target = section_ids
                    .get(section)
                    .context("incad_library_teaching_source_missing")?;
                ensure!(
                    source["text_sha256"] == section_hashes[section],
                    "incad_library_teaching_source_hash"
                );
                g.edge(
                    &n,
                    target,
                    "citesStatement",
                    json!([r]),
                    "teaching_reconstruction",
                )?;
            }
            for row in array(&p["bom_rows"], "incad_library_teaching_bom")? {
                ensure!(
                    array(&bom["rows"], "incad_library_bom_rows")?.contains(row),
                    "incad_library_teaching_bom_mismatch"
                );
            }
        }
    }
    let mut source_files = Vec::new();
    for (p, f) in &files {
        let file = json!({"path":relative(root.path(),p)?,"sha256":f.sha256,"bytes":f.bytes});
        let n = g.node(
            "SourceFile",
            file["path"].as_str().expect("path"),
            file.clone(),
            json!([case_ref]),
            "source_identity_declared",
        )?;
        g.edge(
            &cn,
            &n,
            "hasSourceFile",
            json!([case_ref]),
            "source_relationship",
        )?;
        source_files.push(file);
    }
    let graph = IncadSourceGraph {
        schema: "oev2.incad-source-graph/1".into(),
        case_id,
        case_number: number.into(),
        source_revision: revision.into(),
        title: text(input, "title")?.into(),
        purpose: text(input, "purpose")?.into(),
        nodes: g.nodes,
        edges: g.edges,
        source_files,
        coverage: json!({"title_status":if text(input,"title")?.trim().is_empty(){"missing"}else{"present"},"purpose_status":if text(input,"purpose")?.trim().is_empty(){"missing"}else{"present"},"missing_sections":input["missing_sections"],"source_errors":archive.case_manifest["errors"],"complete_for_requested_scope":archive.case_manifest["complete_for_requested_scope"],"baseline_exposure":"development_replay","html_extraction_reexecuted":false,"claim_atomization":"not_run","bom_cad_correspondence":"unknown","closure":"source/projections plus declared BOM files and downloaded CAD archives/members; other upstream files not required or claimed verified"}),
        native_validation: "not_run".into(),
        engineering_adoption: "not_assessed".into(),
    };
    ensure!(graph.validate().is_ok(), "incad_library_graph_contract");
    let graph = serde_json::to_value(graph)?;
    let rdf = rdf(&graph, &graph_id)?;
    Ok(Case { graph, rdf, files })
}
fn rdf(graph: &Value, name: &str) -> Result<Vec<u8>> {
    let mut out = String::new();
    for node in array(&graph["nodes"], "incad_library_nodes")? {
        let subject = text(node, "id")?;
        out += &format!(
            "<{subject}> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <{NS}{}> <{name}> .\n",
            text(node, "kind")?
        );
        for key in ["status", "properties", "source_refs"] {
            out += &format!(
                "<{subject}> <{NS}{key}> {} <{name}> .\n",
                serde_json::to_string(&serde_json::to_string(&node[key])?)?
            );
        }
        if let Some(props) = node["properties"].as_object() {
            for (key, value) in props {
                if let Some(v) = value.as_str() {
                    ensure!(
                        key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'),
                        "incad_library_rdf_predicate"
                    );
                    out += &format!(
                        "<{subject}> <{NS}{key}> {} <{name}> .\n",
                        serde_json::to_string(v)?
                    );
                }
            }
        }
    }
    for e in array(&graph["edges"], "incad_library_edges")? {
        let (s, t, r) = (text(e, "source")?, text(e, "target")?, text(e, "relation")?);
        out += &format!("<{s}> <{NS}{r}> <{t}> <{name}> .\n");
        out += &format!(
            "<{}> <{NS}assertion> {} <{name}> .\n",
            text(e, "id")?,
            serde_json::to_string(&serde_json::to_string(e)?)?
        );
    }
    ensure!(out.len() <= MAX_JSON, "incad_library_rdf_budget");
    Ok(out.into_bytes())
}

fn resolve_pin(archive: &Path, row: &Value) -> Result<ArtifactReference> {
    let pin: ArtifactReference = serde_json::from_value(row["projection_manifest"].clone())?;
    path(archive, &pin.path)?;
    ensure!(
        crate::source_pack::valid_sha(&pin.sha256),
        "incad_library_projection_sha"
    );
    Ok(pin)
}
fn discover(archive: &Path) -> Result<(PathBuf, Vec<u8>, Vec<Value>)> {
    let index = archive.join("design-inputs/index.json");
    let raw = read_regular_bytes(&index, MAX_JSON)?;
    let v = strict_json(&raw)?;
    ensure!(
        v["schema"] == "incad.design-input-index/1",
        "incad_library_discovery_schema"
    );
    let entries = array(&v["cases"], "incad_library_discovery_cases")?;
    ensure!(
        !entries.is_empty() && entries.len() <= MAX_CASES,
        "incad_library_case_budget"
    );
    let mut ids = BTreeSet::new();
    let mut rows = Vec::new();
    for e in entries {
        let n = number(&e["number"])?;
        ensure!(ids.insert(n.to_owned()), "incad_library_duplicate_case");
        let folder = text(e, "folder")?;
        ensure!(
            folder.starts_with(&format!("design-inputs/cn/{n}/")),
            "incad_library_projection_folder"
        );
        let p = path(archive, folder)?.join("manifest.json");
        let raw = read_regular_bytes(&p, MAX_JSON)?;
        let m = strict_json(&raw)?;
        ensure!(
            m["number"] == n && m["source"]["manifest_sha256"] == e["source_manifest_sha256"],
            "incad_library_discovery_identity"
        );
        rows.push(json!({"number":n,"projection_manifest":{"path":format!("{folder}/manifest.json"),"sha256":sha256(raw)},"case_manifest_sha256":e["source_manifest_sha256"]}));
    }
    rows.sort_by(|a, b| a["number"].as_str().cmp(&b["number"].as_str()));
    Ok((index, raw, rows))
}
fn manifest(root: &SkillRoot, index: &Path) -> Result<(PathBuf, Vec<u8>, Value, PathBuf)> {
    let index = location(root, index, true)?;
    let path = index.join("library.json");
    let raw = read_regular_bytes(&path, MAX_JSON)?;
    let doc = strict_json(&raw)?;
    let typed: IncadSourceLibraryManifest = serde_json::from_value(doc.clone())?;
    ensure!(
        typed.validate().is_ok()
            && typed.schema == "oev2.incad-source-library-manifest/1"
            && typed.builder_sha256 == builder()
            && typed.limits == limits(),
        "incad_library_manifest_identity"
    );
    let archive = location(root, Path::new(&typed.archive_directory), false)?;
    let rows = array(&doc["cases"], "incad_library_manifest_cases")?;
    ensure!(
        !rows.is_empty() && rows.len() <= MAX_CASES,
        "incad_library_case_budget"
    );
    let mut ids = BTreeSet::new();
    for r in rows {
        let n = number(&r["number"])?;
        ensure!(ids.insert(n), "incad_library_duplicate_case");
        resolve_pin(&archive, r)?;
        ensure!(
            r["graph"]["path"] == format!("cases/{n}.json")
                && r["rdf"]["path"] == format!("cases/{n}.nq")
                && crate::source_pack::valid_sha(text(&r["graph"], "sha256")?)
                && crate::source_pack::valid_sha(text(&r["rdf"], "sha256")?),
            "incad_library_graph_reference"
        );
    }
    Ok((index, raw, doc, archive))
}
fn checked_case(
    root: &SkillRoot,
    index: &Path,
    archive: &Path,
    row: &Value,
    budget: &mut Budget,
) -> Result<Case> {
    let n = number(&row["number"])?;
    let pin = resolve_pin(archive, row)?;
    let mut case = graph_case(root, archive, n, &pin)?;
    ensure!(
        case.graph["source_revision"] == row["case_manifest_sha256"]
            && case.graph["title"] == row["title"]
            && case.graph["purpose"] == row["purpose"],
        "incad_library_case_identity"
    );
    let graph_path = path(index, text(&row["graph"], "path")?)?;
    let raw = read_regular_bytes(&graph_path, MAX_JSON)?;
    ensure!(
        sha256(&raw) == row["graph"]["sha256"] && strict_json(&raw)? == case.graph,
        "incad_library_graph_changed"
    );
    let rdf_path = path(index, text(&row["rdf"], "path")?)?;
    let rdf = read_regular_bytes(&rdf_path, MAX_JSON)?;
    ensure!(
        sha256(&rdf) == row["rdf"]["sha256"] && rdf == case.rdf,
        "incad_library_rdf_changed"
    );
    // Recheck the derived records with returned/full source closures. These
    // are internal snapshots and never become original source graph nodes.
    for (path, bytes) in [(graph_path, raw), (rdf_path, rdf)] {
        case.files.insert(
            path,
            FileIdentity {
                bytes: bytes.len() as u64,
                sha256: sha256(bytes),
            },
        );
    }
    budget.reserve(&case.files)?;
    Ok(case)
}
fn final_files(files: &BTreeMap<PathBuf, FileIdentity>) -> Result<()> {
    for (p, want) in files {
        ensure!(
            fingerprint_file(p, want.bytes)? == *want,
            "incad_library_source_changed:{}",
            p.display()
        );
    }
    Ok(())
}
fn extend_files(
    target: &mut BTreeMap<PathBuf, FileIdentity>,
    source: &BTreeMap<PathBuf, FileIdentity>,
) -> Result<()> {
    for (p, f) in source {
        if let Some(old) = target.insert(p.clone(), f.clone()) {
            ensure!(old == *f, "incad_library_conflicting_file");
        }
    }
    Ok(())
}
fn summary(graph: &Value) -> Value {
    json!({"case_number":graph["case_number"],"case_id":graph["case_id"],"title":graph["title"],"purpose":graph["purpose"],"source_revision":graph["source_revision"],"node_count":graph["nodes"].as_array().map_or(0,Vec::len),"edge_count":graph["edges"].as_array().map_or(0,Vec::len),"coverage":graph["coverage"]})
}
fn report(
    root: &SkillRoot,
    index: &Path,
    archive: &Path,
    status: &str,
    cases: Vec<Value>,
    budget: &Budget,
) -> Result<Value> {
    let mut limits = limits();
    limits["charged_hash_bytes"] = json!(budget.bytes);
    limits["selected_file_count"] = json!(budget.files);
    let r = IncadSourceLibraryReport {
        schema: "oev2.incad-source-library/1".into(),
        status: status.into(),
        index_directory: relative(root.path(), index)?,
        archive_directory: relative(root.path(), archive)?,
        case_count: cases.len() as u64,
        case_summaries: cases,
        limits,
        meaning: MEANING.into(),
    };
    ensure!(r.validate().is_ok(), "incad_library_report_contract");
    Ok(serde_json::to_value(r)?)
}

/// Build a new immutable source graph snapshot, preserving originals and older indexes.
pub fn build(root: &SkillRoot, archive: &Path, index: &Path) -> Result<Value> {
    let archive = location(root, archive, false)?;
    let index = location(root, index, true)?;
    ensure!(!index.try_exists()?, "incad_library_output_exists");
    let (discovery, raw, rows) = discover(&archive)?;
    let stage = crate::source_pack::Stage::new(root.path(), "incad-library")?;
    fs::create_dir(stage.path.join("cases"))?;
    let mut budget = Budget::new();
    let mut documents = Vec::new();
    let mut summaries = Vec::new();
    let mut allfiles = BTreeMap::new();
    let mut output_bytes = 0u64;
    for mut row in rows {
        let n = number(&row["number"])?.to_owned();
        let pin = resolve_pin(&archive, &row)?;
        let case = graph_case(root, &archive, &n, &pin)?;
        budget.reserve(&case.files)?;
        case.verify_files()?;
        ensure!(
            case.graph["source_revision"] == row["case_manifest_sha256"],
            "incad_library_discovery_identity"
        );
        let graph = encode(&case.graph)?;
        output_bytes = output_bytes
            .checked_add((graph.len() + case.rdf.len()) as u64)
            .context("incad_library_output_budget")?;
        ensure!(
            output_bytes <= MAX_OUTPUT_BYTES,
            "incad_library_output_budget"
        );
        let gp = format!("cases/{n}.json");
        let rp = format!("cases/{n}.nq");
        write_new_bytes(&stage.path.join(&gp), &graph)?;
        write_new_bytes(&stage.path.join(&rp), &case.rdf)?;
        row["graph"] = json!({"path":gp,"sha256":sha256(&graph)});
        row["rdf"] = json!({"path":rp,"sha256":sha256(&case.rdf)});
        row["title"] = case.graph["title"].clone();
        row["purpose"] = case.graph["purpose"].clone();
        summaries.push(summary(&case.graph));
        documents.push(row);
        extend_files(&mut allfiles, &case.files)?;
    }
    let doc = IncadSourceLibraryManifest {
        schema: "oev2.incad-source-library-manifest/1".into(),
        archive_directory: relative(root.path(), &archive)?,
        builder_sha256: builder(),
        cases: documents,
        limits: limits(),
        meaning: MEANING.into(),
    };
    ensure!(doc.validate().is_ok(), "incad_library_manifest_contract");
    write_new_bytes(
        &stage.path.join("library.json"),
        &encode(&serde_json::to_value(doc)?)?,
    )?;
    final_files(&allfiles)?;
    ensure!(
        read_regular_bytes(&discovery, MAX_JSON)? == raw,
        "incad_library_discovery_changed"
    );
    let parent = index.parent().context("incad_library_index_parent")?;
    oe_core::ensure_directory(parent)?;
    oe_core::rename_directory_exclusive(&stage.path, &index)?;
    report(root, &index, &archive, "built", summaries, &budget)
}

/// Rebuild and compare the graph from selected fixed sources, then rehash its source closure.
pub fn verify(root: &SkillRoot, index: &Path) -> Result<Value> {
    let (index, raw, doc, archive) = manifest(root, index)?;
    let mut budget = Budget::new();
    let mut summaries = Vec::new();
    let mut allfiles = BTreeMap::new();
    for row in array(&doc["cases"], "incad_library_manifest_cases")? {
        let case = checked_case(root, &index, &archive, row, &mut budget)?;
        case.verify_files()?;
        extend_files(&mut allfiles, &case.files)?;
        summaries.push(summary(&case.graph));
    }
    final_files(&allfiles)?;
    ensure!(
        read_regular_bytes(&index.join("library.json"), MAX_JSON)? == raw,
        "incad_library_index_changed"
    );
    report(root, &index, &archive, "verified", summaries, &budget)
}

/// Search source text and identifiers; every returned case is independently source-verified.
pub fn search(root: &SkillRoot, index: &Path, query: &str, limit: usize) -> Result<Value> {
    ensure!(
        (1..=20).contains(&limit) && !query.trim().is_empty() && query.chars().count() <= 1000,
        "incad_library_query_limits"
    );
    let (index, raw, doc, archive) = manifest(root, index)?;
    let terms = query
        .split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>();
    let mut budget = Budget::new();
    let mut results = Vec::new();
    let mut allfiles = BTreeMap::new();
    for row in array(&doc["cases"], "incad_library_manifest_cases")? {
        let case = checked_case(root, &index, &archive, row, &mut budget)?;
        let mut fields = BTreeMap::new();
        fields.insert(
            "case_number",
            case.graph["case_number"].as_str().unwrap_or("").to_owned(),
        );
        fields.insert(
            "title",
            case.graph["title"].as_str().unwrap_or("").to_owned(),
        );
        fields.insert(
            "purpose",
            case.graph["purpose"].as_str().unwrap_or("").to_owned(),
        );
        let source_text = case.graph["nodes"]
            .as_array()
            .expect("nodes")
            .iter()
            .filter(|n| {
                matches!(
                    n["kind"].as_str(),
                    Some(
                        "FunctionStatement"
                            | "ConditionStatement"
                            | "ReferenceStatement"
                            | "ComponentSelectionStatement"
                            | "BomRow"
                            | "ModelDesignation"
                    )
                )
            })
            .map(|n| n["properties"].to_string())
            .collect::<Vec<_>>()
            .join("\n");
        fields.insert("source_statements_and_bom", source_text);
        let joined = fields
            .values()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
            .to_lowercase();
        if terms.iter().all(|t| joined.contains(t)) && results.len() < limit {
            case.verify_files()?;
            extend_files(&mut allfiles, &case.files)?;
            let mut r = summary(&case.graph);
            r["matched_fields"] = json!(
                fields
                    .iter()
                    .filter(|(_, v)| terms.iter().any(|t| v.to_lowercase().contains(t)))
                    .map(|(k, _)| *k)
                    .collect::<Vec<_>>()
            );
            r["cad_byte_revalidation"] = json!("verified");
            r["graph"] = case.graph;
            results.push(r);
        }
    }
    final_files(&allfiles)?;
    ensure!(
        read_regular_bytes(&index.join("library.json"), MAX_JSON)? == raw,
        "incad_library_index_changed"
    );
    let result = IncadSourceSearchResult {
        schema: "oev2.incad-source-search/1".into(),
        status: "candidates".into(),
        query: query.into(),
        coverage: json!({"declarations_rebuilt":doc["cases"].as_array().expect("cases").len(),"returned_source_closures_verified":results.len(),"unreturned_cad_byte_revalidation":"not_run"}),
        results,
        meaning: format!(
            "{MEANING} Lexical matching only; returned source closures were rehashed. Limit/absence of matches does not establish absence of engineering information."
        ),
    };
    ensure!(result.validate().is_ok(), "incad_library_search_contract");
    Ok(serde_json::to_value(result)?)
}

pub fn view(root: &SkillRoot, index: &Path, case_number: &str) -> Result<Value> {
    number(&json!(case_number))?;
    let (index, raw, doc, archive) = manifest(root, index)?;
    let row = array(&doc["cases"], "incad_library_manifest_cases")?
        .iter()
        .find(|r| r["number"] == case_number)
        .context("incad_library_case_not_found")?;
    let mut budget = Budget::new();
    let case = checked_case(root, &index, &archive, row, &mut budget)?;
    case.verify_files()?;
    final_files(&case.files)?;
    ensure!(
        read_regular_bytes(&index.join("library.json"), MAX_JSON)? == raw,
        "incad_library_index_changed"
    );
    let result = IncadSourceViewResult {
        schema: "oev2.incad-source-view/1".into(),
        status: "source_verified".into(),
        case_number: case_number.into(),
        graph: serde_json::from_value(case.graph)?,
        meaning: MEANING.into(),
    };
    ensure!(result.validate().is_ok(), "incad_library_view_contract");
    Ok(serde_json::to_value(result)?)
}
