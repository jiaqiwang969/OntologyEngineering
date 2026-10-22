//! Complete exported-symbol-list content around the unchanged archive kernel.
//! This pure adapter creates no process or compiler/owner capability. In
//! particular, retained bytes do not prove that rustc emitted the original.
use anyhow::{Context, Result, ensure};
use oe_contracts::*;
use oe_core::{
    SkillRoot, create_new_private_dir, no_symlinks, read_regular_bytes, sha256, write_new_bytes,
};
use serde_json::json;
use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
};

const SCHEMA: &str = "oev2.native-link-file-input-request/7";
const FACTS: &str = "oev2.native-link-file-input-facts/7";
const FLAG: &str = "-exported_symbols_list";
const MAX_FILES: usize = 128;
const MAX_BYTES: usize = 16 * 1024 * 1024;
const UNCOVERED: [&str; 4] = [
    "complete captured content does not prove compiler emission or source authenticity",
    "actual compiler/stage/process/SDK association and native /6 integration remain required",
    "content DTOs grant no process owner, registry or elapsed-time capability",
    "complete pair byte/debug/signature and original whole library scope remain required",
];
#[derive(Debug, PartialEq)]
struct Occurrence {
    option: usize,
    value: usize,
    spelling: String,
    source: PathBuf,
}

fn text(p: &Path) -> Result<String> {
    Ok(p.to_str().context("link_files7_non_utf8_path")?.to_owned())
}
fn normal_absolute(p: &Path) -> Result<()> {
    ensure!(
        p.is_absolute()
            && p.components()
                .all(|c| !matches!(c, Component::CurDir | Component::ParentDir)),
        "link_files7_nonabsolute_or_traversal"
    );
    ensure!(
        !text(p)?.split('/').any(|s| s == "." || s == ".."),
        "link_files7_literal_dot_component"
    );
    no_symlinks(p)
}
fn paths(
    root: &SkillRoot,
    r: &CodeNativeLinkFileInputRequest7,
) -> Result<(PathBuf, PathBuf, PathBuf)> {
    r.validate()
        .map_err(|e| anyhow::anyhow!("link_files7_contract:{e:?}"))?;
    ensure!(
        r.schema == SCHEMA && r.received_arguments.len() <= 10_000,
        "link_files7_schema_or_argument_budget"
    );
    let cwd = PathBuf::from(&r.caller_cwd);
    normal_absolute(&cwd)?;
    ensure!(cwd.is_dir(), "link_files7_cwd_missing");
    let target = root.private_path(Path::new(&r.target_root))?;
    let capture = root.private_path(Path::new(&r.capture_root))?;
    normal_absolute(&target)?;
    normal_absolute(&capture)?;
    ensure!(
        text(&target)? == r.target_root
            && text(&capture)? == r.capture_root
            && target.is_dir()
            && capture != target
            && !capture.starts_with(&target)
            && !target.starts_with(&capture),
        "link_files7_target_capture_scope"
    );
    Ok((cwd, target, capture))
}
fn parse(
    r: &CodeNativeLinkFileInputRequest7,
    cwd: &Path,
    target: &Path,
) -> Result<(Vec<Occurrence>, Vec<String>, Vec<u64>)> {
    let args = &r.received_arguments;
    ensure!(
        args.iter()
            .all(|s| !s.contains('\0') && !s.starts_with('@')),
        "link_files7_unexpanded_or_nul_argument"
    );
    let mut files = Vec::new();
    let mut base = Vec::new();
    let mut ordinals = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        let forwarded = arg == &format!("-Wl,{FLAG}");
        if arg == FLAG || forwarded {
            let value = args
                .get(i + 1)
                .context("link_files7_missing_file_argument")?;
            let value = if forwarded {
                value
                    .strip_prefix("-Wl,")
                    .context("link_files7_forwarded_value_spelling")?
            } else {
                value
            };
            ensure!(
                !value.is_empty()
                    && !value.starts_with('-')
                    && !value.contains([',', '\0'])
                    && !value.starts_with('@'),
                "link_files7_file_argument_syntax"
            );
            let source = Path::new(value);
            ensure!(
                !value.split('/').any(|s| s == "." || s == ".."),
                "link_files7_literal_dot_component"
            );
            ensure!(
                source
                    .components()
                    .all(|c| !matches!(c, Component::CurDir | Component::ParentDir)),
                "link_files7_file_traversal"
            );
            let source = if source.is_absolute() {
                source.to_owned()
            } else {
                cwd.join(source)
            };
            normal_absolute(&source)?;
            ensure!(
                source.starts_with(target) && source != target,
                "link_files7_file_outside_target"
            );
            ensure!(files.len() < MAX_FILES, "link_files7_file_count_budget");
            files.push(Occurrence {
                option: i,
                value: i + 1,
                spelling: arg.clone(),
                source,
            });
            i += 2;
            continue;
        }
        // Do not accept a file option concealed inside another spelling. The
        // unchanged /3 parser remains responsible for all other file options.
        let hidden_export = arg.split(',').any(|s| {
            s == FLAG || s.starts_with(&format!("{FLAG}=")) || s.starts_with(&format!("{FLAG}:"))
        });
        ensure!(
            !hidden_export,
            "link_files7_export_option_spelling_unsupported"
        );
        base.push(arg.clone());
        ordinals.push(i as u64);
        i += 1;
    }
    Ok((files, base, ordinals))
}
fn logical(ordinal: usize, sha: &str) -> String {
    format!("{ordinal:06}-{sha}.exports")
}

pub fn prepare_link_file_inputs7(
    root: &SkillRoot,
    r: &CodeNativeLinkFileInputRequest7,
) -> Result<CodeNativeLinkFileInputFacts7> {
    let (cwd, target, capture) = paths(root, r)?;
    ensure!(!capture.exists(), "link_files7_capture_not_fresh");
    let (occurrences, base, ordinals) = parse(r, &cwd, &target)?;
    let mut originals = Vec::new();
    let mut bytes_total = 0usize;
    // Every source is validated before a capture directory is created.
    for occurrence in &occurrences {
        let binding = crate::native_archive::bind_file(&occurrence.source)?;
        let bytes = read_regular_bytes(&occurrence.source, MAX_BYTES)?;
        bytes_total = bytes_total
            .checked_add(bytes.len())
            .context("link_files7_aggregate_overflow")?;
        ensure!(
            bytes_total <= MAX_BYTES
                && sha256(&bytes) == binding.sha256
                && bytes.len() as u64 == binding.bytes,
            "link_files7_complete_source_budget_or_changed"
        );
        crate::native_archive::verify_binding(&binding)?;
        originals.push((binding, bytes));
    }
    if !originals.is_empty() {
        create_new_private_dir(&capture)?;
    }
    let mut files = Vec::new();
    for (occurrence, (original_file, bytes)) in occurrences.iter().zip(originals) {
        crate::native_archive::verify_binding(&original_file)?;
        let logical_path = logical(occurrence.option, &original_file.sha256);
        let saved = capture.join(&logical_path);
        write_new_bytes(&saved, &bytes)?;
        crate::native_archive::verify_binding(&original_file)?;
        files.push(CodeNativeLinkFileBinding7 {
            option_argument_ordinal: occurrence.option as u64,
            value_argument_ordinal: occurrence.value as u64,
            option_spelling: occurrence.spelling.clone(),
            original_file,
            captured_file: crate::native_archive::bind_file(&saved)?,
            raw_bytes: ArtifactReference {
                path: text(saved.strip_prefix(root.path())?)?,
                sha256: sha256(&bytes),
            },
            logical_path,
        });
    }
    let facts = CodeNativeLinkFileInputFacts7 {
        schema: FACTS.into(),
        request: r.clone(),
        base_arguments: base,
        base_argument_ordinals: ordinals,
        files,
        uncovered: UNCOVERED.iter().map(|s| (*s).into()).collect(),
    };
    verify_retained7(root, &facts)?;
    verify_live7(root, &facts)?;
    Ok(facts)
}

/// Late retained-content inspection deliberately does not restore original
/// availability. Rustc may already have removed its temporary source files.
pub fn verify_retained7(root: &SkillRoot, f: &CodeNativeLinkFileInputFacts7) -> Result<()> {
    f.validate()
        .map_err(|e| anyhow::anyhow!("link_files7_facts_contract:{e:?}"))?;
    ensure!(
        f.schema == FACTS && f.uncovered == UNCOVERED,
        "link_files7_facts_schema_or_scope_changed"
    );
    let (cwd, target, capture) = paths(root, &f.request)?;
    let (expected, base, ordinals) = parse(&f.request, &cwd, &target)?;
    ensure!(
        f.base_arguments == base
            && f.base_argument_ordinals == ordinals
            && f.files.len() == expected.len(),
        "link_files7_occurrence_map_changed"
    );
    let mut total = 0usize;
    let mut paths_seen = BTreeSet::new();
    for (occurrence, binding) in expected.iter().zip(&f.files) {
        binding
            .validate()
            .map_err(|e| anyhow::anyhow!("link_files7_binding_contract:{e:?}"))?;
        let selected = capture.join(logical(occurrence.option, &binding.original_file.sha256));
        ensure!(
            binding.option_argument_ordinal == occurrence.option as u64
                && binding.value_argument_ordinal == occurrence.value as u64
                && binding.option_spelling == occurrence.spelling
                && binding.original_file.path == text(&occurrence.source)?
                && binding.logical_path
                    == logical(occurrence.option, &binding.original_file.sha256)
                && binding.captured_file.path == text(&selected)?
                && paths_seen.insert(selected.clone())
                && root.private_path(Path::new(&binding.raw_bytes.path))? == selected,
            "link_files7_bound_occurrence_changed"
        );
        crate::native_archive::verify_binding(&binding.captured_file)?;
        let bytes = read_regular_bytes(&selected, MAX_BYTES)?;
        total = total
            .checked_add(bytes.len())
            .context("link_files7_aggregate_overflow")?;
        ensure!(
            total <= MAX_BYTES
                && sha256(&bytes) == binding.raw_bytes.sha256
                && binding.raw_bytes.sha256 == binding.original_file.sha256
                && binding.original_file.bytes == bytes.len() as u64,
            "link_files7_retained_bytes_changed"
        );
    }
    Ok(())
}
pub fn verify_live7(root: &SkillRoot, f: &CodeNativeLinkFileInputFacts7) -> Result<()> {
    verify_retained7(root, f)?;
    for file in &f.files {
        crate::native_archive::verify_binding(&file.original_file)?;
    }
    Ok(())
}

/// Combine only independently verified existing /3 input facts. No ordinary
/// JSON or arbitrary replacement vector becomes execution authority here.
pub fn execution_arguments7(
    root: &SkillRoot,
    f: &CodeNativeLinkFileInputFacts7,
    base: &CodeNativeInputPreparationFacts,
) -> Result<Vec<String>> {
    verify_retained7(root, f)?;
    crate::native_archive::verify_prepared(root, base)?;
    ensure!(
        base.request.received_arguments == f.base_arguments
            && base.expanded_arguments == f.base_arguments
            && base.request.caller_cwd == f.request.caller_cwd
            && base.request.target_root == f.request.target_root
            && base.executed_arguments.len() == f.base_arguments.len(),
        "link_files7_base_binding_changed"
    );
    let mut args = f.request.received_arguments.clone();
    for (ordinal, value) in f
        .base_argument_ordinals
        .iter()
        .zip(&base.executed_arguments)
    {
        args[usize::try_from(*ordinal)?] = value.clone();
    }
    for file in &f.files {
        args[usize::try_from(file.value_argument_ordinal)?] =
            if file.option_spelling.starts_with("-Wl,") {
                format!("-Wl,{}", file.captured_file.path)
            } else {
                file.captured_file.path.clone()
            };
    }
    Ok(args)
}

pub struct BorrowedNativeLinkFiles7<'a> {
    pub root: &'a SkillRoot,
}
impl CodeNativeLinkFileInputPort7 for BorrowedNativeLinkFiles7<'_> {
    fn prepare_link_file_inputs7(
        &self,
        input: &CodeNativeLinkFileInputRequest7,
    ) -> std::result::Result<CodeNativeLinkFileInputFacts7, FailureRecord> {
        prepare_link_file_inputs7(self.root, input).map_err(|e| FailureRecord {
            code: ErrorCode::InvalidContract, phase: "native_link_file_content7".into(), retryable: false,
            partial_result: Some(json!({"reason":format!("{e:#}"),"execution":"not_run","compiler_origin":"unknown"})),
        })
    }
}
