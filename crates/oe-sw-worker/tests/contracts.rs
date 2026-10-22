use oe_contracts::SolidWorksNativeRequest;
use serde_json::{Value, json};
fn input() -> Value {
    let pin = json!({"path":"C:\\installed\\source","bytes":1,"sha256":"a".repeat(64)});
    json!({"schema":"oev2.solidworks-native-request/1","job_id":"test-native-job-00001","action":"api_check","project":null,"work":null,
      "installation":{"executable":pin,"type_library":pin,"constants_library":pin,"template":null,"major_version":33},
      "input":null,"output_name":null,"configuration":"not_applicable","plate":null,"new_thickness_m":null})
}
fn valid(v: Value) -> bool {
    serde_json::from_value::<SolidWorksNativeRequest>(v)
        .is_ok_and(|r| oe_sw_worker::validate_request(&r).is_ok())
}
#[test]
fn finite_native_request_keeps_identity_units_and_ownership_boundaries() {
    let baseline = input();
    assert!(valid(baseline.clone()));
    let mut v = baseline.clone();
    v["action"] = json!("attach_active");
    assert!(!valid(v));
    let mut v = baseline.clone();
    v["installation"]["executable"]["sha256"] = json!("not-a-hash");
    assert!(!valid(v));
    let mut v = baseline.clone();
    v["action"] = json!("inspect_part");
    v["input"] = json!({"path":"C:\\copy.SLDPRT","bytes":1,"sha256":"a".repeat(64)});
    v["configuration"] = json!("Default");
    assert!(!valid(v.clone()));
    let ref_ = json!({"path":"project.json","sha256":"b".repeat(64)});
    v["project"] = ref_.clone();
    v["work"] = ref_;
    assert!(valid(v.clone()));
    let mut bad = v.clone();
    bad["project"]["path"] = json!("../project.json");
    assert!(!valid(bad));
    let mut bad = v;
    bad["output_name"] = json!("overwrite.SLDPRT");
    assert!(!valid(bad));
}
#[test]
fn finite_plate_rejects_unbound_units_and_unsafe_output() {
    let mut v = input();
    v["action"] = json!("create_plate");
    v["project"] = json!({"path":"project.json","sha256":"b".repeat(64)});
    v["work"] = v["project"].clone();
    v["installation"]["template"] = v["installation"]["executable"].clone();
    v["output_name"] = json!("new.SLDPRT");
    v["plate"] = json!({"width_m":0.08,"height_m":0.05,"thickness_m":0.01,"bore_diameter_m":0.01,"reference_plane":"Front Plane","feature_name":"OwnedPlate"});
    assert!(valid(v.clone()));
    let mut bad = v.clone();
    bad["plate"]["width_m"] = json!(80);
    assert!(!valid(bad));
    let mut bad = v.clone();
    bad["plate"]["bore_diameter_m"] = json!(0.05);
    assert!(!valid(bad));
    v["output_name"] = json!("..\\original.SLDPRT");
    assert!(!valid(v));
}
#[test]
fn sha256_platform_implementations_match_standard_vectors() {
    assert_eq!(
        oe_sw_worker::sha256(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        oe_sw_worker::sha256(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        oe_sw_worker::sha256(&vec![b'a'; 1_000_000]),
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
    );
}
#[test]
fn cad_path_spelling_keeps_absolute_targets_without_claiming_file_ownership() {
    assert_eq!(
        oe_sw_worker::cad_api_path("\\\\?\\C:\\task\\plate.SLDPRT").unwrap(),
        "C:\\task\\plate.SLDPRT"
    );
    assert_eq!(
        oe_sw_worker::cad_api_path("C:\\task\\plate.SLDPRT").unwrap(),
        "C:\\task\\plate.SLDPRT"
    );
    assert_eq!(
        oe_sw_worker::cad_api_path("\\\\?\\UNC\\server\\share\\plate.SLDPRT").unwrap(),
        "\\\\server\\share\\plate.SLDPRT"
    );
    for bad in [
        "plate.SLDPRT",
        "C:plate.SLDPRT",
        "\\\\.\\pipe\\channel",
        "\\\\server",
        "\\\\?\\Volume{device}\\plate.SLDPRT",
    ] {
        assert!(oe_sw_worker::cad_api_path(bad).is_err(), "{bad}");
    }
    assert!(oe_sw_worker::cad_api_path(&format!("C:\\{}", "x".repeat(260))).is_err());
}
#[test]
fn isolated_profile_is_explicit_and_never_changes_old_request_ownership() {
    let mut v = input();
    v["execution_profile"] = json!("owned_private_desktop");
    assert!(!valid(v.clone()));
    v["schema"] = json!("oev2.solidworks-native-request/2");
    assert!(valid(v.clone()));
    v["execution_profile"] = json!("attach_existing");
    assert!(!valid(v.clone()));
    v["execution_profile"] = Value::Null;
    assert!(!valid(v.clone()));
    v["schema"] = json!("oev2.solidworks-native-request/1");
    assert!(valid(v));
}
#[test]
fn bore_witness_rejects_residual_bottom_and_wrong_axis_despite_matching_scalar_claims() {
    use oe_contracts::{
        SolidWorksNativeBody, SolidWorksNativeCircle, SolidWorksNativeFace, SolidWorksNativePlate,
    };
    let plate: SolidWorksNativePlate = serde_json::from_value(json!({"width_m":0.08,"height_m":0.05,"thickness_m":0.01,"bore_diameter_m":0.01,"reference_plane":"Front Plane","feature_name":"OwnedPlate"})).unwrap();
    let circle = |z| SolidWorksNativeCircle {
        edge_index: 0,
        parameters_native: vec![0.04, 0.025, z, 0., 0., 1., 0.005],
    };
    let mut faces: Vec<_> = (0..6)
        .map(|index| SolidWorksNativeFace {
            index,
            kind: "plane".into(),
            parameters_native: vec![0.; 6],
            circular_edges: vec![],
        })
        .collect();
    faces.push(SolidWorksNativeFace {
        index: 6,
        kind: "cylinder".into(),
        parameters_native: vec![0.04, 0.025, 0., 0., 0., 1., 0.005],
        circular_edges: vec![circle(0.), circle(0.012)],
    });
    let body = SolidWorksNativeBody {
        index: 0,
        face_count: 7,
        extreme_points_m: vec![0.; 18],
        mass_properties_native: vec![0.; 12],
        faces: Some(faces),
    };
    assert!(oe_sw_worker::check_bore(&plate, 0.012, &[body.clone()]).is_ok());
    let mut blind = body.clone();
    blind.faces.as_mut().unwrap()[6].circular_edges[1].parameters_native[2] = 0.01;
    assert!(oe_sw_worker::check_bore(&plate, 0.012, &[blind]).is_err());
    let mut wrong = body.clone();
    wrong.faces.as_mut().unwrap()[6].parameters_native[6] = 0.006;
    assert!(oe_sw_worker::check_bore(&plate, 0.012, &[wrong]).is_err());
    let mut missing = body;
    missing.faces = None;
    assert!(oe_sw_worker::check_bore(&plate, 0.012, &[missing]).is_err());
}
#[test]
fn reopen_record_rejects_same_process_and_changed_source_without_authenticating_json() {
    use oe_contracts::SolidWorksNativeObservation;
    let mut first = input();
    first["action"] = json!("create_plate");
    first["configuration"] = json!("Default");
    first["project"] = json!({"path":"project.json","sha256":"b".repeat(64)});
    first["work"] = first["project"].clone();
    first["output_name"] = json!("owned.SLDPRT");
    first["installation"]["template"] = first["installation"]["executable"].clone();
    first["plate"] = json!({"width_m":0.08,"height_m":0.05,"thickness_m":0.01,"bore_diameter_m":0.01,"reference_plane":"Front Plane","feature_name":"OwnedPlate"});
    let model = json!({"path":"C:\\private\\owned.SLDPRT","bytes":128,"sha256":"c".repeat(64)});
    let process = |pid, time| json!({"pid":pid,"creation_filetime":time,"session_id":2,"image":first["installation"]["executable"]});
    let raw1 = serde_json::to_vec(&first).unwrap();
    let mut saved = json!({"schema":"oev2.solidworks-native-observation/1","job_id":first["job_id"],"action":"create_plate","request_sha256":oe_sw_worker::sha256(&raw1),"worker":process(101,1000),"cad":process(102,1100),"execution":"completed","input":null,"output":model,"configuration":"Default","bodies":[{"index":0,"face_count":7,"extreme_points_m":[0.08,0,0,0,0,0,0,0.05,0,0,0,0,0,0,0.01,0,0,0],"mass_properties_native":[0,0,0,0.00004,0,0,0,0,0,0,0,0]}],"parameter_m":0.01,"cad_exit_observed":true,"api":{"scope":"synthetic_record_only"},"save":{"return_value":true,"errors":0},"errors":[]});
    let mut second = first.clone();
    second["action"] = json!("inspect_part");
    second["job_id"] = json!("test-native-reopen-00002");
    second["input"] = model;
    second["output_name"] = Value::Null;
    second["plate"] = Value::Null;
    let raw2 = serde_json::to_vec(&second).unwrap();
    let mut reopened = saved.clone();
    reopened["action"] = json!("inspect_part");
    reopened["job_id"] = second["job_id"].clone();
    reopened["request_sha256"] = json!(oe_sw_worker::sha256(&raw2));
    reopened["input"] = second["input"].clone();
    reopened["output"] = Value::Null;
    reopened["worker"] = process(201, 2000);
    reopened["cad"] = process(202, 2100);
    let a: SolidWorksNativeRequest = serde_json::from_value(first).unwrap();
    let b: SolidWorksNativeRequest = serde_json::from_value(second).unwrap();
    let check = |saved: &Value, reopened: &Value| {
        oe_sw_worker::check_reopen(
            &a,
            &raw1,
            &serde_json::from_value::<SolidWorksNativeObservation>(saved.clone()).unwrap(),
            &b,
            &raw2,
            &serde_json::from_value::<SolidWorksNativeObservation>(reopened.clone()).unwrap(),
        )
    };
    let report = check(&saved, &reopened).unwrap();
    assert_eq!(
        report["producer_execution_authentication"],
        "not_established_by_json"
    );
    assert_eq!(report["engineering_adoption"], "not_assessed");
    let mut same = reopened.clone();
    same["cad"] = saved["cad"].clone();
    assert!(check(&saved, &same).is_err());
    saved["output"]["sha256"] = json!("d".repeat(64));
    assert!(check(&saved, &reopened).is_err());
}
