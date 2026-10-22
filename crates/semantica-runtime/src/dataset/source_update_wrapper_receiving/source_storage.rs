//! Input-driven baseline and source storage receiving. No oracle supplies state.
//! Author exposure is development_replay; the executable reads outputs only
//! after all four cases and all49 actions, including errors, have finished.
use super::*;
use std::path::{Path,PathBuf};

const INPUTS: [&str;2] = ["source-storage-09/INPUTS-41.json", "source-storage-09/INPUTS-8.json"];
const CASES: [&str;4] = ["source_alias_update_roundtrip", "graph_selection_and_refusal",
    "generalized_source_refusal_boundary", "source_diff_single_line_discrimination"];

fn captured<'a>(captures: &'a BTreeMap<String,Value>, name: &Value) -> Result<&'a Value> {
    captures.get(name.as_str().context("capture_name")?).context("producer_capture_unavailable")
}
fn format(a: &Value) -> Result<RdfFormat> {
    Ok(match text(a,"format")? { "nt" => RdfFormat::NTriples, "nquads" => RdfFormat::NQuads,
        _ => anyhow::bail!("frozen_storage_format_unreceived") })
}
fn opt<'a>(a: &'a Value,k: &str) -> Result<Option<&'a str>> {
    match a.get(k) {None|Some(Value::Null)=>Ok(None),Some(Value::String(v))=>Ok(Some(v)),_=>anyhow::bail!("non_string:{k}")}
}
fn storage_action(session: &mut DatasetSession, a: &Value, captures: &BTreeMap<String,Value>,
    warnings: &mut Vec<Value>) -> Result<Value> {
    use crate::dataset::source_storage::SourceSnapshotInput;
    use semantica_contracts::{SemanticSourceSerializeRequest,SemanticSourceSnapshotArgument};
    if text(a,"operation")?=="serialize" {
        let output=session.serialize_source_nt_nq_internal(&SemanticSourceSerializeRequest {
            format:text(a,"format")?.into(),base_uri:opt(a,"base_uri")?.map(str::to_owned),
            graph_name:opt(a,"graph_name")?.map(str::to_owned),
        })?;
        *warnings=output.warnings.into_iter().map(|message| json!({"module":"builtins","type":"UserWarning","message":message})).collect();
        return Ok(json!(output.text));
    }
    let argument = |capture:&str,replace:&str| -> Result<Option<SemanticSourceSnapshotArgument>> {
        let Some(name)=a.get(capture) else {return Ok(None)};
        let mut value=captured(captures,name)?.clone();
        if let Some(fields)=a.get(replace).and_then(Value::as_object) {
            ensure!(fields.len()==1 && fields.keys().all(|k|matches!(k.as_str(),"algorithm"|"dataset_sha256")),"frozen_DTO_replacement_domain");
            for (k,v) in fields {value[k]=v.clone();}
        }
        let argument:SemanticSourceSnapshotArgument=serde_json::from_value(value)?;
        argument.validate().map_err(|e|anyhow::anyhow!("input_DTO_construction:{e:?}"))?;
        Ok(Some(argument))
    };
    let before=argument("before_capture","before_dto_replace")?;
    let after=argument("after_capture","after_dto_replace")?;
    let before=before.as_ref().map(SourceSnapshotInput::Snapshot).unwrap_or(SourceSnapshotInput::NotSnapshot);
    let after=after.as_ref().map(SourceSnapshotInput::Snapshot);
    let result=session.diff_source_internal(before,after,text(a,"created_at")?)?;
    Ok(serde_json::to_value(result)?)
}
fn operation(session: &mut DatasetSession, a: &Value, captures: &BTreeMap<String,Value>,
    warnings: &mut Vec<Value>) -> Result<Value> {
    Ok(match text(a,"operation")? {
        "load" => {
            let data = if a.get("from_saved_bytes").is_some() {
                captured(captures,&a["from_saved_bytes"])?["reread_utf8"].as_str().context("saved_capture_text_missing")?
            } else { text(a,"data")? };
            serde_json::to_value(session.load_rdflib_with_base(data.as_bytes(),format(a)?,opt(a,"base_uri")?,opt(a,"graph_name")?)?)?
        },
        "snapshot" => serde_json::to_value(session.snapshot_rdflib(text(a,"created_at")?)?)?,
        "update" => {
            let mut loader=SourceUpdateBytesLoader::new(BTreeMap::new(),1024*1024,100_000)?;
            let r=session.update_source_full_internal(&a["query"],&BTreeMap::new(),&BTreeMap::new(),&mut loader)?;
            json!({"added":r.added,"removed":r.removed,"quad_count":r.quad_count,"revision":r.revision})
        },
        "select" => {
            let r=session.helper_source_internal("select",text(a,"query")?,&BTreeMap::new(),&BTreeMap::new(),
                a.get("use_cache").and_then(Value::as_bool).unwrap_or(true))?;
            let mut v=r.result;v["revision"]=json!(r.revision);v["cache_hit"]=json!(r.cache_hit);v
        },
        "serialize"|"diff" => storage_action(session,a,captures,warnings)?,
        v=>anyhow::bail!("storage_action_unreceived:{v}"),
    })
}

// Caller transport in a fresh private TempDir. This is not a runtime save API.
fn transport(root:&Path, a:&Value, captures:&BTreeMap<String,Value>) -> Result<Value> {
    use std::{fs::OpenOptions,io::{Read,Write}};
    let value=captured(captures,&a["from_capture"])?.as_str().context("producer_not_serialized_string")?;
    ensure!(value.len()<=262144,"transport_budget");
    let name=text(a,"relative_path")?;
    ensure!(Path::new(name).file_name().and_then(|v|v.to_str())==Some(name) && name!="." && name!="..","transport_path");
    let path=root.join(name);let mut options=OpenOptions::new();options.create_new(true).write(true);
    #[cfg(unix)] {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW|libc::O_CLOEXEC);
    }
    let mut file=options.open(&path)?;file.write_all(value.as_bytes())?;file.sync_all()?;drop(file);
    ensure!(path.symlink_metadata()?.file_type().is_file(),"transport_not_regular");
    let mut read=OpenOptions::new();read.read(true);
    #[cfg(unix)] {
        use std::os::unix::fs::OpenOptionsExt;
        read.custom_flags(libc::O_NOFOLLOW|libc::O_NONBLOCK|libc::O_CLOEXEC);
    }
    let mut bytes=Vec::new();read.open(&path)?.take(262145).read_to_end(&mut bytes)?;
    ensure!(bytes==value.as_bytes(),"transport_exact_reread_failed");
    Ok(json!({"artifact":{"path":path,"bytes":bytes.len(),"sha256":crate::sha256(&bytes)},
        "exact_bytes_reread":true,"reread_utf8":String::from_utf8(bytes)?}))
}

fn execute_all(candidate: bool) -> Result<Vec<Value>> {
    let base=PathBuf::from(std::env::var_os("OE_SOURCE_STORAGE_OUTPUT_ROOT").context("explicit storage test output root required")?);
    ensure!(base.is_absolute(),"absolute output root required");
    let private=base.join(if candidate {"candidate"} else {"baseline"});std::fs::create_dir(&private)?;let mut result=Vec::new();
    for input in INPUTS {
        let input:Value=serde_json::from_str(&crate::private_test_fixtures::text(input))?;
        for case in input["cases"].as_array().context("cases")? {
            let case_id=text(case,"id")?;let directory=private.join(case_id);std::fs::create_dir(&directory)?;
            let mut sessions=BTreeMap::<String,DatasetSession>::new();let mut captures=BTreeMap::<String,Value>::new();
            let mut actions=Vec::new();
            for a in case["actions"].as_array().context("actions")? {
                let op=text(a,"operation")?;let name=text(a,"session")?;
                let mut row=json!({"id":a["id"],"operation":op,"session":name,"warnings":[]});
                let missing=!candidate && matches!(op,"serialize"|"diff");
                let dependency = if op=="transport_save_reread" {a.get("from_capture")}
                    else if op=="load" {a.get("from_saved_bytes")} else {None};
                if dependency.is_some_and(|v| captured(&captures,v).is_err()) {
                    row["status"]=json!("blocked_not_run");row["wrapper_called"]=json!(false);
                    row["phase"]=json!("dependency_check");
                } else if missing {
                    // The old Rust source lane has no serializer/diff entry.
                    // Never call the native writer and sign its result as source.
                    row["status"]=json!("missing_source_entry");row["wrapper_called"]=json!(false);
                    row["phase"]=json!("entry_lookup");
                } else {
                    let mut warnings=Vec::new();
                    let r=if op=="new" {
                        ensure!(text(a,"backend")?=="rdflib","frozen_backend_input");
                        let cache=a["cache"].as_bool().context("cache_input")?;
                        ensure!(!sessions.contains_key(name),"new_session_exists");
                        sessions.insert(name.into(),DatasetSession::new_source_update_internal(cache));
                        Ok(json!({"profile":"ontology-runtime","cache_queries":cache,"backend":"rdflib"}))
                    } else if op=="transport_save_reread" {transport(&directory,a,&captures)}
                    else {operation(sessions.get_mut(name).context("session_missing")?,a,&captures,&mut warnings)};
                    row["wrapper_called"]=json!(op!="transport_save_reread");
                    if op=="transport_save_reread" {row.as_object_mut().unwrap().remove("warnings");}
                    else {row["warnings"]=json!(warnings);}
                    match r {
                        Ok(raw) => {
                            if let Some(capture)=opt(a,"capture")? {captures.insert(capture.into(),raw.clone());}
                            row["raw"]=raw;row["status"]=json!(if op=="transport_save_reread" {"completed_transport"} else {"completed"});row["phase"]=json!(if op=="transport_save_reread" {"caller_transport"} else {"result_capture"});
                        },
                        Err(e) => {
                            row["phase"]=json!("original_wrapper");
                            if let Some(e)=e.downcast_ref::<SourceFailure>() {
                                row["status"]=json!("original_error");row["error"]=json!({"module":e.module,"type":e.class,"message":e.to_string()});
                            } else {row["status"]=json!("unreceived_Rust_error");row["diagnostic"]=json!(format!("{e:#}"));}
                        }
                    }
                }
                let mut states=BTreeMap::new();
                for (id,s) in &mut sessions {states.insert(id.clone(),state(s)?);}
                row["states"]=json!(states);actions.push(row);
            }
            result.push(json!({"case_id":case_id,"actions":actions}));
        }
    }
    Ok(result)
}

#[test]
fn source_storage_baseline_records_actual49_and_missing_source_entries() {
    let result=execute_all(false).unwrap();
    assert_eq!(result.iter().map(|v| v["case_id"].as_str().unwrap()).collect::<Vec<_>>(),CASES);
    let rows:Vec<_>=result.iter().flat_map(|v|v["actions"].as_array().unwrap()).collect();
    let counts=rows.iter().fold(BTreeMap::<String,usize>::new(),|mut m,r| { *m.entry(r["status"].as_str().unwrap().into()).or_default()+=1;m });
    println!("SOURCE_STORAGE_BASELINE={}",json!({"cases":result,"counts":counts,"engine":"Rust existing private source UPDATE V3","oracle_read":false}));
    assert_eq!(rows.len(),49);assert_eq!(counts.get("missing_source_entry"),Some(&16));assert_eq!(counts.get("blocked_not_run"),Some(&6));
}

fn comparable(row: &Value) -> Value {
    let mut fields=serde_json::Map::new();
    for key in ["id","operation","session","phase","wrapper_called","status","raw","error","warnings","states"] {
        if let Some(value)=row.get(key) {fields.insert(key.into(),value.clone());}
    }
    if row["operation"]=="transport_save_reread" && row["status"]=="completed_transport" {
        // Exact bytes/SHA/readback stay compared; independent run roots differ.
        fields["raw"].as_object_mut().unwrap().remove("reread_utf8");
        let name=Path::new(fields["raw"]["artifact"]["path"].as_str().unwrap()).file_name().unwrap().to_str().unwrap().to_owned();
        fields["raw"]["artifact"]["path"]=json!(name);
    }
    Value::Object(fields)
}

#[test]
fn source_storage_receives_frozen49() {
    let result=execute_all(true).unwrap();
    println!("SOURCE_STORAGE_ACTUAL={}",json!({"cases":result,"author_exposure":"development_replay","engine":"Rust existing source V3; constructor RDFLib label is frozen input-domain metadata, not execution backend","all49_finished_before_oracle_read":true}));
    // First output/qualification read occurs only here, after all49 real actions.
    let review:Value=serde_json::from_str(&crate::private_test_fixtures::text("source-storage-09/ORIGINAL-REVIEW.json")).unwrap();
    assert_eq!(review["status"],"original_observations_qualified_for_explicit_scope_not_rust_acceptance");
    assert_eq!(review["totals"]["actions_recorded"],49);
    let mut comparison=Vec::new();let mut actual_count=0;
    for (actual,case_id) in result.iter().zip(CASES) {
        assert_eq!(actual["case_id"],case_id);
        let oracle:Value=serde_json::from_str(&crate::private_test_fixtures::text(&format!("source-storage-09/{case_id}.json"))).unwrap();
        let actual=actual["actions"].as_array().unwrap();let expected=oracle["actions"].as_array().unwrap();assert_eq!(actual.len(),expected.len());
        for (actual,expected) in actual.iter().zip(expected) {
            actual_count+=1;let a=comparable(actual);let e=comparable(expected);
            comparison.push(json!({"id":actual["id"],"matched":a==e,"actual":a,"expected":e}));
        }
    }
    println!("SOURCE_STORAGE_COMPARISON={}",json!({"comparisons":comparison,"actions":actual_count,"comparison_contract":"strict selected full source fields, ordered SELECT and every session state; caller transport absolute root is mapped to fixed relative file role while bytes/hash/readback remain exact; runtime environment/profile versions are separately recorded, not pretended equal"}));
    assert_eq!(actual_count,49);
    let failed:Vec<_>=comparison.iter().filter(|r|r["matched"]!=true).map(|r|r["id"].clone()).collect();
    assert!(failed.is_empty(),"source storage mismatches: {failed:?}");
}

#[test]
fn source_storage_native_guard_and_old_output_contract_stay_separate() {
    use semantica_contracts::{SemanticSourceSerializeRequest,SemanticSourceSnapshotArgument};
    let mut s=DatasetSession::new_source_update_internal(true);
    let mut loader=SourceUpdateBytesLoader::new(BTreeMap::new(),4096,100).unwrap();
    s.update_source_full_internal(&json!("INSERT DATA {?slot <urn:p> \"source\"}"),&BTreeMap::new(),&BTreeMap::new(),&mut loader).unwrap();
    let request=SemanticSourceSerializeRequest{format:"nquads".into(),base_uri:None,graph_name:None};
    assert!(s.serialize(RdfFormat::NQuads,None).is_err());
    assert!(s.serialize_source_nt_nq_internal(&request).unwrap().text.contains("?slot <urn:p>"));
    let snapshot=s.snapshot_rdflib("fixed").unwrap();let mut old=snapshot.clone();old.algorithm="semantica-rdf-dataset-c14n-v1".into();
    assert!(old.validate().is_err());
    let argument:SemanticSourceSnapshotArgument=serde_json::from_value(serde_json::to_value(old).unwrap()).unwrap();assert!(argument.validate().is_ok());
    assert!(DatasetSession::new().serialize_source_nt_nq_internal(&request).is_err());
    assert!(DatasetSession::new_source_data_internal(true).serialize_source_nt_nq_internal(&request).is_err());
}

#[test]
fn source_storage_implicit_diff_and_missing_graph_preserve_cache() {
    use crate::dataset::source_storage::{SourceSnapshotInput,source_snapshot_argument};
    use semantica_contracts::SemanticSourceSerializeRequest;
    let mut s=DatasetSession::new_source_update_internal(true);
    s.load_rdflib_with_base(b"<urn:s> <urn:p> \"v\" .\n",RdfFormat::NTriples,None,None).unwrap();
    s.helper_source_internal("ask","ASK {<urn:s> <urn:p> ?o}",&BTreeMap::new(),&BTreeMap::new(),true).unwrap();
    let snap=s.snapshot_rdflib("fixed").unwrap();let before=state(&mut s).unwrap();
    let r=s.serialize_source_nt_nq_internal(&SemanticSourceSerializeRequest {format:"nt".into(),base_uri:Some("urn:base:".into()),graph_name:Some("urn:missing".into())}).unwrap();
    assert_eq!(r.text,"");assert_eq!(r.warnings,vec!["NTSerializer does not support base."]);
    let after=state(&mut s).unwrap();
    for key in ["revision","query_cache_entries","snapshot_cached","quad_count","quads"] {assert_eq!(before[key],after[key],"{key}");}
    assert!(after["contexts"].as_array().unwrap().iter().any(|c|c["value"]=="urn:missing"));
    let arg=source_snapshot_argument(&snap);let diff=s.diff_source_internal(SourceSnapshotInput::Snapshot(&arg),None,"diff-time").unwrap();
    assert_eq!(diff.unchanged_count,1);assert!(!diff.changed);assert_eq!(diff.created_at,"diff-time");assert!(s.rdflib_snapshot_cache.get().is_some());
    let cold=DatasetSession::new_source_update_internal(true);
    assert!(cold.diff_source_internal(SourceSnapshotInput::NotSnapshot,None,"fixed").is_err());assert!(cold.rdflib_snapshot_cache.get().is_none());
    assert!(cold.diff_source_internal(SourceSnapshotInput::Snapshot(&arg),Some(SourceSnapshotInput::NotSnapshot),"fixed").is_err());assert!(cold.rdflib_snapshot_cache.get().is_none());
}

#[test]
fn source_storage_splitlines_preserves_python_boundaries() {
    use crate::dataset::source_storage::{splitlines,SourceSnapshotInput};
    use semantica_contracts::SemanticSourceSnapshotArgument;
    assert_eq!(splitlines("a\r\nb\rc\nd\u{b}e\u{c}f\u{1c}g\u{1d}h\u{1e}i\u{85}j\u{2028}k\u{2029}"),["a","b","c","d","e","f","g","h","i","j","k"]);
    assert!(splitlines("").is_empty());assert_eq!(splitlines("\n"),[""]);assert_eq!(splitlines("a\r\n"),["a"]);
    let text="<urn:s> <urn:p> \"\"\"a\nb\"\"\" .";
    let arg=SemanticSourceSnapshotArgument {schema_version:"1.0".into(),algorithm:"semantica-rdf-dataset-c14n-v2".into(),dataset_sha256:crate::sha256(text),quad_count:1,revision:0,created_at:"fixed".into(),canonical_nquads:text.into()};
    let s=DatasetSession::new_source_update_internal(true);
    let error=s.diff_source_internal(SourceSnapshotInput::Snapshot(&arg),Some(SourceSnapshotInput::Snapshot(&arg)),"fixed").unwrap_err();
    let e=error.downcast_ref::<SourceFailure>().unwrap();assert_eq!(e.class,"SemanticInputError");assert_eq!(e.to_string(),"cannot diff a dataset snapshot with invalid integrity");
}

#[test]
fn source_storage_load_subject_errors_are_typed_and_atomic() {
    // Independent source-derived controls, not additional original observations.
    let mut s=DatasetSession::new_source_update_internal(true);
    s.load_rdflib_with_base(b"<urn:seed> <urn:p> <urn:o> .\n",RdfFormat::NQuads,None,None).unwrap();
    s.helper_source_internal("ask","ASK {<urn:seed> <urn:p> ?o}",&BTreeMap::new(),&BTreeMap::new(),true).unwrap();
    s.snapshot_rdflib("fixed").unwrap();let before=state(&mut s).unwrap();
    for bad in ["?other <urn:p> <urn:o> .", "\"literal subject\" <urn:p> <urn:o> .", "22 <urn:p> <urn:o> .", "  ?indented <urn:p> <urn:o> ."] {
        let data=format!("<urn:other> <urn:p> <urn:o> .\r\n{bad}\r\n");
        let error=s.load_rdflib_with_base(data.as_bytes(),RdfFormat::NQuads,None,None).unwrap_err();
        let e=error.downcast_ref::<SourceFailure>().unwrap_or_else(||panic!("unreceived actual parser error: {error:#}"));
        assert_eq!(e.class,"SemanticInputError");
        let quoted=semantica_json::python_repr(&json!(bad)).unwrap();
        assert_eq!(e.to_string(),format!("Failed to parse RDF as nquads: Invalid line (Subject must be uriref or nodeID):\n{quoted}"));
        assert_eq!(state(&mut s).unwrap(),before,"parse failure must keep data/revision/warm caches");
    }
    let error=s.load_rdflib_with_base(b"<urn:ok> ?predicate <urn:o> .\n",RdfFormat::NQuads,None,None).unwrap_err();
    assert!(error.downcast_ref::<SourceFailure>().is_none(),"unreceived predicate grammar must not be relabeled as subject failure");
}
