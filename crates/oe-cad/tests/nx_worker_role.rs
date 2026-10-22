//! Source-complete synthetic packages only; no PE, SDK, SSH or native task runs.
use oe_cad::nx::{self, InputBinding, PrepareRequest, client::Client, transport::RemoteProfile};
use oe_core::SkillRoot;
use oe_nx_protocol::*;
use std::{fs, path::PathBuf};

struct Fixture {
    _temporary: tempfile::TempDir,
    root: SkillRoot,
    worker_directory: PathBuf,
    worker: WorkerManifest,
}
impl Fixture {
    fn new(role: &str) -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().canonicalize().unwrap();
        fs::write(path.join("SKILL.md"), "synthetic role boundary").unwrap();
        fs::create_dir(path.join("var")).unwrap();
        let worker_directory = path.join("var/package");
        fs::create_dir(&worker_directory).unwrap();
        let data = [
            (
                "worker/oe-nx-worker.exe",
                b"not a PE; never execute".as_slice(),
            ),
            ("sources/fixture.rs", b"not compiled".as_slice()),
            ("build/fixture.txt", b"no build".as_slice()),
        ];
        for (name, bytes) in data {
            let file = worker_directory.join(name);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, bytes).unwrap();
        }
        let worker = WorkerManifest {
            schema: WORKER_SCHEMA.into(),
            worker_id: role.into(),
            version: "synthetic-role".into(),
            target: "x86_64-pc-windows-msvc".into(),
            executable: FileEntry::from_bytes(data[0].0, data[0].1).unwrap(),
            runtime_files: vec![],
            source_files: vec![FileEntry::from_bytes(data[1].0, data[1].1).unwrap()],
            build_recipe: FileEntry::from_bytes(data[2].0, data[2].1).unwrap(),
            toolchain: "no compiler".into(),
            required_runtime: RuntimeIdentity {
                product: "NX".into(),
                product_label: "synthetic".into(),
                architecture: "x64".into(),
                libraries: vec![FileEntry::from_bytes("NXBIN/libufun.dll", b"not loaded").unwrap()],
            },
            capabilities: vec![
                Capability {
                    action: "create_cylinder_solid".into(),
                    params_schema: "oev2.nx-rotating-part-params/1".into(),
                },
                Capability {
                    action: "inspect".into(),
                    params_schema: "synthetic.inspect/v1".into(),
                },
            ],
        };
        let root = SkillRoot::new(&path).unwrap();
        let mut result = Self {
            _temporary: temporary,
            root,
            worker_directory,
            worker,
        };
        result.persist();
        result
    }
    fn persist(&mut self) {
        fs::write(
            self.worker_directory.join("worker.json"),
            serde_json::to_vec(&self.worker).unwrap(),
        )
        .unwrap();
    }
    fn request(&self, action: &str, schema: &str) -> PrepareRequest {
        let parameters = self.root.path().join("var/params.json");
        fs::write(
            &parameters,
            serde_json::to_vec(&serde_json::json!({"schema":schema,"action":action})).unwrap(),
        )
        .unwrap();
        PrepareRequest {
            worker_directory: self.worker_directory.clone(),
            parameter_file: parameters,
            inputs: Vec::<InputBinding>::new(),
            job_id: JobId::new("synthetic-new-role-job").unwrap(),
            root_identity: RootIdentity {
                host_id: "synthetic-host".into(),
                volume_id: "synthetic-volume".into(),
                directory_id: "synthetic-root".into(),
            },
            action: action.into(),
            parameter_schema: schema.into(),
        }
    }
}

#[test]
fn generic_completeness_remains_distinct_from_explicit_current_role() {
    let mut f = Fixture::new("oe-nx-rotating-part");
    let original = fs::read(f.worker_directory.join("worker.json")).unwrap();
    let complete = nx::verify_worker(&f.root, &f.worker_directory).unwrap();
    assert_eq!(complete.status, "package_complete");
    assert!(!complete.execution_performed);
    assert_eq!(complete.native_acceptance, "not_evaluated");
    assert!(
        format!(
            "{:#}",
            nx::verify_current_worker_role(&f.root, &f.worker_directory).unwrap_err()
        )
        .contains("native_worker_identity_contract_differs")
    );
    assert_eq!(
        fs::read(f.worker_directory.join("worker.json")).unwrap(),
        original
    );
    f.worker.worker_id = "oe-nx-native-acceptance".into();
    f.persist();
    let role = nx::verify_current_worker_role(&f.root, &f.worker_directory).unwrap();
    assert_eq!(role["schema"], "oev2.nx-current-worker-role-verification/1");
    assert_eq!(role["status"], "declared_role_matched");
    assert_eq!(role["package"]["status"], "package_complete");
    assert_eq!(role["compiled_executable_identity"], "not_observed");
    assert_eq!(role["native_acceptance"], "not_evaluated");
    assert_eq!(role["engineering_acceptance"], "not_evaluated");
    assert_eq!(role["execution_performed"], false);
}

#[test]
fn cylinder_prepare_refuses_bad_role_but_old_non_cylinder_prepare_keeps_meaning() {
    let f = Fixture::new("oe-nx-rotating-part");
    let request = f.request("create_cylinder_solid", "oev2.nx-rotating-part-params/1");
    assert!(
        format!("{:#}", nx::prepare(&f.root, &request).unwrap_err())
            .contains("native_worker_identity_contract_differs")
    );
    let old = f.request("inspect", "synthetic.inspect/v1");
    let package = nx::prepare(&f.root, &old).unwrap();
    package.verify().unwrap();
    assert_eq!(package.job.worker.worker_id, "oe-nx-rotating-part");
    assert!(!f.root.path().join("var/stage").exists());
}

#[test]
fn cylinder_stage_rechecks_role_before_profile_admission_or_transport() {
    let mut f = Fixture::new("oe-nx-native-acceptance");
    let request = f.request("create_cylinder_solid", "oev2.nx-rotating-part-params/1");
    let mut package = nx::prepare(&f.root, &request).unwrap();
    package.job.worker.worker_id = "oe-nx-rotating-part".into();
    package
        .members
        .insert("job.json".into(), serde_json::to_vec(&package.job).unwrap());
    package.verify().unwrap();
    let profile = RemoteProfile {
        schema: "cad-agent.nx-remote-profile/v2".into(),
        ssh_command: vec!["ssh".into(), "synthetic-no-connection".into()],
        remote_root: "C:/synthetic".into(),
        nx_root: "C:/synthetic/NX".into(),
        controller_path: "C:/synthetic/controller.exe".into(),
        controller: BlobIdentity::of(b"not a controller"),
        expected_root: None,
        legacy_review: None,
        dispatch_writer_review: None,
    };
    let client = Client {
        root: &f.root,
        profile: &profile,
    };
    let error = client
        .stage(&package, std::path::Path::new("var/stage"))
        .unwrap_err();
    assert!(format!("{error:#}").contains("native_worker_identity_contract_differs"));
    assert!(!f.root.path().join("var/stage").exists());
    f.worker.worker_id = "oe-nx-native-acceptance".into();
    f.persist();
    let complete = nx::verify_current_worker_role(&f.root, &f.worker_directory).unwrap();
    assert_eq!(complete["execution_performed"], false);
}

#[test]
fn stepped_stage_rechecks_role_before_profile_admission_or_transport() {
    let mut f = Fixture::new("oe-nx-native-acceptance");
    f.worker.capabilities.push(Capability {
        action: "create_stepped_shaft".into(),
        params_schema: "oev2.nx-stepped-shaft-params/1".into(),
    });
    f.persist();
    let request = f.request("create_stepped_shaft", "oev2.nx-stepped-shaft-params/1");
    let mut package = nx::prepare(&f.root, &request).unwrap();
    package.job.worker.worker_id = "oe-nx-rotating-part".into();
    package
        .members
        .insert("job.json".into(), serde_json::to_vec(&package.job).unwrap());
    package.verify().unwrap();
    let profile = RemoteProfile {
        schema: "cad-agent.nx-remote-profile/v2".into(),
        ssh_command: vec!["ssh".into(), "synthetic-no-connection".into()],
        remote_root: "C:/synthetic".into(),
        nx_root: "C:/synthetic/NX".into(),
        controller_path: "C:/synthetic/controller.exe".into(),
        controller: BlobIdentity::of(b"not a controller"),
        expected_root: None,
        legacy_review: None,
        dispatch_writer_review: None,
    };
    let client = Client {
        root: &f.root,
        profile: &profile,
    };
    let error = client
        .stage(&package, std::path::Path::new("var/stage"))
        .unwrap_err();
    assert!(format!("{error:#}").contains("native_worker_identity_contract_differs"));
    assert!(!f.root.path().join("var/stage").exists());
    f.worker.worker_id = "oe-nx-native-acceptance".into();
    f.persist();
    let complete = nx::verify_current_worker_role(&f.root, &f.worker_directory).unwrap();
    assert_eq!(complete["execution_performed"], false);
}

#[test]
fn stepped_actions_require_current_role_before_prepare() {
    for action in ["create_stepped_shaft", "inspect_stepped_shaft"] {
        let f = Fixture::new("oe-nx-rotating-part");
        let request = f.request(action, "oev2.nx-stepped-shaft-params/1");
        assert!(
            format!("{:#}", nx::prepare(&f.root, &request).unwrap_err())
                .contains("native_worker_identity_contract_differs")
        );
    }
}
