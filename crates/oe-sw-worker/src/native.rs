//! Finite new-process execution, with raw call intent/result preservation.
use crate::{
    com::{self, Apartment, Arg, Trace},
    installation::{Api, Installation, observe_file, pinned_file},
    owned::{OwnedCad, PrivateDesktop, cad_processes, process_record},
};
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    SolidWorksNativeBody, SolidWorksNativeCircle, SolidWorksNativeFace,
    SolidWorksNativeObservation, SolidWorksNativeRequest,
};
use oe_windows_controller::windows::{PinnedFile, ProcessWatch, StableRoot};
use serde_json::json;
use std::{ffi::OsStr, path::Path};
use windows::{Win32::System::Com::IDispatch, core::VARIANT};

pub fn run(
    request_path: &Path,
    raw: &[u8],
    request: &SolidWorksNativeRequest,
    out: &Path,
) -> Result<()> {
    crate::validate_request(request)?;
    let parent = StableRoot::open(out.parent().context("solidworks_output_parent")?, None)?;
    let directory =
        parent.create_directory_new(out.file_name().context("solidworks_output_name")?)?;
    directory.create_file_new(
        OsStr::new("request.json"),
        raw,
        crate::MAX_RECORD_BYTES as u64,
    )?;
    let mut trace = Trace::new(directory.create_log_new(OsStr::new("native-calls.jsonl"))?);
    let worker = process_record(&ProcessWatch::current()?)?;
    let mut observation = SolidWorksNativeObservation {
        schema: crate::observation_schema(request).into(),
        job_id: request.job_id.clone(),
        action: request.action.clone(),
        request_sha256: crate::sha256(raw),
        worker,
        cad: None,
        execution: "failed".into(),
        input: request.input.clone(),
        output: None,
        configuration: request.configuration.clone(),
        bodies: vec![],
        parameter_m: None,
        cad_exit_observed: false,
        api: json!({"execution":"not_run"}),
        save: json!({"execution":"not_run"}),
        errors: vec![],
    };
    // Reverse drop order: owned COM objects, STA, then our thread desktop context.
    let mut private_desktop = None;
    let mut apartment = None;
    let mut owned_session = None;
    let mut owned_document_created = false;
    let result = (|| -> Result<()> {
        if request.action != "api_check"
            && (request.execution_profile.is_some() || cad_processes()?.is_empty())
        {
            private_desktop = Some(PrivateDesktop::enter(&request.job_id, &mut trace)?);
        }
        apartment = Some(Apartment::enter()?);
        let sta = apartment.as_ref().unwrap();
        let mut bindings = Vec::<(String, PinnedFile)>::new();
        let base = request_path.parent().context("solidworks_request_parent")?;
        for reference in [&request.project, &request.work].into_iter().flatten() {
            let path = base.join(&reference.path);
            let pin = observe_file(&path, crate::MAX_RECORD_BYTES as u64)?;
            ensure!(
                pin.sha256 == reference.sha256,
                "solidworks_project_work_bytes_changed"
            );
            bindings.push((reference.sha256.clone(), pinned_file(&pin)?));
        }
        let mut installation = Installation::load(&request.installation)?;
        observation.api = installation.api.observation.clone();
        trace.push(json!({"phase":"api_checked","installation":request.installation,"project":request.project,"work":request.work,"scope":"source_identity_not_engineering_applicability"}))?;
        if request.action == "api_check" {
            observation.execution = "api_only".into();
            return Ok(());
        }
        let mut input = request.input.as_ref().map(pinned_file).transpose()?;
        if let Some(file) = &input {
            trace.push(json!({"phase":"readonly_input_start","input":request.input,"file_identity":{"volume_id":file.identity().volume_id,"file_id":file.identity().file_id},"owner":"retained_original_readonly_file"}))?;
        }
        let output_path = request
            .output_name
            .as_ref()
            .map(|name| directory.path().join(name));
        let mut copy_identity = None;
        if request.action == "update_thickness" {
            let bytes = input
                .as_mut()
                .context("solidworks_update_input_missing")?
                .read_all(crate::MAX_MODEL_BYTES)?;
            directory.create_file_new(
                OsStr::new(request.output_name.as_ref().unwrap()),
                &bytes,
                crate::MAX_MODEL_BYTES,
            )?;
            let mut copy = directory
                .pin_file(
                    OsStr::new(request.output_name.as_ref().unwrap()),
                    crate::MAX_MODEL_BYTES,
                )?
                .context("solidworks_copy_not_created")?;
            ensure!(
                crate::sha256(&copy.read_all(crate::MAX_MODEL_BYTES)?)
                    == request.input.as_ref().unwrap().sha256,
                "solidworks_copy_bytes_mismatch_before_open"
            );
            copy.verify()?;
            copy_identity = Some(copy.identity().clone());
            trace.push(json!({"phase":"task_copy_bytes_identity_before_open","file_identity":{"volume_id":copy.identity().volume_id,"file_id":copy.identity().file_id},"input_sha256":request.input.as_ref().unwrap().sha256,"native_write":"not_started"}))?;
        }
        owned_session = Some(OwnedCad::spawn(
            &request.installation.executable,
            request.execution_profile.as_deref(),
            private_desktop.clone(),
            directory.path(),
            sta,
            &installation.api,
            &mut trace,
        )?);
        let cad = owned_session.as_mut().unwrap();
        observation.cad = Some(cad.identity.clone());
        cad.verify_isolation(&mut trace)?;
        // Keep the process identity in the durable log before any document call.
        let app = cad.app.as_ref().context("solidworks_owned_app_missing")?;
        let api = &installation.api;
        let doc = if request.action == "create_plate" {
            let template = request.installation.template.as_ref().unwrap();
            let doc = object(
                &mut trace,
                app,
                api,
                "ISldWorks.NewDocument",
                vec![
                    Arg::Text(crate::cad_api_path(&template.path)?),
                    Arg::I4(0),
                    Arg::R8(0.0),
                    Arg::R8(0.0),
                ],
            )?;
            owned_document_created = true;
            feature_inventory(&doc, api, &mut trace)?;
            ensure!(
                com::integer(&value(&mut trace, &doc, api, "IModelDoc2.GetType", vec![])?)? == 1,
                "solidworks_template_not_part"
            );
            create_plate(&doc, request, api, &mut trace)?;
            doc
        } else {
            let path = if request.action == "update_thickness" {
                output_path.as_ref().unwrap().to_string_lossy().into_owned()
            } else {
                request.input.as_ref().unwrap().path.clone()
            };
            let path = crate::cad_api_path(&path)?;
            let options = if request.action == "inspect_part" {
                3
            } else {
                1
            };
            let (result, args) = trace.invoke(
                app,
                api,
                "ISldWorks.OpenDoc6",
                vec![
                    Arg::Text(path.clone()),
                    Arg::I4(1),
                    Arg::I4(options),
                    Arg::Text(request.configuration.clone()),
                    Arg::I4Ref(Box::new(0)),
                    Arg::I4Ref(Box::new(0)),
                ],
            )?;
            ensure!(args[4] == 0, "solidworks_open_native_errors:{}", args[4]);
            trace.push(json!({"phase":"open_result","errors":args[4],"warnings":args[5],"readonly":options==3}))?;
            let doc = com::dispatch(&result)?;
            owned_document_created = true;
            ensure!(
                com::integer(&value(&mut trace, &doc, api, "IModelDoc2.GetType", vec![])?)? == 1,
                "solidworks_not_part"
            );
            let readback = com::string(&value(
                &mut trace,
                &doc,
                api,
                "IModelDoc2.GetPathName",
                vec![],
            )?)?;
            if request.action == "update_thickness" {
                let actual_path = Path::new(&readback);
                let actual_root = StableRoot::open(
                    actual_path
                        .parent()
                        .context("solidworks_open_copy_parent")?,
                    None,
                )?;
                let actual_directory = actual_root.as_directory()?;
                ensure!(
                    actual_directory.identity() == directory.identity(),
                    "solidworks_open_copy_directory_mismatch"
                );
                let actual_identity = actual_directory.native_mutable_file_identity(
                    actual_path
                        .file_name()
                        .context("solidworks_open_copy_name")?,
                    crate::MAX_MODEL_BYTES,
                )?;
                ensure!(
                    Some(&actual_identity) == copy_identity.as_ref(),
                    "solidworks_opened_copy_identity_mismatch"
                );
                trace.push(json!({"phase":"task_copy_open_file_identity","file_identity":{"volume_id":actual_identity.volume_id,"file_id":actual_identity.file_id},"content_while_native_writer":"not_claimed","original_input":"retained_readonly_pin"}))?;
            } else {
                let actual = observe_file(Path::new(&readback), crate::MAX_MODEL_BYTES)?;
                ensure!(
                    actual.sha256 == request.input.as_ref().unwrap().sha256
                        && actual.bytes == request.input.as_ref().unwrap().bytes,
                    "solidworks_opened_other_file"
                );
                let actual_owner = pinned_file(&actual)?;
                ensure!(
                    actual_owner.identity()
                        == input
                            .as_ref()
                            .context("solidworks_input_owner_missing")?
                            .identity(),
                    "solidworks_opened_file_identity_mismatch"
                );
                actual_owner.verify()?;
            }
            if request.action == "update_thickness" {
                update(&doc, request, api, &mut trace)?;
            }
            doc
        };
        let configuration_manager = object(
            &mut trace,
            &doc,
            api,
            "IModelDoc2.ConfigurationManager",
            vec![],
        )?;
        let config = object(
            &mut trace,
            &configuration_manager,
            api,
            "IConfigurationManager.ActiveConfiguration",
            vec![],
        )?;
        observation.configuration = com::string(&value(
            &mut trace,
            &config,
            api,
            "IConfiguration.Name",
            vec![],
        )?)?;
        drop(config);
        drop(configuration_manager);
        ensure!(
            request.configuration.is_empty() || request.configuration == observation.configuration,
            "solidworks_configuration_readback_mismatch"
        );
        observation.bodies = inspect(&doc, api, &mut trace)?;
        cad.verify_isolation(&mut trace)?;
        if let Some(plate) = &request.plate {
            observation.parameter_m = Some(parameter(&doc, &plate.feature_name, api, &mut trace)?);
            let expected = request.new_thickness_m.unwrap_or(plate.thickness_m);
            ensure!(
                (observation.parameter_m.unwrap() - expected).abs() <= 1e-10,
                "solidworks_parameter_readback_mismatch"
            );
            let volume = (plate.width_m * plate.height_m
                - std::f64::consts::PI * (plate.bore_diameter_m / 2.0).powi(2))
                * expected;
            let measured = observation
                .bodies
                .iter()
                .map(|b| b.mass_properties_native[3])
                .sum::<f64>();
            ensure!(
                (measured - volume).abs() <= 1e-14f64.max(volume.abs() * 1e-8),
                "solidworks_plate_volume_mismatch"
            );
            trace.push(json!({"phase":"finite_plate_volume_check","expected_m3":volume,"observed_m3":measured,"absolute_tolerance_m3":1e-14,"relative_tolerance":1e-8,"topology_and_engineering_adoption":"not_established"}))?;
            let bore = crate::check_bore(plate, expected, &observation.bodies)?;
            trace.push(json!({"phase":"finite_plate_bore_geometry_check","result":bore,"source":"independent request dimensions; complete native face/rim inventory","engineering_adoption":"not_established"}))?;
        }
        if let Some(path) = &output_path {
            let ext = object(&mut trace, &doc, api, "IModelDoc2.Extension", vec![])?;
            let (result, args) = trace.invoke(
                &ext,
                api,
                "IModelDocExtension.SaveAs",
                vec![
                    Arg::Text(crate::cad_api_path(&path.to_string_lossy())?),
                    Arg::I4(0),
                    Arg::I4(1),
                    Arg::NullDispatch,
                    Arg::I4Ref(Box::new(0)),
                    Arg::I4Ref(Box::new(0)),
                ],
            )?;
            observation.save = json!({"execution":"returned","return_value":com::boolean(&result)?,"errors":args[4],"warnings":args[5],"requested_path":path});
            ensure!(
                com::boolean(&result)? && args[4] == 0,
                "solidworks_native_save_failed"
            );
            let actual = com::string(&value(
                &mut trace,
                &doc,
                api,
                "IModelDoc2.GetPathName",
                vec![],
            )?)?;
            let actual_path = Path::new(&actual);
            let actual_parent = StableRoot::open(
                actual_path
                    .parent()
                    .context("solidworks_saved_parent_missing")?,
                None,
            )?;
            ensure!(
                actual_parent.as_directory()?.identity() == directory.identity()
                    && actual_path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .zip(path.file_name().and_then(|s| s.to_str()))
                        .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b)),
                "solidworks_saved_file_owner_mismatch"
            );
            observation.save["native_path"] = json!(actual);
        }
        cad.verify_isolation(&mut trace)?;
        let title = com::string(&value(
            &mut trace,
            &doc,
            api,
            "IModelDoc2.GetTitle",
            vec![],
        )?)?;
        drop(doc);
        trace.invoke(app, api, "ISldWorks.CloseDoc", vec![Arg::Text(title)])?;
        observation.cad_exit_observed = cad.exit(sta, api, &mut trace)?;
        ensure!(
            observation.cad_exit_observed,
            "solidworks_owned_exit_not_observed"
        );
        if let Some(path) = output_path {
            let actual = Path::new(
                observation.save["native_path"]
                    .as_str()
                    .context("solidworks_saved_native_path_missing")?,
            );
            let actual_parent = StableRoot::open(
                actual.parent().context("solidworks_saved_parent_missing")?,
                None,
            )?;
            let actual_directory = actual_parent.as_directory()?;
            let mut target = directory
                .pin_file(path.file_name().unwrap(), crate::MAX_MODEL_BYTES)?
                .context("solidworks_saved_target_missing")?;
            let native = actual_directory
                .pin_file(
                    actual
                        .file_name()
                        .context("solidworks_saved_name_missing")?,
                    crate::MAX_MODEL_BYTES,
                )?
                .context("solidworks_saved_native_missing")?;
            ensure!(
                target.identity() == native.identity(),
                "solidworks_saved_native_identity_changed"
            );
            let raw = target.read_all(crate::MAX_MODEL_BYTES)?;
            target.verify()?;
            native.verify()?;
            observation.save["file_identity"] = json!({"volume_id":target.identity().volume_id,"file_id":target.identity().file_id});
            observation.output = Some(oe_contracts::SolidWorksNativeFilePin {
                path: path.to_string_lossy().into_owned(),
                bytes: raw.len() as u64,
                sha256: crate::sha256(&raw),
            });
        }
        if let Some(file) = &mut input {
            file.verify()?;
            ensure!(
                crate::sha256(&file.read_all(crate::MAX_MODEL_BYTES)?)
                    == request.input.as_ref().unwrap().sha256,
                "solidworks_input_drift"
            );
            trace.push(json!({"phase":"readonly_input_end","input":request.input,"file_identity":{"volume_id":file.identity().volume_id,"file_id":file.identity().file_id},"full_bytes_and_identity":"verified_unchanged_after_native_exit"}))?;
        }
        for (sha, file) in &mut bindings {
            file.verify()?;
            ensure!(
                crate::sha256(&file.read_all(crate::MAX_RECORD_BYTES as u64)?) == *sha,
                "solidworks_project_work_drift"
            );
        }
        installation.verify()?;
        directory.verify()?;
        observation.execution = "completed".into();
        Ok(())
    })();
    if let Err(error) = &result {
        observation.errors.push(format!("{error:#}"));
        trace.push(json!({"phase":"execution_failed","error":format!("{error:#}"),"automatic_retry":"never","unobserved_cad_exit":"unknown; inspect recorded process before any further action"}))?;
        // When binding and a document return were both observed, discard only
        // this new process's single document. Never save a failed model, retry
        // an operation, use a foreign instance, or terminate a process.
        if owned_document_created && !observation.cad_exit_observed {
            if let Some(cad) = owned_session.as_mut() {
                let cleanup = (|| -> Result<bool> {
                    cad.verify_isolation(&mut trace)?;
                    let installation = Installation::load(&request.installation)?;
                    let api = &installation.api;
                    let app = cad
                        .app
                        .as_ref()
                        .context("solidworks_cleanup_owned_binding_missing")?;
                    ensure!(
                        com::integer(&value(
                            &mut trace,
                            app,
                            api,
                            "ISldWorks.GetDocumentCount",
                            vec![]
                        )?)? == 1,
                        "solidworks_cleanup_document_inventory_changed"
                    );
                    let doc = object(&mut trace, app, api, "ISldWorks.ActiveDoc", vec![])?;
                    let title = com::string(&value(
                        &mut trace,
                        &doc,
                        api,
                        "IModelDoc2.GetTitle",
                        vec![],
                    )?)?;
                    drop(doc);
                    trace.push(json!({"phase":"owned_failure_cleanup_intent","cad":cad.identity,"document":title,"save":"not_requested","retry":"never","foreign_control":"none"}))?;
                    trace.invoke(app, api, "ISldWorks.CloseDoc", vec![Arg::Text(title)])?;
                    ensure!(
                        com::integer(&value(
                            &mut trace,
                            app,
                            api,
                            "ISldWorks.GetDocumentCount",
                            vec![]
                        )?)? == 0,
                        "solidworks_cleanup_close_not_observed"
                    );
                    cad.exit(
                        apartment
                            .as_ref()
                            .context("solidworks_cleanup_apartment_missing")?,
                        api,
                        &mut trace,
                    )
                })();
                match cleanup {
                    Ok(exited) => {
                        observation.cad_exit_observed = exited;
                        trace.push(json!({"phase":"owned_failure_cleanup_result","exit_observed":exited,"execution":"failed","original_failure":"preserved"}))?;
                    }
                    Err(error) => {
                        trace.push(json!({"phase":"owned_failure_cleanup_failed","error":format!("{error:#}"),"exit_observed":false,"termination":"not_attempted","original_failure":"preserved"}))?;
                    }
                }
            }
        }
    }
    crate::check_observation(request, raw, &observation)?;
    let encoded = serde_json::to_vec_pretty(&observation)?;
    directory.create_file_new(
        OsStr::new("observation.json"),
        &encoded,
        crate::MAX_RECORD_BYTES as u64,
    )?;
    println!(
        "{}",
        serde_json::to_string(
            &json!({"execution":observation.execution,"observation":directory.path().join("observation.json"),"engineering_adoption":"not_assessed"})
        )?
    );
    result
}
fn value(
    trace: &mut Trace,
    obj: &IDispatch,
    api: &Api,
    key: &str,
    args: Vec<Arg>,
) -> Result<VARIANT> {
    Ok(trace.invoke(obj, api, key, args)?.0)
}
fn object(
    trace: &mut Trace,
    obj: &IDispatch,
    api: &Api,
    key: &str,
    args: Vec<Arg>,
) -> Result<IDispatch> {
    com::dispatch(&value(trace, obj, api, key, args)?)
}
fn parameter(doc: &IDispatch, name: &str, api: &Api, trace: &mut Trace) -> Result<f64> {
    let p = object(
        trace,
        doc,
        api,
        "IModelDoc2.Parameter",
        vec![Arg::Text(format!("D1@{name}"))],
    )?;
    com::real(&value(trace, &p, api, "IDimension.SystemValue", vec![])?)
}
fn update(
    doc: &IDispatch,
    r: &SolidWorksNativeRequest,
    api: &Api,
    trace: &mut Trace,
) -> Result<()> {
    let p = r.plate.as_ref().unwrap();
    let before = parameter(doc, &p.feature_name, api, trace)?;
    ensure!(
        (before - p.thickness_m).abs() <= 1e-10,
        "solidworks_update_prior_dimension_mismatch"
    );
    let dimension = object(
        trace,
        doc,
        api,
        "IModelDoc2.Parameter",
        vec![Arg::Text(format!("D1@{}", p.feature_name))],
    )?;
    value(
        trace,
        &dimension,
        api,
        "IDimension.SystemValue.put",
        vec![Arg::R8(r.new_thickness_m.unwrap())],
    )?;
    ensure!(
        com::boolean(&value(
            trace,
            doc,
            api,
            "IModelDoc2.ForceRebuild3",
            vec![Arg::Bool(false)]
        )?)?,
        "solidworks_update_rebuild_failed"
    );
    Ok(())
}
fn create_plate(
    doc: &IDispatch,
    r: &SolidWorksNativeRequest,
    api: &Api,
    trace: &mut Trace,
) -> Result<()> {
    let p = r.plate.as_ref().unwrap();
    let ext = object(trace, doc, api, "IModelDoc2.Extension", vec![])?;
    ensure!(
        com::boolean(&value(
            trace,
            &ext,
            api,
            "IModelDocExtension.SelectByID2",
            vec![
                Arg::Text(p.reference_plane.clone()),
                Arg::Text("PLANE".into()),
                Arg::R8(0.0),
                Arg::R8(0.0),
                Arg::R8(0.0),
                Arg::Bool(false),
                Arg::I4(0),
                Arg::NullDispatch,
                Arg::I4(0)
            ]
        )?)?,
        "solidworks_reference_plane_not_selected"
    );
    value(
        trace,
        doc,
        api,
        "IModelDoc2.InsertSketch2",
        vec![Arg::Bool(true)],
    )?;
    let sketch = object(trace, doc, api, "IModelDoc2.SketchManager", vec![])?;
    let rectangle = value(
        trace,
        &sketch,
        api,
        "ISketchManager.CreateCornerRectangle",
        vec![
            Arg::R8(0.0),
            Arg::R8(0.0),
            Arg::R8(0.0),
            Arg::R8(p.width_m),
            Arg::R8(p.height_m),
            Arg::R8(0.0),
        ],
    )?;
    ensure!(
        com::objects(&rectangle, 4)?.len() == 4,
        "solidworks_rectangle_segment_inventory"
    );
    drop(rectangle);
    if p.bore_diameter_m > 0.0 {
        object(
            trace,
            &sketch,
            api,
            "ISketchManager.CreateCircle",
            vec![
                Arg::R8(p.width_m / 2.0),
                Arg::R8(p.height_m / 2.0),
                Arg::R8(0.0),
                Arg::R8((p.width_m + p.bore_diameter_m) / 2.0),
                Arg::R8(p.height_m / 2.0),
                Arg::R8(0.0),
            ],
        )?;
    }
    value(
        trace,
        doc,
        api,
        "IModelDoc2.InsertSketch2",
        vec![Arg::Bool(true)],
    )?;
    drop(sketch);
    let manager = object(trace, doc, api, "IModelDoc2.FeatureManager", vec![])?;
    let feature = object(
        trace,
        &manager,
        api,
        "IFeatureManager.FeatureExtrusion3",
        vec![
            Arg::Bool(true),
            Arg::Bool(false),
            Arg::Bool(false),
            Arg::I4(0),
            Arg::I4(0),
            Arg::R8(p.thickness_m),
            Arg::R8(0.0),
            Arg::Bool(false),
            Arg::Bool(false),
            Arg::Bool(false),
            Arg::Bool(false),
            Arg::R8(0.0),
            Arg::R8(0.0),
            Arg::Bool(false),
            Arg::Bool(false),
            Arg::Bool(false),
            Arg::Bool(false),
            Arg::Bool(true),
            Arg::Bool(true),
            Arg::Bool(true),
            Arg::I4(0),
            Arg::R8(0.0),
            Arg::Bool(false),
        ],
    )?;
    value(
        trace,
        &feature,
        api,
        "IFeature.Name.put",
        vec![Arg::Text(p.feature_name.clone())],
    )?;
    ensure!(
        com::string(&value(trace, &feature, api, "IFeature.Name", vec![])?)? == p.feature_name,
        "solidworks_feature_name_readback"
    );
    Ok(())
}
fn feature_inventory(doc: &IDispatch, api: &Api, trace: &mut Trace) -> Result<()> {
    let mut next = value(trace, doc, api, "IModelDoc2.FirstFeature", vec![])?;
    let mut features = Vec::new();
    for _ in 0..256 {
        if com::vt(&next) == windows::Win32::System::Variant::VT_DISPATCH.0
            && unsafe {
                next.as_raw()
                    .Anonymous
                    .Anonymous
                    .Anonymous
                    .pdispVal
                    .is_null()
            }
        {
            trace.push(json!({"phase":"new_template_feature_inventory","features":features,"scope":"observed template names; independent dimensions unchanged"}))?;
            return Ok(());
        }
        let feature = com::dispatch(&next)?;
        let name = com::string(&value(trace, &feature, api, "IFeature.Name", vec![])?)?;
        let kind = com::string(&value(
            trace,
            &feature,
            api,
            "IFeature.GetTypeName2",
            vec![],
        )?)?;
        features.push(json!({"name":name,"type":kind}));
        next = value(trace, &feature, api, "IFeature.GetNextFeature", vec![])?;
    }
    anyhow::bail!("solidworks_template_feature_inventory_budget")
}
fn inspect(doc: &IDispatch, api: &Api, trace: &mut Trace) -> Result<Vec<SolidWorksNativeBody>> {
    let bodies = com::objects(
        &value(
            trace,
            doc,
            api,
            "IPartDoc.GetBodies2",
            vec![Arg::I4(0), Arg::Bool(false)],
        )?,
        256,
    )?;
    ensure!(!bodies.is_empty(), "solidworks_no_solid_body");
    let mut out = Vec::new();
    for (index, body) in bodies.into_iter().enumerate() {
        ensure!(
            com::integer(&value(trace, &body, api, "IBody2.GetType", vec![])?)? == 0,
            "solidworks_body_type_mismatch"
        );
        let faces = com::integer(&value(trace, &body, api, "IBody2.GetFaceCount", vec![])?)?;
        ensure!(
            (1..=1_000_000).contains(&faces),
            "solidworks_body_face_count_invalid"
        );
        let mass = com::reals(
            &value(
                trace,
                &body,
                api,
                "IBody2.GetMassProperties",
                vec![Arg::R8(1.0)],
            )?,
            12,
        )?;
        ensure!(
            mass.len() == 12 && mass[3] > 0.0,
            "solidworks_mass_properties_shape"
        );
        let mut extremes = Vec::new();
        for direction in [
            [1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
        ] {
            let (result, args) = trace.invoke(
                &body,
                api,
                "IBody2.GetExtremePoint",
                vec![
                    Arg::R8(direction[0]),
                    Arg::R8(direction[1]),
                    Arg::R8(direction[2]),
                    Arg::R8Ref(Box::new(f64::NAN)),
                    Arg::R8Ref(Box::new(f64::NAN)),
                    Arg::R8Ref(Box::new(f64::NAN)),
                ],
            )?;
            ensure!(com::boolean(&result)?, "solidworks_extreme_point_failed");
            for v in &args[3..] {
                let v = v.as_f64().context("solidworks_extreme_point_nonfinite")?;
                ensure!(v.is_finite(), "solidworks_extreme_point_nonfinite");
                extremes.push(v);
            }
        }
        out.push(SolidWorksNativeBody {
            index: index as u64,
            face_count: faces as u64,
            extreme_points_m: extremes,
            mass_properties_native: mass,
            faces: Some(inspect_faces(&body, faces as usize, api, trace)?),
        });
    }
    Ok(out)
}
fn inspect_faces(
    body: &IDispatch,
    count: usize,
    api: &Api,
    trace: &mut Trace,
) -> Result<Vec<SolidWorksNativeFace>> {
    let faces = com::objects(&value(trace, body, api, "IBody2.GetFaces", vec![])?, 4096)?;
    ensure!(
        faces.len() == count,
        "solidworks_complete_face_inventory_mismatch"
    );
    let mut out = Vec::new();
    for (index, face) in faces.into_iter().enumerate() {
        let surface = object(trace, &face, api, "IFace2.GetSurface", vec![])?;
        let cylinder = com::boolean(&value(trace, &surface, api, "ISurface.IsCylinder", vec![])?)?;
        let plane = com::boolean(&value(trace, &surface, api, "ISurface.IsPlane", vec![])?)?;
        ensure!(!(cylinder && plane), "solidworks_surface_type_ambiguous");
        let (kind, parameters) = if cylinder {
            (
                "cylinder",
                com::reals(
                    &value(trace, &surface, api, "ISurface.CylinderParams", vec![])?,
                    7,
                )?,
            )
        } else if plane {
            (
                "plane",
                com::reals(
                    &value(trace, &surface, api, "ISurface.PlaneParams", vec![])?,
                    6,
                )?,
            )
        } else {
            ("other", vec![])
        };
        let mut circles = Vec::new();
        if cylinder {
            let edges = com::objects(&value(trace, &face, api, "IFace2.GetEdges", vec![])?, 4096)?;
            for (edge_index, edge) in edges.into_iter().enumerate() {
                let curve = object(trace, &edge, api, "IEdge.GetCurve", vec![])?;
                if com::boolean(&value(trace, &curve, api, "ICurve.IsCircle", vec![])?)? {
                    circles.push(SolidWorksNativeCircle {
                        edge_index: edge_index as u64,
                        parameters_native: com::reals(
                            &value(trace, &curve, api, "ICurve.CircleParams", vec![])?,
                            7,
                        )?,
                    });
                }
            }
        }
        out.push(SolidWorksNativeFace {
            index: index as u64,
            kind: kind.into(),
            parameters_native: parameters,
            circular_edges: circles,
        });
    }
    trace.push(json!({"phase":"complete_native_face_inventory","faces":out,"scope":"native surface and circular boundary curves; not general full trimmed BRep topology"}))?;
    Ok(out)
}
