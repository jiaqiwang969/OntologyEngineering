//! Process-level book commands: isolated authored fixtures, no live semantic approval.
use oe_semantic::book_sources::{VOL1, VOL1_CHAPTERS, VOL2, VOL2_CHAPTERS};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
fn installed() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_owned()
}
fn write(path: &Path, data: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, data).unwrap();
}
struct Case(PathBuf);
impl Case {
    fn new() -> Self {
        let path = installed().join(format!(
            "var/maintenance/rust-tests/books-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        oe_core::create_new_private_dir(&path).unwrap();
        write(&path.join("SKILL.md"), "synthetic book CLI receiver\n");
        Self(path)
    }
    fn command(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_oev2"));
        cmd.current_dir(&self.0)
            .arg("--root")
            .arg(&self.0)
            .env_clear()
            .env("PATH", "")
            .env("HOME", self.0.join("var/empty-home"));
        cmd
    }
    fn run(&self, args: &[&str], code: i32) -> Value {
        let out = self.command().args(args).output().unwrap();
        assert_eq!(
            out.status.code(),
            Some(code),
            "{}",
            String::from_utf8_lossy(&out.stdout)
        );
        assert!(
            out.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    }
    fn corpus(&self) {
        let one = self.0.join("references").join(VOL1);
        let two = self.0.join("references").join(VOL2);
        for p in [
            "README.md",
            "resources/README.md",
            "handbook/README.md",
            "handbook/main.tex",
            "handbook/preamble.tex",
            "handbook/chapters/appB-glossary.tex",
            "handbook/build_handbook.py",
            "handbook/figures/image.png",
            "handbook/fragments/generated.tex",
        ] {
            write(&one.join(p), format!("fixture {p}\n"));
        }
        for (i, ch) in VOL1_CHAPTERS.iter().enumerate() {
            write(&one.join(ch).join("README.md"), "reader guide\n");
            write(
                &one.join(format!("handbook/chapters/ch{:02}.tex", i + 1)),
                "UniqueBookNeedle 可信😀 original source\n",
            );
        }
        for p in [
            "README.md",
            "propositions-index.md",
            "handbook/README.md",
            "front-matter/preface.md",
            "handbook/book-metadata.tex",
            "handbook/build_handbook.py",
            "handbook/build_isolated.py",
            "handbook/main.tex",
            "handbook/preamble.tex",
            "handbook/test_build_handbook.py",
            "handbook/figures-imagegen/a.png",
            "handbook/figures-rendered/a.png",
        ] {
            write(&two.join(p), format!("fixture {p}\n"));
        }
        for ch in VOL2_CHAPTERS {
            write(
                &two.join(ch).join("chapter.md"),
                "UniqueBookNeedle 可信😀 original source\n",
            );
            write(&two.join(ch).join("README.md"), "reader guide\n");
        }
        for name in [
            "a-semiconductor",
            "b-motorcycle-truck",
            "c-glossary",
            "d-method-tables",
        ] {
            write(
                &two.join(format!("appendices/appendix-{name}.md")),
                "appendix\n",
            );
        }
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
}

#[test]
fn source_checks_need_explicit_write_and_search_keeps_unicode_under_empty_path() {
    let c = Case::new();
    c.corpus();
    let first = c.run(&["books", "authoring-locks"], 1);
    assert_eq!(first["result"]["passed"], false);
    assert!(!c.0.join("var").exists());
    let written = c.run(&["books", "authoring-locks", "--write"], 0);
    assert_eq!(written["result"]["passed"], true);
    assert_eq!(written["result"]["write_recovery"]["atomic_batch"], false);
    let checked = c.run(&["books", "authoring-locks"], 0);
    assert!(
        checked["result"]["locks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|x| x["updated"] == false)
    );
    let hits = c.run(
        &[
            "books",
            "search",
            "UniqueBookNeedle",
            "可信😀",
            "--limit",
            "2",
            "--context",
            "0",
        ],
        0,
    );
    assert_eq!(hits["result"].as_array().unwrap().len(), 2);
    assert!(hits["result"].to_string().contains("可信😀"));
    assert_eq!(hits["semantic_execution"], "not_run");
    write(
        &c.0.join(format!("references/{VOL1}/handbook/chapters/ch01.tex")),
        "changed source",
    );
    let blocked = c.run(&["books", "search", "no match possible"], 2);
    assert_eq!(blocked["status"], "blocked");
    assert_eq!(
        c.run(&["books", "authoring-locks"], 1)["result"]["passed"],
        false
    );
}

#[test]
fn every_native_read_command_is_structured_and_blocked_without_a_runtime() {
    let c = Case::new();
    for args in [
        vec!["books", "migration-map", "--volume", "vol1"],
        vec!["books", "resolve-successor", "--old-path", "old/asset.ttl"],
        vec![
            "books",
            "asset-text",
            "--package",
            "package",
            "--asset",
            "asset",
        ],
        vec![
            "books",
            "chapter-asset-text",
            "--volume",
            "vol1",
            "--chapter",
            "ch01",
            "--asset",
            "asset",
        ],
        vec!["books", "verify-sources"],
    ] {
        let response = c.run(&args, 2);
        assert_eq!(response["status"], "blocked");
        assert_eq!(response["publication"], "not_performed");
    }
    assert!(!c.0.join("runtime/.venv").exists());
}

#[test]
fn invalid_book_arguments_and_engrave_overlap_fail_without_source_write_or_private_echo() {
    let c = Case::new();
    for args in [
        vec!["books", "migration-map", "--volume", "PRIVATE_BAD_VOLUME"],
        vec!["books", "engrave", "--source-root", "PRIVATE_VALUE"],
        vec!["books", "rebind", "--unknown", "PRIVATE_VALUE"],
    ] {
        let result = c.run(&args, 2);
        assert_eq!(result["error"]["code"], "invalid_arguments");
        assert!(!result.to_string().contains("PRIVATE_"));
    }
    fs::create_dir_all(c.0.join("var/source")).unwrap();
    c.run(
        &[
            "books",
            "engrave",
            "--source-root",
            "var/source",
            "--book-root",
            "var/source",
            "--output",
            "var/source",
            "--parts",
            "3",
            "--overwrite",
        ],
        2,
    );
    assert_eq!(fs::read_dir(c.0.join("var/source")).unwrap().count(), 0);
}

#[test]
fn rebind_refuses_installed_distribution_and_preserves_explicit_checkout_selection() {
    let c = Case::new();
    let checkout = c.0.join("explicit checkout");
    fs::create_dir(&checkout).unwrap();
    write(
        &checkout.join("semantica/chapter_packages/registry.yaml"),
        serde_json::to_vec(&json!({"packages":[]})).unwrap(),
    );
    let before = fs::read(checkout.join("semantica/chapter_packages/registry.yaml")).unwrap();
    c.run(
        &[
            "books",
            "rebind",
            "--semantica-root",
            "explicit checkout",
            "--write",
        ],
        2,
    );
    assert_eq!(
        fs::read(checkout.join("semantica/chapter_packages/registry.yaml")).unwrap(),
        before
    );
    let installed = c.0.join("runtime/site-packages");
    fs::create_dir_all(&installed).unwrap();
    c.run(
        &[
            "books",
            "rebind",
            "--semantica-root",
            "runtime/site-packages",
            "--write",
        ],
        2,
    );
    assert_eq!(fs::read_dir(installed).unwrap().count(), 0);
}

#[test]
fn rebind_check_then_explicit_write_updates_all_29_synthetic_authoring_packages() {
    let c = Case::new();
    let packages = c.0.join("checkout/semantica/chapter_packages");
    let mut records = Vec::new();
    let mut mappings = Vec::new();
    for (volume, count) in [("vol1", 9), ("vol2", 20)] {
        for number in 1..=count {
            let chapter = format!("ch{number:02}");
            let package = format!("semantica.chapter_packages.{volume}.{chapter}");
            let directory = packages.join(volume).join(&chapter);
            let anchor = if volume == "vol1" {
                format!("references/{VOL1}/handbook/chapters/{chapter}.tex")
            } else {
                format!("ontology-engineering/references/{VOL2}/{chapter}/chapter.md")
            };
            let relative = anchor
                .strip_prefix("ontology-engineering/")
                .unwrap_or(&anchor);
            write(&c.0.join(relative), "synthetic source 可信😀\n");
            let guide = if volume == "vol1" {
                c.0.join(format!(
                    "references/{VOL1}/{}/README.md",
                    VOL1_CHAPTERS[number - 1]
                ))
            } else {
                c.0.join(relative).parent().unwrap().join("README.md")
            };
            write(&guide, "synthetic guide\n");
            if volume == "vol2" {
                write(
                    &c.0.join(format!(
                        "references/{VOL2}/handbook/fragments/readme-{chapter}.tex"
                    )),
                    "synthetic TeX\n",
                );
            }
            let contract = json!({"external_specification":{"source_anchor":anchor}, "preserved_text":"保持😀"});
            write(
                &directory.join("contract.yaml"),
                serde_json::to_vec(&contract).unwrap(),
            );
            let mut manifest = json!({"assets":[{"asset_id":"chapter-contract","role":"chapter_contract","path":"contract.yaml","sha256":"b".repeat(64),"source_anchor":anchor}], "preserved_extra":{"x":7}});
            if volume == "vol1" {
                manifest["chapter"] = json!({"external_specification":{"source_anchor":anchor}});
            } else {
                manifest["book_source"] = json!({"logical_anchor":anchor, "sha256":"a".repeat(64)});
                mappings.push(json!({"new_package":package,"new_asset_id":"chapter-contract","new_sha256":"b".repeat(64)}));
            }
            write(
                &directory.join("manifest.yaml"),
                serde_json::to_vec(&manifest).unwrap(),
            );
            records.push(json!({"volume":volume,"chapter":chapter,"package_id":package,"manifest":format!("{volume}/{chapter}/manifest.yaml")}));
        }
    }
    write(
        &packages.join("registry.yaml"),
        serde_json::to_vec(&json!({"packages":records})).unwrap(),
    );
    write(
        &packages.join("vol2/migration-map.json"),
        serde_json::to_vec(&json!({"mappings":mappings})).unwrap(),
    );
    let watched = packages.join("vol2/ch01/contract.yaml");
    let before = fs::read(&watched).unwrap();
    let checked = c.run(&["books", "rebind", "--semantica-root", "checkout"], 1);
    assert_eq!(checked["result"]["package_count"], 29);
    assert_eq!(checked["result"]["error_count"], 0);
    assert_eq!(fs::read(&watched).unwrap(), before);
    assert!(!c.0.join("var").exists());
    let updated = c.run(
        &["books", "rebind", "--semantica-root", "checkout", "--write"],
        0,
    );
    assert_eq!(updated["result"]["write_recovery"]["status"], "completed");
    assert_eq!(updated["publication"], "not_performed");
    let raw: Value = serde_json::from_slice(&fs::read(watched).unwrap()).unwrap();
    assert_eq!(raw["preserved_text"], "保持😀");
    let final_check = c.run(&["books", "rebind", "--semantica-root", "checkout"], 0);
    assert_eq!(final_check["result"]["changed_count"], 0);
}

#[test]
fn maintenance_help_remains_clap_help() {
    let c = Case::new();
    let out = c
        .command()
        .args(["books", "rebind", "--help"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("--root") && text.contains("--semantica-root"));
}

#[test]
#[ignore = "Reads installed source-locked Semantica book APIs; no authoring changes"]
fn actual_native_text_is_emitted_in_full_with_exact_hash() {
    let root = installed();
    let run = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_oev2"))
            .current_dir(&root)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stdout)
        );
        serde_json::from_slice::<Value>(&out.stdout).unwrap()
    };
    let map = run(&["books", "migration-map", "--volume", "vol1"]);
    let first = &map["result"]["entries"][0];
    let result = run(&[
        "books",
        "asset-text",
        "--package",
        first["package_id"].as_str().unwrap(),
        "--asset",
        first["asset_id"].as_str().unwrap(),
    ]);
    assert_eq!(
        result["text_sha256"],
        oe_core::sha256(result["result"]["text"].as_str().unwrap())
    );
    assert_eq!(result["native_execution"]["result"], result["result"]);
}
