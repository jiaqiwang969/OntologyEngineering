//! New finite typed production-module content tests. Native callbacks use the
//! existing shared process API under test scope, not the formal build owner.
use anyhow::Result;
use oe_core::{
    SkillRoot,
    process::{CommandSpec, run_command},
    sha256, write_new_bytes,
};
use oe_evolution::{native_archive::*, *};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
const CLANG: &str =
    "/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/bin/clang";
const AR: &str = "/opt/homebrew/Cellar/llvm/22.1.1/bin/llvm-ar";
struct Fixture {
    root: SkillRoot,
    next: u64,
}
impl Fixture {
    fn new() -> Self {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../var/maintenance/typed-archive-actual-tests");
        std::fs::create_dir_all(&parent).unwrap();
        let unique = format!(
            "{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let parent = parent.canonicalize().unwrap();
        let p = parent.join(unique);
        std::fs::create_dir(&p).unwrap();
        write_new_bytes(
            &p.join("SKILL.md"),
            b"# New bounded typed content fixture\n",
        )
        .unwrap();
        std::fs::create_dir(p.join("var")).unwrap();
        std::fs::create_dir(p.join("var/target")).unwrap();
        std::fs::create_dir(p.join("var/processes")).unwrap();
        Self {
            root: SkillRoot::new(p).unwrap(),
            next: 0,
        }
    }
    fn path(&self, s: &str) -> PathBuf {
        self.root.path().join(s)
    }
    fn write(&self, s: &str, b: &[u8]) -> PathBuf {
        let p = self.path(s);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        write_new_bytes(&p, b).unwrap();
        p
    }
    fn reference(&self, p: &Path) -> ArtifactReference {
        ArtifactReference {
            path: p.to_str().unwrap().into(),
            sha256: sha256(std::fs::read(p).unwrap()),
        }
    }
    fn json(&mut self, value: &Value) -> ArtifactReference {
        let id = self.next;
        self.next += 1;
        let p = self.write(
            &format!("var/doc-{id}.json"),
            &serde_json::to_vec(value).unwrap(),
        );
        self.reference(&p)
    }
    fn process(
        &mut self,
        exe: &str,
        args: &[String],
        cwd: &Path,
        environment: &[CodeNativeEnvironmentEntry],
        role: &str,
    ) -> Result<CodeNativeInputProcessEvidence> {
        let id = self.next;
        self.next += 1;
        let dir = self.path(&format!("var/processes/p-{id}"));
        let spec = CommandSpec {
            executable: PathBuf::from(exe),
            arguments: args.iter().map(Into::into).collect(),
            cwd: cwd.into(),
            environment: environment
                .iter()
                .map(|x| (x.name.clone().into(), x.value.clone().into()))
                .collect::<BTreeMap<_, _>>(),
            remove_environment: Vec::new(),
            timeout: Duration::from_secs(30),
            output_limit: 1024 * 1024,
        };
        let obs = run_command(&spec, &dir)?;
        Ok(CodeNativeInputProcessEvidence {
            schema: "oev2.native-input-process-evidence/3".into(),
            role: role.into(),
            executable: bind_file(Path::new(exe))?,
            arguments: args.into(),
            cwd: cwd.to_str().unwrap().into(),
            environment: environment.into(),
            process: self.reference(&dir.join("process.json")),
            stdout: self.reference(&dir.join("stdout")),
            stderr: self.reference(&dir.join("stderr")),
            status: obs.status,
            exit_code: obs.exit_code.map(i64::from),
        })
    }
    fn compile(&mut self, name: &str, source: &str) -> PathBuf {
        let src = self.write(&format!("var/target/{name}.c"), source.as_bytes());
        let out = self.path(&format!("var/target/{name}.o"));
        let cwd = self.root.path().to_owned();
        let args = vec![
            "-c".into(),
            "-g".into(),
            "-O0".into(),
            src.to_str().unwrap().into(),
            "-o".into(),
            out.to_str().unwrap().into(),
        ];
        let p = self
            .process(CLANG, &args, &cwd, &[], "native_link")
            .unwrap();
        assert_eq!(p.exit_code, Some(0));
        verify_process_evidence(&self.root, &p).unwrap();
        out
    }
    fn archive(&mut self, members: &[PathBuf]) -> PathBuf {
        let out = self.path("var/target/original.rlib");
        let mut args = vec![
            "--format=darwin".into(),
            "qcU".into(),
            out.to_str().unwrap().into(),
        ];
        args.extend(members.iter().map(|x| x.to_str().unwrap().into()));
        let cwd = self.root.path().to_owned();
        let p = self.process(AR, &args, &cwd, &[], "archive_index").unwrap();
        assert_eq!(p.exit_code, Some(0));
        out
    }
    fn request(&mut self, args: Vec<String>) -> CodeNativeInputPreparationRequest {
        let id = self.next;
        self.next += 1;
        CodeNativeInputPreparationRequest {
            schema: "oev2.native-input-preparation-request/3".into(),
            algorithm: ALGORITHM.into(),
            received_arguments: args,
            caller_cwd: self.root.path().to_str().unwrap().into(),
            target_root: self.path("var/target").to_str().unwrap().into(),
            stage_root: self
                .path(&format!("var/stage-{id}"))
                .to_str()
                .unwrap()
                .into(),
            evidence_root: self
                .path(&format!("var/evidence-{id}"))
                .to_str()
                .unwrap()
                .into(),
            external_archives: Vec::new(),
            index_executable: bind_file(Path::new(AR)).unwrap(),
            index_environment: vec![CodeNativeEnvironmentEntry {
                name: "LC_ALL".into(),
                value: "C".into(),
            }],
        }
    }
    fn prepare(
        &mut self,
        r: &CodeNativeInputPreparationRequest,
    ) -> Result<CodeNativeInputPreparationFacts> {
        let root = SkillRoot::new(self.root.path())?;
        prepare_link_inputs(&root, r, &mut |q| {
            self.process(
                &q.executable.path,
                &q.arguments,
                Path::new(&q.cwd),
                &q.environment,
                "archive_index",
            )
        })
    }
    fn changed_report(
        &mut self,
        p: &CodeNativeInputProcessEvidence,
        change: impl FnOnce(&mut Value),
    ) -> CodeNativeInputProcessEvidence {
        let mut p = p.clone();
        let mut raw: Value =
            serde_json::from_slice(&std::fs::read(&p.process.path).unwrap()).unwrap();
        change(&mut raw);
        let id = self.next;
        self.next += 1;
        let dir = format!("var/raw-process-negative-{id}");
        let stdout = self.write(
            &format!("{dir}/stdout"),
            &std::fs::read(&p.stdout.path).unwrap(),
        );
        let stderr = self.write(
            &format!("{dir}/stderr"),
            &std::fs::read(&p.stderr.path).unwrap(),
        );
        let report = self.write(
            &format!("{dir}/process.json"),
            &serde_json::to_vec(&raw).unwrap(),
        );
        p.stdout = self.reference(&stdout);
        p.stderr = self.reference(&stderr);
        p.process = self.reference(&report);
        p
    }
}
fn rejected(root: &SkillRoot, f: &CodeNativeInputPreparationFacts, label: &str) {
    assert!(
        verify_prepared(root, f).is_err(),
        "unexpected content acceptance: {label}"
    );
}

#[test]
fn actual_darwin_ordered_archive_nested_response_and_independent_negative_matrix() {
    let mut f = Fixture::new();
    let main = f.compile(
        "main",
        "extern int needed(void); int main(void){ return needed()!=42; }\n",
    );
    let need = f.compile("needed", "int needed(void){return 42;}\n");
    let alpha = f.compile("alpha", "int alpha(void){return 1;}\n");
    let beta = f.compile("beta", "int beta(void){return 2;}\n");
    let unused = f.compile(
        "unused",
        "extern int deliberate_missing(void); int unused(void){return deliberate_missing();}\n",
    );
    let metadata = f.compile(
        "metadata",
        "typedef struct LocalMetadata {int x;} LocalMetadata;\n",
    );
    let a = f.write("var/target/dup-a/same.o", &std::fs::read(alpha).unwrap());
    let b = f.write("var/target/dup-b/same.o", &std::fs::read(beta).unwrap());
    let meta = f.write("var/target/lib.rmeta", &std::fs::read(metadata).unwrap());
    let archive = f.archive(&[need, a, b, unused, meta]);
    let nested = f.write(
        "var/target/nested response.rsp",
        format!("'{}' -Wl,-dead_strip", archive.display()).as_bytes(),
    );
    let outer = f.write(
        "var/target/outer response.rsp",
        format!(
            "'{}' '@{}' -o '{}'",
            main.display(),
            nested.display(),
            f.path("var/target/output").display()
        )
        .as_bytes(),
    );
    let r = f.request(vec![format!("@{}", outer.display())]);
    let facts = f.prepare(&r).unwrap();
    assert_eq!(facts.archives.len(), 1);
    assert_eq!(facts.direct_objects.len(), 1);
    assert_eq!(facts.responses.len(), 2);
    assert_eq!(facts.responses[1].parent_occurrence, Some(0));
    assert_eq!(facts.archives[0].members.len(), 5);
    assert_eq!(facts.archives[0].members[1].original_name, "same.o");
    assert_eq!(facts.archives[0].members[2].original_name, "same.o");
    assert_eq!(facts.archives[0].members[4].staged_name, "lib.rmeta");
    assert_eq!(facts.archives[0].structure_after.len(), 1);
    verify_prepared(&f.root, &facts).unwrap();
    verify_live_inputs(&facts).unwrap();
    let doc = f.json(&serde_json::to_value(&facts).unwrap());
    assert_eq!(
        NativeFullInputInspector { root: &f.root }
            .inspect_full_native_inputs(&doc)
            .unwrap(),
        facts
    );
    let native = f
        .process(
            CLANG,
            &facts.executed_arguments,
            Path::new(&r.stage_root),
            &[],
            "native_link",
        )
        .unwrap();
    assert_eq!(native.exit_code, Some(0));
    verify_process_evidence(&f.root, &native).unwrap();
    let run = f
        .process(
            f.path("var/target/output").to_str().unwrap(),
            &[],
            f.root.path().to_owned().as_path(),
            &[],
            "native_link",
        )
        .unwrap();
    assert_eq!(run.exit_code, Some(0));
    macro_rules! negative {
        ($name:expr,$body:expr) => {{
            let mut changed = facts.clone();
            ($body)(&mut changed);
            rejected(&f.root, &changed, $name);
        }};
    }
    negative!("member drop", |x: &mut CodeNativeInputPreparationFacts| {
        x.archives[0].members.pop();
    });
    negative!(
        "member reorder",
        |x: &mut CodeNativeInputPreparationFacts| x.archives[0].members.swap(1, 2)
    );
    negative!(
        "duplicate loss",
        |x: &mut CodeNativeInputPreparationFacts| {
            x.archives[0].members.remove(2);
        }
    );
    negative!("member attrs", |x: &mut CodeNativeInputPreparationFacts| {
        x.archives[0].members[0].date = "1".into()
    });
    negative!(
        "original name",
        |x: &mut CodeNativeInputPreparationFacts| x.archives[0].members[0].original_name =
            "other.rcgu.o".into()
    );
    negative!(
        "canonical rcgu leakage",
        |x: &mut CodeNativeInputPreparationFacts| x.archives[0].logical_path =
            "archives/original.rcgu.o.rlib".into()
    );
    negative!("staged name", |x: &mut CodeNativeInputPreparationFacts| x
        .archives[0]
        .members[0]
        .staged_name =
        "wrong.o".into());
    negative!(
        "payload digest",
        |x: &mut CodeNativeInputPreparationFacts| x.archives[0].members[0].payload_sha256 =
            "0".repeat(64)
    );
    negative!(
        "member fullheader ordinal",
        |x: &mut CodeNativeInputPreparationFacts| x.archives[0].members[0].staged_header_ordinal +=
            1
    );
    negative!(
        "structure drop",
        |x: &mut CodeNativeInputPreparationFacts| x.archives[0].structure_after.clear()
    );
    negative!(
        "structure name",
        |x: &mut CodeNativeInputPreparationFacts| x.archives[0].structure_after[0].name =
            "/".into()
    );
    negative!(
        "structure digest",
        |x: &mut CodeNativeInputPreparationFacts| x.archives[0].structure_after[0].payload_sha256 =
            "0".repeat(64)
    );
    negative!(
        "original whole binding",
        |x: &mut CodeNativeInputPreparationFacts| x.archives[0].original_file.sha256 =
            "0".repeat(64)
    );
    negative!(
        "index input payload order",
        |x: &mut CodeNativeInputPreparationFacts| x.archives[0]
            .index_request
            .ordered_payload_sha256
            .swap(1, 2)
    );
    negative!("index argv", |x: &mut CodeNativeInputPreparationFacts| x
        .archives[0]
        .index_process
        .arguments[1] =
        "sD".into());
    negative!("index cwd", |x: &mut CodeNativeInputPreparationFacts| x
        .archives[0]
        .index_process
        .cwd =
        r.target_root.clone());
    negative!("index role", |x: &mut CodeNativeInputPreparationFacts| x
        .archives[0]
        .index_process
        .role =
        "native_link".into());
    negative!("index failed", |x: &mut CodeNativeInputPreparationFacts| {
        x.archives[0].index_process.status = "process_failed".into()
    });
    negative!("index exit", |x: &mut CodeNativeInputPreparationFacts| x
        .archives[0]
        .index_process
        .exit_code =
        Some(1));
    negative!("index tool", |x: &mut CodeNativeInputPreparationFacts| x
        .archives[0]
        .index_process
        .executable
        .sha256 =
        "0".repeat(64));
    negative!(
        "index env value",
        |x: &mut CodeNativeInputPreparationFacts| x.archives[0].index_process.environment[0]
            .value = "POSIX".into()
    );
    negative!(
        "response parent",
        |x: &mut CodeNativeInputPreparationFacts| x.responses[1].parent_occurrence = None
    );
    negative!("response cwd", |x: &mut CodeNativeInputPreparationFacts| {
        x.responses[1].caller_cwd = r.target_root.clone()
    });
    negative!(
        "response order",
        |x: &mut CodeNativeInputPreparationFacts| x.responses[0].decoded_arguments.swap(0, 1)
    );
    negative!(
        "response duplicate",
        |x: &mut CodeNativeInputPreparationFacts| x.responses.push(x.responses[1].clone())
    );
    negative!(
        "response range",
        |x: &mut CodeNativeInputPreparationFacts| x.responses[0].expansion_end += 1
    );
    negative!(
        "direct component",
        |x: &mut CodeNativeInputPreparationFacts| x.direct_objects[0].argument_component_ordinal =
            Some(0)
    );
    negative!(
        "staged argv loss",
        |x: &mut CodeNativeInputPreparationFacts| {
            x.executed_arguments.remove(0);
        }
    );
    for (key, value) in [
        ("capture_complete", json!(false)),
        ("streams_collected", json!(1)),
        ("observed_status", json!("timed_out")),
        ("per_stream_limit", json!(0)),
        ("environment_added_keys", json!([])),
        ("environment_removed_keys", json!(["LC_ALL"])),
    ] {
        let mut changed = facts.clone();
        changed.archives[0].index_process =
            f.changed_report(&facts.archives[0].index_process, |v| v[key] = value);
        rejected(&f.root, &changed, key);
    }
    for stream in ["stdout", "stderr"] {
        let mut changed = facts.clone();
        changed.archives[0].index_process = f
            .changed_report(&facts.archives[0].index_process, |v| {
                v[stream]["truncated"] = json!(true)
            });
        rejected(&f.root, &changed, "truncated stream");
    }
    let mut changed = facts.clone();
    changed.archives[0].index_process = f.changed_report(&facts.archives[0].index_process, |v| {
        v["descendant_cleanup"]["state"] = json!("unverified");
    });
    rejected(&f.root, &changed, "cleanup missing");
    let mut changed = facts.clone();
    let bad = f.write("var/bad-payload.bytes", b"changed member");
    changed.archives[0].members[0].original_payload = f.reference(&bad);
    rejected(&f.root, &changed, "actual payload snapshot changed");
    let mut changed = facts.clone();
    let bad = f.write("var/bad-header.bytes", b"changed header");
    changed.archives[0].members[0].staged_header = f.reference(&bad);
    rejected(&f.root, &changed, "actual full header snapshot changed");
    let mut changed = facts.clone();
    let bad = f.write("var/bad-response.bytes", b"other");
    changed.responses[0].raw_bytes = f.reference(&bad);
    rejected(&f.root, &changed, "actual response raw bytes changed");
    let mut wire = serde_json::to_value(&facts.archives[0].members[0]).unwrap();
    wire["date"] = json!("");
    wire["uid"] = json!("");
    wire["gid"] = json!("");
    wire["mode"] = json!("");
    let member: CodeNativeArchiveMemberFact = serde_json::from_value(wire.clone()).unwrap();
    member.validate().unwrap();
    wire.as_object_mut().unwrap().remove("date");
    assert!(serde_json::from_value::<CodeNativeArchiveMemberFact>(wire).is_err());
    // Whole-archive keeps the original extraction semantics and component ordinal.
    let whole_request = f.request(vec![
        main.to_str().unwrap().into(),
        format!("-Wl,-force_load,{}", archive.display()),
        "-o".into(),
        f.path("var/target/whole-output").to_str().unwrap().into(),
    ]);
    let whole = f.prepare(&whole_request).unwrap();
    assert_eq!(whole.archives[0].argument_component_ordinal, Some(1));
    let whole_process = f
        .process(
            CLANG,
            &whole.executed_arguments,
            Path::new(&whole_request.stage_root),
            &[],
            "native_link",
        )
        .unwrap();
    assert_eq!(whole_process.status, "process_failed");
    assert_eq!(whole_process.exit_code, Some(1));
    verify_process_evidence(&f.root, &whole_process).unwrap();
    assert!(
        String::from_utf8(std::fs::read(&whole_process.stderr.path).unwrap())
            .unwrap()
            .contains("_deliberate_missing")
    );
    // This live-boundary failure is intentionally distinct from late CONTENT.
    std::fs::remove_file(&main).unwrap();
    assert!(verify_live_inputs(&facts).is_err());
    verify_prepared(&f.root, &facts).unwrap();
    let record = json!({"scope":"typed archive/response finite content only","positive":"actual fresh native archive + index callback + lazy native link/run exit0","negative_assertions":"named actual assertions in source, not a reclassified historical count","root":f.root.path(),"facts":doc,"fullrunner":"not_qualified","old_proofs":"not_inherited"});
    f.json(&record);
}

#[test]
fn native_success_report_cannot_attest_corrupt_rebuilt_symbol_index() {
    let mut f = Fixture::new();
    let object = f.compile("lib", "int value(void){return 1;}\n");
    let archive = f.archive(&[object]);
    let r = f.request(vec![
        archive.to_str().unwrap().into(),
        "-o".into(),
        f.path("var/target/x").to_str().unwrap().into(),
    ]);
    let root = SkillRoot::new(f.root.path()).unwrap();
    let err = prepare_link_inputs(&root, &r, &mut |q| {
        let process = f.process(
            &q.executable.path,
            &q.arguments,
            Path::new(&q.cwd),
            &q.environment,
            "archive_index",
        )?;
        let p = Path::new(&q.input.path);
        let mut b = std::fs::read(p)?;
        let name_field = std::str::from_utf8(&b[8..24])?.trim();
        let name_len: usize = name_field
            .strip_prefix("#1/")
            .ok_or_else(|| anyhow::anyhow!("actual Darwin extended index fixture required"))?
            .parse()?;
        let index_start = 8 + 60 + name_len;
        assert!(u32::from_le_bytes(b[index_start..index_start + 4].try_into()?) >= 8);
        b[index_start + 8..index_start + 12].copy_from_slice(&0u32.to_le_bytes());
        std::fs::write(p, &b)?;
        Ok(process)
    })
    .unwrap_err();
    assert!(
        format!("{err:#}").contains("archive_index_symbol_member_offset_mismatch"),
        "{err:#}"
    );
}

#[test]
fn response_occurrences_empty_quoted_tokens_and_bounded_limits() {
    let mut f = Fixture::new();
    let empty = f.write("var/target/empty.rsp", b"");
    let repeat = f.write("var/target/repeat.rsp", b"-Wl,-dead_strip");
    let args = vec![
        format!("@{}", empty.display()),
        format!("@{}", repeat.display()),
        format!("@{}", repeat.display()),
        "-o".into(),
        f.path("var/target/x").to_str().unwrap().into(),
    ];
    let r = f.request(args);
    let facts = f.prepare(&r).unwrap();
    assert_eq!(facts.responses.len(), 3);
    assert_eq!(
        facts.responses[0].expansion_start,
        facts.responses[0].expansion_end
    );
    assert_eq!(facts.expanded_arguments[0], facts.expanded_arguments[1]);
    assert_eq!(facts.responses[1].file, facts.responses[2].file);
    assert_ne!(facts.responses[1].occurrence, facts.responses[2].occurrence);
    verify_prepared(&f.root, &facts).unwrap();
    let grammar = f.write(
        "var/target/grammar.rsp",
        b"'-Wl,-dead_strip' \"-Wl,-dead_strip\" -Wl,-dead_strip '' -o var/target/y",
    );
    let r = f.request(vec![format!("@{}", grammar.display())]);
    let facts = f.prepare(&r).unwrap();
    assert_eq!(
        &facts.expanded_arguments[..4],
        &["-Wl,-dead_strip", "-Wl,-dead_strip", "-Wl,-dead_strip", ""]
    );
    verify_prepared(&f.root, &facts).unwrap();
    let huge = f.write("var/target/huge.rsp", &vec![b' '; 1024 * 1024 + 1]);
    let r = f.request(vec![format!("@{}", huge.display())]);
    assert!(f.prepare(&r).is_err());
    let many = f.write("var/target/many.rsp", &b"x ".repeat(10001));
    let r = f.request(vec![format!("@{}", many.display())]);
    assert!(f.prepare(&r).is_err());
    for i in 0..10 {
        f.write(
            &format!("var/target/depth-{i}.rsp"),
            if i < 9 {
                format!("@var/target/depth-{}.rsp", i + 1)
            } else {
                "-o var/target/deep-output".into()
            }
            .as_bytes(),
        );
    }
    let r = f.request(vec!["@var/target/depth-0.rsp".into()]);
    assert!(f.prepare(&r).is_err());
}

#[test]
fn original_gnu_container_is_explicitly_unsupported_without_index_callback() {
    let mut f = Fixture::new();
    let object = f.compile("gnu-long-member-name", "int value(void){return 1;}\n");
    let archive = f.path("var/target/gnu.a");
    let args = vec![
        "--format=gnu".into(),
        "qcU".into(),
        archive.to_str().unwrap().into(),
        object.to_str().unwrap().into(),
    ];
    let cwd = f.root.path().to_owned();
    assert_eq!(
        f.process(AR, &args, &cwd, &[], "archive_index")
            .unwrap()
            .exit_code,
        Some(0)
    );
    let r = f.request(vec![
        archive.to_str().unwrap().into(),
        "-o".into(),
        f.path("var/target/x").to_str().unwrap().into(),
    ]);
    let mut called = false;
    let e = prepare_link_inputs(&f.root, &r, &mut |_| {
        called = true;
        anyhow::bail!("cannot index GNU input")
    })
    .unwrap_err();
    assert!(format!("{e:#}").contains("native_input_gnu_archive_unsupported"));
    assert!(!called);
}

#[test]
fn output_hidden_io_and_wrong_side_fail_before_callback() {
    let mut f = Fixture::new();
    let own = f.write(
        "var/target/main.o",
        b"direct object byte fixture never linked",
    );
    let output = f.path("var/target/output");
    let base = vec![
        own.to_str().unwrap().into(),
        "-o".into(),
        output.to_str().unwrap().into(),
    ];
    let cases = vec![
        vec![
            "-o".into(),
            "var/target/a".into(),
            "-o".into(),
            "var/target/b".into(),
        ],
        vec!["-o".into(), "var/outside".into()],
        vec![
            "-Wl,-o,var/target/hidden".into(),
            "-o".into(),
            output.to_str().unwrap().into(),
        ],
        vec![
            "-Wl,@hidden".into(),
            "-o".into(),
            output.to_str().unwrap().into(),
        ],
        vec![
            "-Xlinker".into(),
            "-o".into(),
            "-o".into(),
            output.to_str().unwrap().into(),
        ],
        vec![
            "var/foreign.o".into(),
            "-o".into(),
            output.to_str().unwrap().into(),
        ],
    ];
    for args in cases {
        let r = f.request(args);
        let mut called = false;
        assert!(
            prepare_link_inputs(&f.root, &r, &mut |_| {
                called = true;
                anyhow::bail!("callback must not run")
            })
            .is_err()
        );
        assert!(!called);
        assert!(!Path::new(&r.stage_root).exists());
    }
    let r = f.request(base);
    let facts = f.prepare(&r).unwrap();
    assert_eq!(facts.archives.len(), 0);
    verify_prepared(&f.root, &facts).unwrap();
}

#[test]
fn malformed_response_unknown_container_and_callback_failure_refuse() {
    let mut f = Fixture::new();
    for (i, raw) in [
        b"'unclosed".as_slice(),
        b"trailing\\",
        b"\xff",
        b"\xef\xbb\xbf",
        b"\0",
    ]
    .iter()
    .enumerate()
    {
        let p = f.write(&format!("var/target/bad-{i}.rsp"), raw);
        let r = f.request(vec![format!("@{}", p.display())]);
        let mut called = false;
        assert!(
            prepare_link_inputs(&f.root, &r, &mut |_| {
                called = true;
                anyhow::bail!("no")
            })
            .is_err()
        );
        assert!(!called);
    }
    let cycle = f.write("var/target/cycle.rsp", b"@var/target/cycle.rsp");
    let r = f.request(vec![format!("@{}", cycle.display())]);
    assert!(f.prepare(&r).is_err());
    for (i, raw) in [b"!<thin>\n".as_slice(), b"!<arch>\ntruncated"]
        .iter()
        .enumerate()
    {
        let p = f.write(&format!("var/target/bad-{i}.a"), raw);
        let r = f.request(vec![
            p.to_str().unwrap().into(),
            "-o".into(),
            f.path("var/target/x").to_str().unwrap().into(),
        ]);
        assert!(f.prepare(&r).is_err());
    }
    let object = f.compile("lib", "int value(void){return 1;}\n");
    let archive = f.archive(&[object]);
    let r = f.request(vec![
        archive.to_str().unwrap().into(),
        "-o".into(),
        f.path("var/target/x").to_str().unwrap().into(),
    ]);
    let err = prepare_link_inputs(&f.root, &r, &mut |_| {
        anyhow::bail!("independent callback failed")
    })
    .unwrap_err();
    assert!(format!("{err:#}").contains("independent callback failed"));
}

#[test]
fn hidden_native_file_lists_libraries_and_bare_inputs_reject_before_callback() {
    let mut f = Fixture::new();
    let list = f.write("var/target/hidden-list.txt", b"untracked-native.o\n");
    let out = f.path("var/target/output").to_str().unwrap().to_owned();
    let cases = vec![
        vec!["-filelist".into(), list.to_str().unwrap().into()],
        vec![format!("-Wl,-filelist,{}", list.display())],
        vec![
            "-Xlinker".into(),
            "-filelist".into(),
            "-Xlinker".into(),
            list.to_str().unwrap().into(),
        ],
        vec![format!("-filelist={}", list.display())],
        vec![format!("-Wl,-order_file,{}", list.display())],
        vec![format!("-Wl,-exported_symbols_list,{}", list.display())],
        vec!["-bundle_loader".into(), list.to_str().unwrap().into()],
        vec!["-lcustom".into()],
        vec!["-l".into(), "custom".into()],
        vec!["-Wl,-lcustom".into()],
        vec!["-framework".into(), "Custom".into()],
        vec![format!("-Wl,-weak_library,{}", list.display())],
        vec![format!("--config={}", list.display())],
        vec![list.to_str().unwrap().into()],
        vec!["var/target/untracked.dylib".into()],
        vec!["var/target/untracked.tbd".into()],
        vec!["-Xlinker".into(), "var/target/untracked.dylib".into()],
        vec!["-Wl,var/target/untracked.txt".into()],
    ];
    for mut args in cases {
        args.extend(["-o".into(), out.clone()]);
        let request = f.request(args);
        let mut called = false;
        let result = prepare_link_inputs(&f.root, &request, &mut |_| {
            called = true;
            anyhow::bail!("hidden input callback must not run")
        });
        assert!(
            result.is_err(),
            "unexpected hidden input acceptance: {:?}",
            request.received_arguments
        );
        assert!(!called);
        assert!(!Path::new(&request.stage_root).exists());
    }
}
