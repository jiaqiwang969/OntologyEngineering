//! Private, synthetic installer backend. No interpreter, pip, network or native
//! semantics is invoked by the orchestration tests. External identity rejection
//! uses a shell fixture returning explicitly synthetic probe coordinates.
use super::*;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use std::{
    cell::{Cell, RefCell},
    io::{Cursor, Write},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use zip::write::SimpleFileOptions;

const VERSION: &str = "9.9.9+fixture.1";
const WHEEL: &str = "semantica-9.9.9+fixture.1-py3-none-any.whl";
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    root: SkillRoot,
}
impl Fixture {
    fn new(old: bool) -> Self {
        let installed = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap();
        let path = installed.join(format!(
            "var/maintenance/rust-tests/runtime-install-fixture-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        create_new_private_dir(&path).unwrap();
        fs::write(path.join("SKILL.md"), "synthetic installer fixture\n").unwrap();
        fs::create_dir_all(path.join("runtime/vendor")).unwrap();
        let package = b"__version__ = '9.9.9+fixture.1'\n";
        let dist = "semantica-9.9.9+fixture.1.dist-info";
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name,bytes) in [
            ("semantica/__init__.py".into(),package.to_vec()),
            (format!("{dist}/METADATA"),format!("Metadata-Version: 2.4\nName: semantica\nVersion: {VERSION}\nRequires-Python: >=3.10\n\n").into_bytes()),
            (format!("{dist}/WHEEL"),b"Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n".to_vec()),
            (format!("{dist}/RECORD"),format!("semantica/__init__.py,sha256={},{}\n{dist}/RECORD,,\n",URL_SAFE_NO_PAD.encode(Sha256::digest(package)),package.len()).into_bytes()),
        ] {writer.start_file(name,SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).unix_permissions(0o644)).unwrap();writer.write_all(&bytes).unwrap();}
        let bytes = writer.finish().unwrap().into_inner();
        fs::write(path.join("runtime/vendor").join(WHEEL), &bytes).unwrap();
        fs::write(path.join("runtime/semantica-source-lock.json"),serde_json::to_vec(&json!({"$schema":crate::runtime::SOURCE_LOCK_SCHEMA,"source":{"commit":"1".repeat(40),"version":VERSION},"artifact":{"filename":WHEEL,"sha256":sha256(&bytes)}})).unwrap()).unwrap();
        fs::write(path.join("runtime/requirements.txt"),format!("--find-links runtime/vendor\nsemantica[ontology-runtime]=={VERSION}\nfixture-dependency==1.0\n")).unwrap();
        executable(
            &path.join("fixture-python"),
            "fixture bootstrap; never executed\n",
        );
        if old {
            executable(
                &path.join("runtime/.venv/bin/python"),
                "old fixture interpreter; never executed\n",
            );
            fs::write(path.join("runtime/.venv/owner.txt"), "original bytes\n").unwrap();
        }
        Self {
            root: SkillRoot::new(path).unwrap(),
        }
    }
    fn target(&self) -> PathBuf {
        self.root.path().join("runtime/.venv")
    }
    fn output(&self) -> PathBuf {
        self.root.path().join("var/install-test")
    }
    fn options(&self) -> InstallOptions {
        let mut o = InstallOptions::new(self.root.path().join("fixture-python"), self.output());
        o.offline = true;
        o
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            fs::remove_dir_all(self.root.path()).unwrap();
        }
    }
}
fn executable(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
}
fn json_file(path: &Path) -> Value {
    strict_json(&fs::read(path).unwrap()).unwrap()
}
fn inventory(root: &Path) -> BTreeMap<String, String> {
    fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
        for e in fs::read_dir(dir).unwrap() {
            let path = e.unwrap().path();
            if path.is_dir() {
                walk(base, &path, out);
            } else {
                out.insert(
                    path.strip_prefix(base).unwrap().to_str().unwrap().into(),
                    sha256(fs::read(path).unwrap()),
                );
            }
        }
    }
    let mut output = BTreeMap::new();
    walk(root, root, &mut output);
    output
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mutation {
    None,
    InputAfterPreparation,
    PreparedWheelDuringFinal,
    ForeignFinalDirectory,
    RetargetLock,
    WrongPreparedSemantica,
    ForeignTargetDuringPreparation,
}
struct Fake {
    root: PathBuf,
    fail: Option<&'static str>,
    mutation: Mutation,
    calls: RefCell<Vec<Value>>,
    verified: Cell<usize>,
    replacement_guard: RefCell<Option<RuntimeAccessGuard>>,
    version: [u64; 3],
}
impl Fake {
    fn new(f: &Fixture) -> Self {
        Self {
            root: f.root.path().to_owned(),
            fail: None,
            mutation: Mutation::None,
            calls: RefCell::new(Vec::new()),
            verified: Cell::new(0),
            replacement_guard: RefCell::new(None),
            version: [3, 13, 1],
        }
    }
    fn ids(&self) -> Vec<String> {
        self.calls
            .borrow()
            .iter()
            .map(|v| v["id"].as_str().unwrap().into())
            .collect()
    }
}
impl Backend for Fake {
    fn origin(&self) -> &'static str {
        "fixture_no_real_install_or_semantic_execution"
    }
    fn run(&self, spec: &CommandSpec, evidence: &Path) -> Result<ProcessObservation> {
        let id = evidence.file_name().unwrap().to_str().unwrap();
        let output = evidence.parent().unwrap().parent().unwrap();
        let args: Vec<_> = spec
            .arguments
            .iter()
            .map(|s| s.to_str().unwrap().to_owned())
            .collect();
        let environment: BTreeMap<_, _> = spec
            .environment
            .iter()
            .map(|(key, value)| (key.to_str().unwrap(), value.to_str().unwrap()))
            .collect();
        let removed: Vec<_> = spec
            .remove_environment
            .iter()
            .map(|key| key.to_str().unwrap())
            .collect();
        let call = json!({"id":id,"executable":spec.executable,"arguments":args,"cwd":spec.cwd,"environment":environment,"remove_environment":removed,"execution_kind":"fixture"});
        self.calls.borrow_mut().push(call.clone());
        ensure!(spec.cwd == self.root, "fixture_wrong_cwd");
        for key in ["TMPDIR", "XDG_CACHE_HOME", "PIP_CACHE_DIR"] {
            let path = Path::new(spec.environment.get(&OsString::from(key)).unwrap());
            ensure!(
                path.starts_with(output),
                "fixture_cache_outside_transaction"
            );
        }
        ensure!(
            spec.remove_environment
                .contains(&OsString::from("PYTHONPATH")),
            "fixture_pythonpath_not_removed"
        );
        if id == "prepare-venv" || id == "create-final-venv" {
            let selected = PathBuf::from(args.last().unwrap());
            executable(
                &selected.join("bin/python"),
                &format!("fixture interpreter at {}\n", selected.display()),
            );
            fs::write(
                selected.join("pyvenv.cfg"),
                format!("fixture final-prefix = {}\n", selected.display()),
            )?;
            if id == "create-final-venv" {
                ensure!(
                    selected == self.root.join("runtime/.venv"),
                    "fixture_venv_was_relocated"
                );
                if self.mutation == Mutation::PreparedWheelDuringFinal {
                    fs::write(
                        output.join("wheels/fixture_dependency-1-py3-none-any.whl"),
                        "changed",
                    )?;
                }
                if self.mutation == Mutation::RetargetLock {
                    fs::remove_file(self.root.join("var/state/runtime-access.lock"))?;
                    *self.replacement_guard.borrow_mut() =
                        Some(RuntimeAccessGuard::shared(&self.root)?);
                }
            }
        }
        if id == "prepare-full-dependencies" {
            let index = args.iter().position(|a| a == "--wheel-dir").unwrap();
            let wheels = PathBuf::from(&args[index + 1]);
            fs::copy(
                self.root.join("runtime/vendor").join(WHEEL),
                wheels.join(WHEEL),
            )?;
            fs::write(
                wheels.join("pip-99-py3-none-any.whl"),
                "fixture pip closure",
            )?;
            fs::write(
                wheels.join("fixture_dependency-1-py3-none-any.whl"),
                "fixture dependency closure",
            )?;
            if self.mutation == Mutation::InputAfterPreparation {
                fs::write(
                    self.root.join("runtime/requirements.txt"),
                    "changed while preparing",
                )?;
            }
            if self.mutation == Mutation::WrongPreparedSemantica {
                fs::write(wheels.join(WHEEL), "wrong source artifact")?;
            }
            if self.mutation == Mutation::ForeignTargetDuringPreparation {
                fs::rename(
                    self.root.join("runtime/.venv"),
                    output.join("detached-original-venv"),
                )?;
                executable(
                    &self.root.join("runtime/.venv/bin/python"),
                    "external replacement interpreter\n",
                );
                fs::write(
                    self.root.join("runtime/.venv/external-owner.txt"),
                    "external replacement during preparation\n",
                )?;
            }
        }
        if id == "install-full-dependencies" && self.mutation == Mutation::ForeignFinalDirectory {
            fs::rename(
                self.root.join("runtime/.venv"),
                output.join("detached-owned-new-venv"),
            )?;
            fs::create_dir(self.root.join("runtime/.venv"))?;
            fs::write(
                self.root.join("runtime/.venv/external-owner.txt"),
                "do not replace",
            )?;
        }
        let failed = self.fail == Some(id);
        let stdout = if id == "python-version" {
            serde_json::to_vec(&self.version)?
        } else {
            format!("fixture stdout for {id}\n").into_bytes()
        };
        let stderr = if failed {
            format!("fixture partial failure for {id}\n").into_bytes()
        } else {
            Vec::new()
        };
        let status = if failed {
            "process_failed"
        } else {
            "completed"
        };
        let exit_code = Some(if failed { 7 } else { 0 });
        let report = json!({"status":status,"exit_code":exit_code,"execution_kind":"fixture","stdout_sha256":sha256(&stdout),"stderr_sha256":sha256(&stderr)});
        create_new_private_dir(evidence)?;
        write_new(&evidence.join("request.json"), &call)?;
        write_new(&evidence.join("process.json"), &report)?;
        write_new_bytes(&evidence.join("stdout.txt"), &stdout)?;
        write_new_bytes(&evidence.join("stderr.txt"), &stderr)?;
        Ok(ProcessObservation {
            status: status.into(),
            exit_code,
            stdout,
            stderr,
            report,
        })
    }
    fn verify(
        &self,
        _root: &SkillRoot,
        python: &Path,
        target: &Path,
        _wheel: &VerifiedWheel,
        _lock: &RuntimeSourceLock,
    ) -> Result<Value> {
        self.verified.set(self.verified.get() + 1);
        ensure!(
            target == self.root.join("runtime/.venv") && python == target.join("bin/python"),
            "fixture_wrong_final_identity_target"
        );
        ensure!(self.fail != Some("verify"), "fixture_verification_failed");
        Ok(
            json!({"status":"verified","execution_kind":"fixture","environment_prefix":target,"python_executable":python,"semantic_execution":"not_run"}),
        )
    }
}

#[test]
fn complete_install_creates_final_prefix_and_preserves_previous_environment() {
    let f = Fixture::new(true);
    let old = inventory(&f.target());
    let old_id = directory(&f.target()).unwrap();
    let backend = Fake::new(&f);
    let report = install_with(&f.root, &f.options(), &backend).unwrap();
    assert_eq!(report["status"], "ready");
    assert_eq!(report["origin"], backend.origin());
    assert_eq!(report["semantic_execution"], "not_run");
    assert_eq!(backend.verified.get(), 1);
    assert_eq!(old, inventory(&f.output().join("previous-venv")));
    assert_eq!(
        old_id,
        directory(&f.output().join("previous-venv")).unwrap()
    );
    assert_ne!(old_id, directory(&f.target()).unwrap());
    assert!(
        fs::read_to_string(f.target().join("pyvenv.cfg"))
            .unwrap()
            .contains(f.target().to_str().unwrap())
    );
    assert_eq!(json_file(&f.output().join("report.json")), report);
    assert!(f.output().join("ready.json").is_file());
    assert!(f.output().join("prepared-wheels.json").is_file());
    let calls = backend.calls.borrow();
    for id in ["prepare-full-dependencies", "install-full-dependencies"] {
        let call = calls.iter().find(|c| c["id"] == id).unwrap();
        let args = call["arguments"].as_array().unwrap();
        assert!(args.contains(&json!("--requirement")));
        assert!(!args.contains(&json!("--no-deps")));
        assert!(args.contains(&json!("--no-index")));
    }
    let locked = calls
        .iter()
        .find(|c| c["id"] == "install-locked-semantica")
        .unwrap();
    let args = locked["arguments"].as_array().unwrap();
    assert!(args.contains(&json!("--force-reinstall")));
    assert!(args.contains(&json!("--no-deps")));
    assert_eq!(
        args.last().unwrap(),
        &json!(f.root.path().join("runtime/vendor").join(WHEEL))
    );
    for call in calls
        .iter()
        .filter(|c| c["id"].as_str().unwrap().starts_with("install-"))
    {
        assert!(
            call["arguments"]
                .as_array()
                .unwrap()
                .contains(&json!("--isolated"))
        );
        assert_eq!(call["executable"], json!(f.target().join("bin/python")));
    }
    assert_eq!(report["service_switchover"], "not_atomic");
}

#[test]
fn final_pip_disables_inherited_configuration_and_uses_prepared_offline_closure() {
    let f = Fixture::new(true);
    let backend = Fake::new(&f);
    let wheelhouse = f.root.path().join("var/explicit-wheelhouse");
    fs::create_dir_all(&wheelhouse).unwrap();
    let mut options = f.options();
    options.offline = false;
    options.wheelhouse = Some(wheelhouse.clone());
    let report = install_with(&f.root, &options, &backend).unwrap();
    assert_eq!(report["status"], "ready");
    let calls = backend.calls.borrow();
    let preparation = calls
        .iter()
        .find(|call| call["id"] == "prepare-full-dependencies")
        .unwrap();
    let preparation_args = preparation["arguments"].as_array().unwrap();
    assert!(preparation_args.contains(&json!(wheelhouse)));
    assert!(!preparation_args.contains(&json!("--no-index")));
    for call in calls.iter().filter(|call| {
        call["id"].as_str().unwrap().starts_with("install-")
            || ["dependency-check", "installed-dependency-versions"]
                .contains(&call["id"].as_str().unwrap())
    }) {
        let arguments = call["arguments"].as_array().unwrap();
        assert!(arguments.contains(&json!("--isolated")));
        // Explicit null config overrides a hostile inherited PIP_CONFIG_FILE;
        // --isolated alone would still permit global/site pip configuration.
        assert_eq!(call["environment"]["PIP_CONFIG_FILE"], "/dev/null");
        for key in [
            "PYTHONPATH",
            "PYTHONHOME",
            "VIRTUAL_ENV",
            "PYTHONSTARTUP",
            "PIP_BUILD_TRACKER",
        ] {
            assert!(
                call["remove_environment"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(key))
            );
        }
        if call["id"].as_str().unwrap().starts_with("install-") {
            assert!(arguments.contains(&json!("--no-index")));
            let links: Vec<_> = arguments
                .windows(2)
                .filter(|pair| pair[0] == "--find-links")
                .map(|pair| &pair[1])
                .collect();
            assert_eq!(links, vec![&json!(f.output().join("wheels"))]);
            assert!(!arguments.contains(&json!(wheelhouse)));
        }
    }
}

#[test]
fn complete_foreign_target_created_during_preparation_is_not_claimed_or_replaced() {
    let f = Fixture::new(true);
    let original = inventory(&f.target());
    let original_identity = directory(&f.target()).unwrap();
    let mut backend = Fake::new(&f);
    backend.mutation = Mutation::ForeignTargetDuringPreparation;
    let report = install_with(&f.root, &f.options(), &backend).unwrap();
    assert_eq!(report["status"], "blocked");
    assert_eq!(
        report["reason"],
        "runtime_target_changed_during_preparation"
    );
    assert_eq!(report["recovery"]["status"], "original_untouched");
    assert_eq!(
        fs::read_to_string(f.target().join("external-owner.txt")).unwrap(),
        "external replacement during preparation\n"
    );
    assert_ne!(directory(&f.target()).unwrap(), original_identity);
    let retained = f.output().join("detached-original-venv");
    assert_eq!(inventory(&retained), original);
    assert_eq!(directory(&retained).unwrap(), original_identity);
    assert!(!f.output().join("previous-venv").exists());
    assert!(!f.output().join("replacement-intent.json").exists());
    assert!(!backend.ids().contains(&"create-final-venv".into()));
    assert_eq!(report["steps"].as_array().unwrap().len(), 3);
}

#[test]
fn every_process_failure_and_verification_failure_retains_trace_and_original() {
    for failed in [
        "python-version",
        "prepare-venv",
        "prepare-full-dependencies",
        "create-final-venv",
        "install-pip",
        "install-full-dependencies",
        "install-locked-semantica",
        "dependency-check",
        "installed-dependency-versions",
        "verify",
    ] {
        let f = Fixture::new(true);
        let before = inventory(&f.target());
        let before_id = directory(&f.target()).unwrap();
        let mut backend = Fake::new(&f);
        backend.fail = Some(failed);
        let report = install_with(&f.root, &f.options(), &backend).unwrap();
        assert_eq!(report["status"], "blocked", "{failed}: {report}");
        assert_eq!(report["runtime_ready"], false);
        assert_eq!(report["semantic_execution"], "not_run");
        assert!(!f.output().join("ready.json").exists());
        assert_eq!(before, inventory(&f.target()), "{failed}");
        assert_eq!(before_id, directory(&f.target()).unwrap());
        let before_replacement = [
            "python-version",
            "prepare-venv",
            "prepare-full-dependencies",
        ]
        .contains(&failed);
        assert_eq!(
            report["recovery"]["status"],
            if before_replacement {
                "original_untouched"
            } else {
                "original_directory_restored"
            }
        );
        if !before_replacement {
            assert!(f.output().join("failed-venv").is_dir());
            assert_eq!(report["recovery"]["byte_rollback_claimed"], false);
        }
        assert_eq!(
            report["steps"].as_array().unwrap().len(),
            backend.calls.borrow().len()
        );
        if failed != "verify" {
            let last = report["steps"].as_array().unwrap().last().unwrap();
            assert_eq!(last["id"], failed);
            assert_eq!(last["process"]["exit_code"], 7);
            assert!(
                !fs::read(f.output().join("steps").join(failed).join("stderr.txt"))
                    .unwrap()
                    .is_empty()
            );
        }
        assert_eq!(json_file(&f.output().join("report.json")), report);
    }
}

#[test]
fn fresh_failed_install_quarantines_partial_directory_without_claiming_old_restore() {
    let f = Fixture::new(false);
    let mut backend = Fake::new(&f);
    backend.fail = Some("create-final-venv");
    let report = install_with(&f.root, &f.options(), &backend).unwrap();
    assert_eq!(report["status"], "blocked");
    assert_eq!(report["recovery"]["status"], "no_previous_runtime");
    assert!(!f.target().exists());
    assert!(f.output().join("failed-venv/bin/python").exists());
    assert!(!f.output().join("previous-venv").exists());
}

#[test]
fn incompatible_bootstrap_and_preparation_drift_do_not_touch_existing_environment() {
    for mutation in [
        Mutation::None,
        Mutation::InputAfterPreparation,
        Mutation::WrongPreparedSemantica,
    ] {
        let f = Fixture::new(true);
        let before = inventory(&f.target());
        let mut backend = Fake::new(&f);
        backend.mutation = mutation;
        if mutation == Mutation::None {
            backend.version = [3, 9, 0];
        }
        let report = install_with(&f.root, &f.options(), &backend).unwrap();
        assert_eq!(report["status"], "blocked");
        assert_eq!(report["recovery"]["status"], "original_untouched");
        assert_eq!(before, inventory(&f.target()));
        assert!(!f.output().join("replacement-intent.json").exists());
        assert_eq!(backend.verified.get(), 0);
    }
}

#[test]
fn invalid_source_or_incomplete_old_environment_rejects_before_any_backend_call() {
    for kind in ["wheel", "requirements", "incomplete"] {
        let f = Fixture::new(true);
        match kind {
            "wheel" => {
                fs::write(f.root.path().join("runtime/vendor").join(WHEEL), "tampered").unwrap()
            }
            "requirements" => fs::write(
                f.root.path().join("runtime/requirements.txt"),
                "--index-url https://invalid.example\n",
            )
            .unwrap(),
            _ => fs::remove_file(f.target().join("bin/python")).unwrap(),
        }
        let before = inventory(&f.target());
        let backend = Fake::new(&f);
        assert!(install_with(&f.root, &f.options(), &backend).is_err());
        assert!(backend.ids().is_empty());
        assert_eq!(before, inventory(&f.target()));
        assert!(!f.output().exists());
    }
}

#[test]
fn bootstrap_python_must_not_be_the_environment_being_replaced() {
    let f = Fixture::new(true);
    let before = inventory(&f.target());
    let backend = Fake::new(&f);
    let mut options = f.options();
    options.python = f.target().join("bin/python");
    assert!(install_with(&f.root, &options, &backend).is_err());
    assert!(backend.ids().is_empty());
    assert_eq!(before, inventory(&f.target()));
    assert!(!f.output().exists());
    #[cfg(unix)]
    {
        let alias = f.root.path().join("bootstrap-alias");
        std::os::unix::fs::symlink(options.python, &alias).unwrap();
        options.python = alias;
        assert!(install_with(&f.root, &options, &backend).is_err());
        assert!(backend.ids().is_empty());
        assert_eq!(before, inventory(&f.target()));
        assert!(!f.output().exists());
    }
}

#[test]
fn output_cannot_occupy_runtime_coordination_lock_or_its_ancestors() {
    for output in [
        "var/state",
        "var/state/runtime-access.lock",
        "var/state/runtime-access.lock/child",
    ] {
        let f = Fixture::new(true);
        let before = inventory(&f.target());
        let backend = Fake::new(&f);
        let mut options = f.options();
        options.output = PathBuf::from(output);
        assert!(install_with(&f.root, &options, &backend).is_err());
        assert!(backend.ids().is_empty());
        assert_eq!(before, inventory(&f.target()));
        assert!(!f.root.path().join("var/state").exists());
    }
}

#[test]
fn prepared_dependency_bytes_changed_after_swap_are_quarantined_and_old_restored() {
    let f = Fixture::new(true);
    let before = inventory(&f.target());
    let mut backend = Fake::new(&f);
    backend.mutation = Mutation::PreparedWheelDuringFinal;
    let report = install_with(&f.root, &f.options(), &backend).unwrap();
    assert_eq!(report["status"], "blocked");
    assert!(
        report["reason"]
            .as_str()
            .unwrap()
            .contains("prepared_wheels_changed")
    );
    assert_eq!(before, inventory(&f.target()));
    assert!(f.output().join("failed-venv").is_dir());
    assert!(!backend.ids().contains(&"install-pip".into()));
}

#[test]
fn foreign_target_identity_blocks_recovery_instead_of_overwriting_it() {
    let f = Fixture::new(true);
    let before = inventory(&f.target());
    let mut backend = Fake::new(&f);
    backend.mutation = Mutation::ForeignFinalDirectory;
    let report = install_with(&f.root, &f.options(), &backend).unwrap();
    assert_eq!(report["status"], "blocked");
    assert_eq!(report["recovery"]["status"], "blocked");
    assert_eq!(
        fs::read_to_string(f.target().join("external-owner.txt")).unwrap(),
        "do not replace"
    );
    assert_eq!(before, inventory(&f.output().join("previous-venv")));
    assert!(
        f.output()
            .join("detached-owned-new-venv/bin/python")
            .exists()
    );
    assert!(!f.output().join("failed-venv").exists());
    assert!(f.output().join("replacement-started.json").exists());
}

#[test]
fn shared_runtime_reader_prevents_replacement_but_retains_preparation_evidence() {
    let f = Fixture::new(true);
    let before = inventory(&f.target());
    let guard = RuntimeAccessGuard::shared(f.root.path()).unwrap();
    let backend = Fake::new(&f);
    let report = install_with(&f.root, &f.options(), &backend).unwrap();
    assert_eq!(report["status"], "blocked");
    assert_eq!(report["reason"], "resource_busy");
    assert_eq!(report["recovery"]["status"], "original_untouched");
    assert_eq!(before, inventory(&f.target()));
    assert_eq!(
        backend.ids(),
        vec![
            "python-version",
            "prepare-venv",
            "prepare-full-dependencies"
        ]
    );
    assert!(f.output().join("prepared-wheels.json").is_file());
    assert!(!f.output().join("replacement-intent.json").exists());
    guard.ensure_current().unwrap();
}

#[test]
fn lost_coordination_lock_cannot_authorize_recovery_of_an_active_new_inode() {
    let f = Fixture::new(true);
    let before = inventory(&f.target());
    let mut backend = Fake::new(&f);
    backend.mutation = Mutation::RetargetLock;
    let report = install_with(&f.root, &f.options(), &backend).unwrap();
    assert_eq!(report["status"], "blocked");
    assert_eq!(report["recovery"]["status"], "blocked");
    assert_eq!(before, inventory(&f.output().join("previous-venv")));
    assert!(f.target().join("pyvenv.cfg").is_file());
    assert!(!f.output().join("failed-venv").exists());
    backend
        .replacement_guard
        .borrow()
        .as_ref()
        .unwrap()
        .ensure_current()
        .unwrap();
    assert!(!backend.ids().contains(&"install-pip".into()));
}

#[cfg(unix)]
#[test]
fn external_identity_probe_cannot_validate_a_different_environment_target() {
    let f = Fixture::new(true);
    let input = inputs(&f.root, &f.options().python).unwrap();
    let script = f.target().join("bin/python");
    let elsewhere = f.root.path().join("var/foreign-environment");
    let observation = json!({"python_version":[3,13,1],"python_executable":script,"environment_prefix":elsewhere,"imported_version":VERSION,"distribution_version":VERSION,"package_root":elsewhere.join("site-packages/semantica"),"imported_file":elsewhere.join("site-packages/semantica/__init__.py"),"imported_paths":[elsewhere.join("site-packages/semantica")],"direct_url":json!({"archive_info":{"hashes":{"sha256":input.lock.artifact_sha256}}}).to_string()});
    let raw = serde_json::to_string(&observation).unwrap();
    assert!(!raw.contains('\''));
    executable(&script, &format!("#!/bin/sh\nprintf '%s\\n' '{raw}'\n"));
    let error = External
        .verify(&f.root, &script, &f.target(), &input.wheel, &input.lock)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("installed_runtime_target_mismatch"),
        "{error:#}"
    );
}
