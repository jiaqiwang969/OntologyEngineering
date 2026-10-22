//! Read frozen historical chapter assets as data. No Python runtime is imported.
use anyhow::{Context, Result, bail, ensure};
use oe_core::sha256;
use oxigraph::io::RdfFormat;
use semantica_runtime::books::SourceProvider;
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

pub use oe_semantic::books::rust::ARCHIVE;
struct Chapter {
    manifest: Value,
    assets: BTreeMap<String, Vec<u8>>,
    manifest_sha256: String,
}
fn read(root: &Path) -> Result<BTreeMap<String, Chapter>> {
    let provider = oe_semantic::books::rust::ArchiveProvider::open(root)?;
    let catalog = provider.catalog()?;
    let mut out = BTreeMap::new();
    for descriptor in catalog.list_chapter_packages(None) {
        let volume = descriptor["volume"]
            .as_str()
            .context("chapter_volume_missing")?;
        let chapter = descriptor["chapter"]
            .as_str()
            .context("chapter_key_missing")?;
        let key = format!("{volume}/{chapter}");
        let package = descriptor["package_id"]
            .as_str()
            .context("chapter_package_id_missing")?;
        let manifest = catalog.read_manifest(package)?.clone();
        let bytes = provider.read(&format!(
            "semantica/chapter_packages/{}",
            descriptor["manifest"]
                .as_str()
                .context("chapter_manifest_path_missing")?
        ))?;
        let mut assets = BTreeMap::new();
        let mut total = 0;
        for a in manifest["assets"]
            .as_array()
            .context("chapter_assets_missing")?
        {
            let id = a["asset_id"].as_str().context("chapter_asset_id_missing")?;
            let data = catalog.asset(&provider, package, id)?;
            total += data.len();
            ensure!(total <= 32 * 1024 * 1024, "chapter_total_budget");
            ensure!(
                assets.insert(id.into(), data).is_none(),
                "chapter_duplicate_asset"
            );
        }
        out.insert(
            key,
            Chapter {
                manifest,
                assets,
                manifest_sha256: sha256(bytes),
            },
        );
    }
    provider.verify()?;
    Ok(out)
}
fn asset<'a>(c: &'a Chapter, id: &str) -> Result<&'a [u8]> {
    c.assets
        .get(id)
        .map(Vec::as_slice)
        .context("chapter_asset_unknown")
}
fn text<'a>(c: &'a Chapter, id: &str) -> Result<&'a str> {
    Ok(std::str::from_utf8(asset(c, id)?)?)
}
fn base(c: &Chapter) -> Result<Vec<u8>> {
    let mut sources = Vec::new();
    for a in c.manifest["assets"]
        .as_array()
        .context("chapter_assets_missing")?
    {
        if a["load_into_dataset"] == true {
            ensure!(a["kind"] == "rdf", "chapter_loaded_kind_unsupported");
            let format = match a["format"].as_str() {
                Some("turtle") => RdfFormat::Turtle,
                Some("xml" | "rdfxml") => RdfFormat::RdfXml,
                _ => bail!("chapter_loaded_format_unsupported:{}", a["format"]),
            };
            sources.push((
                asset(
                    c,
                    a["asset_id"].as_str().context("chapter_asset_id_missing")?,
                )?,
                format,
            ));
        }
    }
    crate::graph::merge_sources(&sources)
}
fn add(data: &mut Vec<u8>, source: &[u8], format: RdfFormat) -> Result<()> {
    *data = crate::graph::merge_sources(&[(data.as_slice(), RdfFormat::Turtle), (source, format)])?;
    Ok(())
}
fn steps(c: &Chapter, steps: &Value) -> Result<Value> {
    let mut data = base(c)?;
    let mut last = None;
    let steps = steps.as_array().context("chapter_steps_missing")?;
    ensure!(steps.len() <= 32, "chapter_step_budget");
    for s in steps {
        match s["operation"]
            .as_str()
            .context("chapter_operation_missing")?
        {
            "load_package" => ensure!(
                s["package_id"] == c.manifest["package_id"]
                    && s["version"] == c.manifest["version"],
                "chapter_package_binding_mismatch"
            ),
            "load_asset" => {
                let format = match s["format"].as_str() {
                    Some("turtle") => RdfFormat::Turtle,
                    Some("xml" | "rdfxml") => RdfFormat::RdfXml,
                    _ => bail!("chapter_format_unsupported"),
                };
                add(
                    &mut data,
                    asset(c, s["asset_id"].as_str().context("chapter_input_missing")?)?,
                    format,
                )?;
            }
            "select" | "ask" => {
                last = Some(crate::graph::query(
                    &crate::graph::load(&data)?,
                    text(c, s["asset_id"].as_str().context("chapter_query_missing")?)?,
                )?)
            }
            "validate" => {
                last = Some(crate::shacl::validate(
                    &data,
                    asset(c, s["asset_id"].as_str().context("chapter_shape_missing")?)?,
                )?)
            }
            operation => bail!("chapter_operation_unsupported:{operation}"),
        }
    }
    last.context("chapter_no_executable_result")
}
pub fn compare_cq_oracle(actual: &Value, oracle: &Value) -> Result<bool> {
    ensure!(
        oracle["kind"] == "binding_multiset",
        "chapter_cq_oracle_unsupported"
    );
    let mut variables: Vec<String> = serde_json::from_value(oracle["variables"].clone())?;
    let mut columns: Vec<String> = serde_json::from_value(actual["columns"].clone())?;
    variables.sort();
    columns.sort();
    if columns != variables {
        return Ok(false);
    }
    let mut got = Vec::new();
    for r in actual["rows"].as_array().context("chapter_rows_missing")? {
        let mut row = BTreeMap::new();
        for v in &variables {
            let t = &r[v];
            ensure!(t.is_object(), "chapter_unbound_term_unsupported");
            row.insert(v.clone(),json!({"value":t["value"],"term_type":t["kind"],"datatype":t["datatype"],"language":t["language"]}));
        }
        got.push(oe_wire::json::canonical_json(&serde_json::to_value(row)?)?);
    }
    let mut expected = oracle["rows"]
        .as_array()
        .context("chapter_oracle_rows_missing")?
        .iter()
        .map(oe_wire::json::canonical_json)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    got.sort();
    expected.sort();
    Ok(got == expected)
}
pub fn compare_validation_oracle(actual: &Value, oracle: &Value) -> Result<bool> {
    let mut e = serde_json::Map::new();
    for (k, v) in oracle.as_object().context("chapter_shape_oracle_missing")? {
        match k.as_str() {
            "execution" => {}
            "fault_count" => {
                e.insert("violation_count".into(), v.clone());
            }
            "required_message_contains" => {
                e.insert("message_contains".into(), v.clone());
            }
            "conforms" | "violation_count" | "message_contains" => {
                e.insert(k.clone(), v.clone());
            }
            _ => bail!("chapter_shape_oracle_key_unsupported:{k}"),
        }
    }
    crate::packages::matches(actual, &Value::Object(e))
}
pub const INTEROPERABILITY_PROFILE: &str = "oev2.rdf11-legacy-simple-literal/1";
/// This is a second, versioned comparison. Never change the frozen raw oracle
/// or erase numeric datatypes, language tags, duplicate rows or term kinds.
pub fn compare_cq_interoperability(actual: &Value, oracle: &Value) -> Result<bool> {
    let mut rdf11 = oracle.clone();
    for row in rdf11["rows"]
        .as_array_mut()
        .context("chapter_oracle_rows_missing")?
    {
        for term in row
            .as_object_mut()
            .context("chapter_oracle_row_invalid")?
            .values_mut()
        {
            if term["term_type"] == "literal"
                && term["datatype"].is_null()
                && term["language"].is_null()
            {
                term["datatype"] = json!("http://www.w3.org/2001/XMLSchema#string");
            }
        }
    }
    compare_cq_oracle(actual, &rdf11)
}
fn vol1(c: &Chapter, s: &Value, all: &BTreeMap<String, Chapter>) -> Result<Vec<Value>> {
    ensure!(s["oracle"]["status"] == "ready", "chapter_oracle_not_ready");
    let operation = s["execution"]["operation"]
        .as_str()
        .context("chapter_operation_missing")?;
    let inputs = s["inputs"].as_array().context("chapter_inputs_missing")?;
    if operation == "forward_chain" || operation == "composite" {
        ensure!(
            inputs.len() == if operation == "forward_chain" { 2 } else { 5 },
            "chapter_input_arity_unsupported"
        );
        let result = crate::rules::forward_chain(
            text(c, inputs[0].as_str().context("chapter_input_invalid")?)?,
            text(c, inputs[1].as_str().context("chapter_input_invalid")?)?,
        )?;
        let oracle = if operation == "forward_chain" {
            &s["oracle"]["expected_conclusions"]
        } else {
            &s["oracle"]["expected"]["inferred"]
        };
        let mut expected = oracle
            .as_array()
            .context("chapter_rule_oracle_missing")?
            .clone();
        expected.sort_by_key(Value::to_string);
        let mut got = result["conclusions"]
            .as_array()
            .context("chapter_rule_conclusions_missing")?
            .clone();
        got.sort_by_key(Value::to_string);
        let mut cases = vec![
            json!({"role":"forward_chain","execution":"completed","expected":oracle,"result":result,"regression":if got==expected{"pass"}else{"failed"}}),
        ];
        if operation == "composite" {
            let mut data = base(c)?;
            add(
                &mut data,
                asset(c, inputs[2].as_str().context("chapter_input_invalid")?)?,
                RdfFormat::Turtle,
            )?;
            for (n, role) in [(3, "ask_owa_evidence"), (4, "ask_cwa_naf")] {
                let result = crate::graph::query(
                    &crate::graph::load(&data)?,
                    text(c, inputs[n].as_str().context("chapter_input_invalid")?)?,
                )?;
                ensure!(result["kind"] == "ask", "chapter_composite_ask_required");
                cases.push(json!({"role":role,"execution":"completed","expected":s["oracle"]["expected"][role],"regression":if result["value"]==s["oracle"]["expected"][role]{"pass"}else{"failed"},"result":result}));
            }
        }
        return Ok(cases);
    }
    if operation == "select_then_cross_package_validate" {
        ensure!(inputs.len() == 1, "chapter_input_arity_unsupported");
        let mut data = base(c)?;
        add(
            &mut data,
            asset(c, inputs[0].as_str().context("chapter_input_invalid")?)?,
            RdfFormat::Turtle,
        )?;
        let result = crate::graph::query(
            &crate::graph::load(&data)?,
            text(
                c,
                s["execution"]["query_asset_id"]
                    .as_str()
                    .context("chapter_query_missing")?,
            )?,
        )?;
        let mut bindings: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for row in result["rows"].as_array().context("chapter_rows_missing")? {
            for (k, v) in row.as_object().context("chapter_row_invalid")? {
                bindings.entry(k.clone()).or_default().push(
                    v["value"]
                        .as_str()
                        .context("chapter_term_value_missing")?
                        .into(),
                );
            }
        }
        let select = json!({"row_count":result["row_count"],"bindings":bindings});
        let dependency = s["execution"]["shape_dependency"]
            .as_str()
            .context("chapter_shape_dependency_missing")?;
        let (package, id) = dependency
            .rsplit_once(':')
            .context("chapter_shape_dependency_invalid")?;
        let owner = all
            .values()
            .find(|c| c.manifest["package_id"] == package)
            .context("chapter_dependency_unknown")?;
        let validation = crate::shacl::validate(&data, asset(owner, id)?)?;
        return Ok(vec![
            json!({"role":"select","execution":"completed","expected":s["oracle"]["select"],"regression":if crate::packages::matches(&select,&s["oracle"]["select"])?{"pass"}else{"failed"},"result":select}),
            json!({"role":"cross_package_validation","execution":"completed","expected":s["oracle"]["validation"],"regression":if compare_validation_oracle(&validation,&s["oracle"]["validation"])?{"pass"}else{"failed"},"result":validation}),
        ]);
    }
    ensure!(
        ["select_exact_multiset", "validate"].contains(&operation),
        "chapter_operation_unsupported:{operation}"
    );
    ensure!(inputs.len() == 2, "chapter_input_arity_unsupported");
    let mut cases = Vec::new();
    for (n, role) in ["positive", "single_fault_negative"]
        .into_iter()
        .enumerate()
    {
        let input = inputs[n].as_str().context("chapter_input_invalid")?;
        let mut data = base(c)?;
        add(&mut data, asset(c, input)?, RdfFormat::Turtle)?;
        let result = if operation == "validate" {
            crate::shacl::validate(
                &data,
                asset(
                    c,
                    s["execution"]["shape_asset_id"]
                        .as_str()
                        .context("chapter_shape_missing")?,
                )?,
            )?
        } else {
            let (variables, rows) = crate::graph::select_with_columns(
                &crate::graph::load(&data)?,
                text(
                    c,
                    s["execution"]["query_asset_id"]
                        .as_str()
                        .context("chapter_query_missing")?,
                )?,
            )?;
            let mut bindings: BTreeMap<String, Vec<String>> =
                variables.into_iter().map(|v| (v, Vec::new())).collect();
            for r in &rows {
                for (k, v) in r {
                    bindings
                        .entry(k.clone())
                        .or_default()
                        .push(crate::packages::lexical(v));
                }
            }
            json!({"row_count":rows.len(),"bindings":bindings})
        };
        let oracle = &s["oracle"][role];
        let okay = if operation == "validate" {
            compare_validation_oracle(&result, oracle)?
        } else {
            crate::packages::matches(&result, oracle)?
        };
        cases.push(json!({"role":role,"execution":"completed","input_id":input,"input_sha256":sha256(asset(c,input)?),"expected":oracle,"result":result,"regression":if okay{"pass"}else{"failed"}}));
    }
    Ok(cases)
}
fn vol2(c: &Chapter, cq: &Value) -> Vec<Value> {
    let mut results = Vec::new();
    for role in ["cq_exact", "positive_path", "single_fault_negative"] {
        let oracle = &cq["oracles"][role];
        let attempt = (|| -> Result<Value> {
            let result = steps(
                c,
                if role == "cq_exact" {
                    &cq["execution"]["steps"]
                } else {
                    &oracle["execution"]
                },
            )?;
            let okay = if role == "cq_exact" {
                compare_cq_oracle(&result, oracle)?
            } else {
                compare_validation_oracle(&result, oracle)?
            };
            let interoperability = if role == "cq_exact" {
                Some(
                    json!({"profile":INTEROPERABILITY_PROFILE,"regression":if compare_cq_interoperability(&result,oracle)?{"pass"}else{"failed"},"raw_oracle_retained":true}),
                )
            } else {
                None
            };
            Ok(
                json!({"role":role,"execution":"completed","expected":oracle,"result":result,"regression":if okay{"pass"}else{"failed"},"interoperability":interoperability}),
            )
        })();
        results.push(match attempt {Ok(v)=>v,Err(e)=>json!({"role":role,"execution":"unsupported_or_failed","regression":"unknown","reason":format!("{e:#}"),"expected_retained":oracle})});
    }
    results
}
pub fn migration(root: &Path) -> Result<Value> {
    let chapters = read(root)?;
    let mut results = Vec::new();
    for (key, c) in &chapters {
        let mut scenarios = Vec::new();
        if let Some(raw) = c
            .assets
            .get("scenarios")
            .or_else(|| c.assets.get("scenario-registry"))
        {
            let registry = oe_wire::json::strict_json(raw)?;
            for s in registry["scenarios"]
                .as_array()
                .context("chapter_scenarios_missing")?
            {
                scenarios.push(match vol1(c,s,&chapters) {
                    Ok(cases)=>json!({"scenario_id":s["id"],"cases":cases}),
                    Err(e)=>json!({"scenario_id":s["id"],"regression":"unknown","reason":format!("{e:#}"),"scenario_retained":s}),
                });
            }
        } else if let Some(raw) = c.assets.get("cq-registry") {
            let registry = oe_wire::json::strict_json(raw)?;
            for cq in registry["competency_questions"]
                .as_array()
                .context("chapter_cqs_missing")?
            {
                scenarios.push(json!({"scenario_id":cq["scenario_id"],"question":cq["question"],"cases":vol2(&c,cq)}));
            }
        }
        let source = if key.starts_with("vol1") {
            &c.manifest["chapter"]["external_specification"]
        } else {
            &c.manifest["book_source"]
        };
        let anchor = source["source_anchor"]
            .as_str()
            .or_else(|| source["logical_anchor"].as_str());
        let source_check=anchor.map(|p| {
            let p=p.strip_prefix("ontology-engineering/").unwrap_or(p);
            let identity=oe_core::fingerprint_file(&root.join("sources/book-authoring").join(p),16*1024*1024);
            match identity {Ok(i)=>json!({"anchor":p,"expected_sha256":source["source_sha256"].as_str().or_else(||source["sha256"].as_str()),"sha256":i.sha256,"matched":source["source_sha256"]==i.sha256||source["sha256"]==i.sha256}),Err(_)=>json!({"anchor":p,"status":"unavailable"})}
        });
        results.push(json!({"chapter":key,"historical_package_id":c.manifest["package_id"],"historical_version":c.manifest["version"],
            "historical_status":c.manifest["status"],"historical_release_status":c.manifest["release_status"],"manifest_sha256":c.manifest_sha256,
            "asset_count":c.assets.len(),"assets":c.manifest["assets"],"source_check":source_check,"gap_asset":c.manifest["gap_asset"],
            "historical_lifecycle":c.manifest["lifecycle"],"scenarios":scenarios,"engineering_adoption":"not_established"}));
    }
    Ok(
        json!({"schema":"oev2.chapter-migration/1","engine_id":crate::ENGINE_ID,"engine_version":crate::ENGINE_VERSION,
        "executable_sha256":crate::executable_digest()?,"archive":ARCHIVE,"archive_sha256":oe_core::fingerprint_file(&root.join(ARCHIVE),32*1024*1024)?.sha256,
        "chapters":results,"historical_receipts":"not_relabelled","scope":"29 chapter manifests/assets and unchanged available independent oracles; unsupported expressions remain unknown; not whole-book completion"}),
    )
}
pub fn asset_text(root: &Path, volume: &str, chapter: &str, id: &str) -> Result<Value> {
    let mut result = oe_semantic::books::rust::chapter_asset_text(root, volume, chapter, id)?;
    result["package"] = result["package_id"].clone();
    result["source"] = json!("historical_byte_identity");
    result["semantic_execution"] = json!("not_run");
    Ok(result)
}
pub fn inventory(root: &Path, volume: &str) -> Result<Value> {
    ensure!(["vol1", "vol2"].contains(&volume), "chapter_volume_unknown");
    let chapters = read(root)?;
    let rows:Vec<_>=chapters.into_iter().filter(|(key,_)|key.starts_with(volume)).map(|(key,c)|json!({"chapter":key,"manifest_sha256":c.manifest_sha256,"historical_manifest":c.manifest,"asset_identities":c.assets.iter().map(|(id,b)|(id.clone(),sha256(b))).collect::<BTreeMap<_,_>>()})).collect();
    Ok(
        json!({"schema":"oev2.chapter-inventory/1","volume":volume,"chapters":rows,"semantic_execution":"not_run","historical_receipts":"not_relabelled"}),
    )
}
