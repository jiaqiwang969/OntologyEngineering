use super::*;
use super::{archive as ar, response};
use oe_core::{create_new_private_dir, write_new_bytes};

const UNCOVERED: [&str; 4] = [
    "content inspection grants no build owner or elapsed-time authority",
    "formal full profile/recipe/side/capture revision 3 is not connected here",
    "late snapshots do not establish live original input availability or compilation origin",
    "whole Semantica/platform/runner qualification is not established",
];
// Explicit full /3 inner staging relation; no old profile/2 guard changes.
fn full_stage_relation(target: &Path, stage: &Path) -> bool {
    let Some(parent) = target.parent() else {
        return false;
    };
    let Some(leaf) = target.file_name().and_then(|s| s.to_str()) else {
        return false;
    };
    let outer = parent.join(format!("{leaf}-native-stage"));
    let Ok(suffix) = stage.strip_prefix(outer) else {
        return false;
    };
    let parts = suffix.components().collect::<Vec<_>>();
    if parts.len() != 3 {
        return false;
    }
    let values = parts
        .iter()
        .map(|c| c.as_os_str().to_str())
        .collect::<Vec<_>>();
    let (Some(role), Some(call), Some(prefix)) = (values[0], values[1], values[2]) else {
        return false;
    };
    ["build-normalizer", "build-runner"].contains(&role)
        && prefix == "oe-native-debug"
        && call
            .strip_prefix("call-")
            .is_some_and(|s| s.len() == 6 && s.bytes().all(|c| c.is_ascii_digit()))
}
fn request_paths(
    root: &SkillRoot,
    r: &CodeNativeInputPreparationRequest,
) -> Result<(PathBuf, PathBuf, PathBuf, PathBuf)> {
    r.validate()
        .map_err(|e| anyhow::anyhow!("native_input_request:{e:?}"))?;
    ensure!(
        r.algorithm == ALGORITHM && r.received_arguments.len() <= 10000,
        "native_input_algorithm_or_argument_limit"
    );
    environment(&r.index_environment)?;
    request_paths_scoped(root, r, None)
}
fn request_paths_scoped(
    root: &SkillRoot,
    r: &CodeNativeInputPreparationRequest,
    approved_caller: Option<&Path>,
) -> Result<(PathBuf, PathBuf, PathBuf, PathBuf)> {
    r.validate()
        .map_err(|e| anyhow::anyhow!("native_input_request:{e:?}"))?;
    ensure!(
        r.algorithm == ALGORITHM && r.received_arguments.len() <= 10000,
        "native_input_algorithm_or_argument_limit"
    );
    environment(&r.index_environment)?;
    let cwd = root.expand(Path::new(&r.caller_cwd))?;
    ensure!(
        cwd.is_absolute()
            && (cwd.starts_with(root.path()) || approved_caller.is_some_and(|p| p == cwd))
            && cwd.is_dir()
            && path_text(&cwd)? == r.caller_cwd,
        "native_input_cwd_outside_root"
    );
    no_symlinks(&cwd)?;
    let target = root.private_path(Path::new(&r.target_root))?;
    let stage = root.private_path(Path::new(&r.stage_root))?;
    let evidence = root.private_path(Path::new(&r.evidence_root))?;
    ensure!(
        target.is_dir()
            && target != stage
            && (target.parent() == stage.parent() || full_stage_relation(&target, &stage))
            && !evidence.starts_with(&target)
            && !evidence.starts_with(&stage)
            && !target.starts_with(&evidence)
            && !stage.starts_with(&evidence),
        "native_input_side_paths_invalid"
    );
    ensure!(
        path_text(&target)? == r.target_root
            && path_text(&stage)? == r.stage_root
            && path_text(&evidence)? == r.evidence_root,
        "native_input_side_path_not_absolute"
    );
    verify_binding(&r.index_executable)?;
    ensure!(
        r.index_executable.mode & 0o111 != 0
            && !Path::new(&r.index_executable.path).starts_with(&target)
            && !Path::new(&r.index_executable.path).starts_with(&stage),
        "native_input_index_tool_invalid"
    );
    let mut names = BTreeSet::new();
    for f in &r.external_archives {
        verify_binding(f)?;
        ensure!(
            Path::new(&f.path)
                .extension()
                .is_some_and(|x| x == "a" || x == "rlib")
                && names.insert(&f.path)
                && !Path::new(&f.path).starts_with(&target)
                && !Path::new(&f.path).starts_with(&stage),
            "native_input_external_archive_scope"
        );
    }
    Ok((cwd, target, stage, evidence))
}
fn absolute(cwd: &Path, value: &str) -> Result<PathBuf> {
    let p = Path::new(value);
    let p = if p.is_absolute() {
        p.to_owned()
    } else {
        cwd.join(p)
    };
    no_symlinks(&p)?;
    Ok(p)
}
// These options read additional files or search for native libraries outside
// the generated direct/archive/response facts. The later full recipe must
// supply typed support rather than silently treating them as ordinary flags.
fn reject_hidden_input(token: &str) -> Result<()> {
    const FILE_OPTIONS: [&str; 16] = [
        "-filelist",
        "-order_file",
        "-exported_symbols_list",
        "-unexported_symbols_list",
        "-reexported_symbols_list",
        "-poison_symbols_list",
        "-sectcreate",
        "-sectorder",
        "-segaddr_table",
        "-bundle_loader",
        "-dylib_file",
        "-weak_library",
        "-lazy_library",
        "-reexport_library",
        "-upward_library",
        "-needed_library",
    ];
    ensure!(
        !FILE_OPTIONS.iter().any(|flag| token == *flag
            || token.starts_with(&format!("{flag}="))
            || token.starts_with(&format!("{flag}:"))),
        "native_input_hidden_file_option_unsupported"
    );
    ensure!(
        !(token.starts_with('-') && token.contains("framework"))
            && !token.starts_with("-F")
            && !token.starts_with("--config")
            && !token.starts_with("-Xclang")
            && !token.starts_with("-include"),
        "native_input_implicit_framework_or_config_unsupported"
    );
    ensure!(
        !token.starts_with("-l") || ["-lSystem", "-lc", "-lm"].contains(&token),
        "native_input_implicit_library_unsupported"
    );
    Ok(())
}
fn coordinate(arg: usize, component: Option<usize>) -> String {
    match component {
        Some(c) => format!("{arg:06}-c{c:06}"),
        None => format!("{arg:06}"),
    }
}
/// One exact ordered traversal serves preparation and rechecking. Unsupported
/// hidden response/output syntax is refused before any native index callback.
fn normalize<F>(args: &[String], cwd: &Path, target: &Path, mut input: F) -> Result<Vec<String>>
where
    F: FnMut(usize, Option<usize>, &Path) -> Result<String>,
{
    ensure!(args.len() <= 10000, "native_input_argument_limit");
    let output = args
        .iter()
        .enumerate()
        .filter(|(_, x)| x.as_str() == "-o")
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    ensure!(
        output.len() == 1 && output[0] + 1 < args.len(),
        "native_input_output_not_unique"
    );
    let outpath = absolute(cwd, &args[output[0] + 1])?;
    ensure!(
        outpath.starts_with(target) && outpath != target,
        "native_input_output_wrong_side"
    );
    let rewrite = |i: usize, c: Option<usize>, s: &str, input: &mut F| -> Result<String> {
        if s.ends_with(".o") || s.ends_with(".rlib") || s.ends_with(".a") {
            input(i, c, &absolute(cwd, s)?)
        } else {
            Ok(s.to_owned())
        }
    };
    let mut result = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        ensure!(
            !arg.contains('\0') && !arg.starts_with('@'),
            "native_input_unexpanded_response"
        );
        if i == output[0] + 1 {
            result.push(path_text(&outpath)?);
            continue;
        }
        reject_hidden_input(arg)?;
        ensure!(
            !arg.starts_with("--output")
                && !(arg.starts_with("-o") && arg != "-o" && !arg.starts_with("-object")),
            "native_input_hidden_output_unsupported"
        );
        if arg == "-Xlinker" {
            let next = args.get(i + 1).context("native_input_xlinker_missing")?;
            ensure!(
                !next.starts_with('@') && next != "-o" && !next.starts_with("--output"),
                "native_input_xlinker_hidden_io_unsupported"
            );
        }
        if let Some(w) = arg.strip_prefix("-Wl,") {
            let mut parts = w.split(',').map(str::to_owned).collect::<Vec<_>>();
            ensure!(
                parts.iter().all(|p| !p.starts_with('@')
                    && p != "-o"
                    && !p.starts_with("--output")
                    && !p.starts_with("-o=")),
                "native_input_wl_hidden_io_unsupported"
            );
            for (c, p) in parts.iter_mut().enumerate() {
                reject_hidden_input(p)?;
                let parameter = c > 0
                    && [
                        "-arch",
                        "-undefined",
                        "-rpath",
                        "-syslibroot",
                        "-install_name",
                        "-compatibility_version",
                        "-current_version",
                        "-oso_prefix",
                    ]
                    .contains(&w.split(',').nth(c - 1).unwrap_or(""));
                ensure!(
                    p.is_empty()
                        || p.starts_with('-')
                        || parameter
                        || p.ends_with(".o")
                        || p.ends_with(".a")
                        || p.ends_with(".rlib"),
                    "native_input_wl_untracked_file_unsupported"
                );
                *p = rewrite(i, Some(c), p, &mut input)?;
            }
            result.push(format!("-Wl,{}", parts.join(",")));
        } else if let Some(p) = arg.strip_prefix("-L") {
            if p.is_empty() {
                let next = args
                    .get(i + 1)
                    .context("native_input_search_path_missing")?;
                let _ = absolute(cwd, next)?;
                result.push(arg.clone());
            } else {
                result.push(format!("-L{}", path_text(&absolute(cwd, p)?)?));
            }
        } else if i > 0 && args[i - 1] == "-L" {
            result.push(path_text(&absolute(cwd, arg)?)?);
        } else {
            let parameter = i > 0
                && [
                    "-arch",
                    "-force_load",
                    "-undefined",
                    "-rpath",
                    "-syslibroot",
                    "-isysroot",
                    "-install_name",
                    "-compatibility_version",
                    "-current_version",
                ]
                .contains(&args[i - 1].as_str());
            ensure!(
                arg.is_empty()
                    || arg.starts_with('-')
                    || parameter
                    || arg.ends_with(".o")
                    || arg.ends_with(".a")
                    || arg.ends_with(".rlib"),
                "native_input_untracked_bare_file_unsupported"
            );
            result.push(rewrite(i, None, arg, &mut input)?);
        }
    }
    Ok(result)
}
fn structure(
    store: &mut Store<'_>,
    a: &ar::Archive,
) -> Result<Vec<CodeNativeArchiveStructureFact>> {
    a.structure
        .iter()
        .map(|s| {
            Ok(CodeNativeArchiveStructureFact {
                header_ordinal: s.ordinal as u64,
                name: s.name.clone(),
                header: store.save("struct-header", &s.header)?,
                name_bytes: store.save("struct-name", &s.name_bytes)?,
                payload: store.save("struct-payload", &s.payload)?,
                payload_sha256: sha256(&s.payload),
                payload_bytes: s.payload.len() as u64,
            })
        })
        .collect()
}
fn member_facts(
    store: &mut Store<'_>,
    original: &ar::Archive,
    staged: &ar::Archive,
) -> Result<Vec<CodeNativeArchiveMemberFact>> {
    original
        .members
        .iter()
        .zip(&staged.members)
        .map(|(a, b)| {
            Ok(CodeNativeArchiveMemberFact {
                member_ordinal: a.ordinal as u64,
                original_header_ordinal: a.header_ordinal as u64,
                staged_header_ordinal: b.header_ordinal as u64,
                original_name: a.name.clone(),
                staged_name: b.name.clone(),
                date: a.date.clone(),
                uid: a.uid.clone(),
                gid: a.gid.clone(),
                mode: a.mode.clone(),
                original_header: store.save("member-original-header", &a.header)?,
                staged_header: store.save("member-staged-header", &b.header)?,
                original_name_bytes: store.save("member-original-name", &a.name_bytes)?,
                staged_name_bytes: store.save("member-staged-name", &b.name_bytes)?,
                original_payload: store.save("member-original-payload", &a.payload)?,
                staged_payload: store.save("member-staged-payload", &b.payload)?,
                payload_sha256: a.sha256.clone(),
                payload_bytes: a.bytes as u64,
            })
        })
        .collect()
}
fn no_gnu(a: &ar::Archive) -> Result<()> {
    ensure!(
        a.structure
            .iter()
            .all(|s| !matches!(s.name.as_str(), "/" | "/SYM64/" | "//")),
        "native_input_gnu_archive_unsupported"
    );
    Ok(())
}

pub fn prepare_link_inputs<F>(
    root: &SkillRoot,
    r: &CodeNativeInputPreparationRequest,
    indexer: &mut F,
) -> Result<CodeNativeInputPreparationFacts>
where
    F: FnMut(&CodeNativeArchiveIndexRequest) -> Result<CodeNativeInputProcessEvidence>,
{
    prepare_link_inputs_checked(root, r, indexer, &mut |_| Ok(()), None)
}
/// Full-runtime admission checks share this preparation body. They run on
/// expanded original argv and all original files before any index callback,
/// and again on retained originals before staging. The pure public /3 port
/// preserves its original preparation contract.
pub(crate) fn prepare_link_inputs_checked<F, A>(
    root: &SkillRoot,
    r: &CodeNativeInputPreparationRequest,
    indexer: &mut F,
    check_arguments: &mut A,
    check_input: Option<&mut dyn FnMut(&Path, &[u8]) -> Result<()>>,
) -> Result<CodeNativeInputPreparationFacts>
where
    F: FnMut(&CodeNativeArchiveIndexRequest) -> Result<CodeNativeInputProcessEvidence>,
    A: FnMut(&[String]) -> Result<()>,
{
    prepare_link_inputs_scoped(root, r, indexer, check_arguments, check_input, None)
}
pub(crate) fn prepare_link_inputs_scoped<F, A>(
    root: &SkillRoot,
    r: &CodeNativeInputPreparationRequest,
    indexer: &mut F,
    check_arguments: &mut A,
    check_input: Option<&mut dyn FnMut(&Path, &[u8]) -> Result<()>>,
    approved_caller: Option<&Path>,
) -> Result<CodeNativeInputPreparationFacts>
where
    F: FnMut(&CodeNativeArchiveIndexRequest) -> Result<CodeNativeInputProcessEvidence>,
    A: FnMut(&[String]) -> Result<()>,
{
    prepare_link_inputs_scoped_verified(
        root,
        r,
        indexer,
        check_arguments,
        check_input,
        approved_caller,
        &mut |p| verify_process_evidence(root, p),
    )
}
pub(crate) fn prepare_link_inputs_scoped_verified<F, A>(
    root: &SkillRoot,
    r: &CodeNativeInputPreparationRequest,
    indexer: &mut F,
    check_arguments: &mut A,
    mut check_input: Option<&mut dyn FnMut(&Path, &[u8]) -> Result<()>>,
    approved_caller: Option<&Path>,
    process_check: &mut dyn FnMut(&CodeNativeInputProcessEvidence) -> Result<()>,
) -> Result<CodeNativeInputPreparationFacts>
where
    F: FnMut(&CodeNativeArchiveIndexRequest) -> Result<CodeNativeInputProcessEvidence>,
    A: FnMut(&[String]) -> Result<()>,
{
    let (cwd, target, stage, evidence) = request_paths_scoped(root, r, approved_caller)?;
    ensure!(
        !stage.exists() && !evidence.exists(),
        "native_input_stage_or_evidence_not_fresh"
    );
    // Validate every output/hidden input before creating side effects.
    let (expanded, _) = response::expand(&r.received_arguments, &cwd, |p| {
        ensure!(
            p.starts_with(root.path()),
            "native_response_outside_selected_root"
        );
        let f = bind_file(p)?;
        let b = read_regular_bytes(p, FILE_LIMIT)?;
        Ok((
            f,
            ArtifactReference {
                path: "var/preflight-not-retained".into(),
                sha256: sha256(&b),
            },
            b,
        ))
    })?;
    check_arguments(&expanded)?;
    normalize(&expanded, &cwd, &target, |_, _, p| {
        ensure!(
            p.starts_with(&target) || r.external_archives.iter().any(|x| Path::new(&x.path) == p),
            "native_input_unselected_file"
        );
        if let Some(check) = check_input.as_mut() {
            check(p, &read_regular_bytes(p, FILE_LIMIT)?)?;
        }
        Ok(path_text(p)?)
    })?;
    create_new_private_dir(&stage)?;
    create_new_private_dir(&evidence)?;
    create_new_private_dir(&stage.join("objects"))?;
    create_new_private_dir(&stage.join("archives"))?;
    let mut store = Store::new(root, evidence);
    let (expanded, responses) = response::expand(&r.received_arguments, &cwd, |p| {
        ensure!(
            p.starts_with(root.path()),
            "native_response_outside_selected_root"
        );
        let f = bind_file(p)?;
        let b = read_regular_bytes(p, FILE_LIMIT)?;
        let raw = store.save("response-raw", &b)?;
        ensure!(bind_file(p)? == f, "native_response_changed");
        Ok((f, raw, b))
    })?;
    check_arguments(&expanded)?;
    let mut objects = Vec::new();
    let mut archives = Vec::new();
    let executed = normalize(&expanded, &cwd, &target, |i, c, p| {
        if !p.starts_with(&target) {
            let external = r
                .external_archives
                .iter()
                .find(|x| Path::new(&x.path) == p)
                .context("native_input_unselected_external_archive")?;
            verify_binding(external)?;
            if let Some(check) = check_input.as_mut() {
                check(p, &read_regular_bytes(p, FILE_LIMIT)?)?;
            }
            return path_text(p);
        }
        let original_file = bind_file(p)?;
        let raw = read_regular_bytes(p, FILE_LIMIT)?;
        ensure!(
            sha256(&raw) == original_file.sha256,
            "native_input_source_changed"
        );
        if let Some(check) = check_input.as_mut() {
            check(p, &raw)?;
        }
        let original_bytes = store.save("input-original", &raw)?;
        let coord = coordinate(i, c);
        if p.extension().is_some_and(|x| x == "o") {
            let logical = format!("objects/{coord}-{}.o", original_file.sha256);
            let staged = stage.join(&logical);
            write_new_bytes(&staged, &raw)?;
            objects.push(CodeNativeDirectInputFact {
                input_argument_ordinal: i as u64,
                argument_component_ordinal: c.map(|x| x as u64),
                logical_path: logical.clone(),
                original_file,
                staged_file: bind_file(&staged)?,
                original_bytes,
                staged_bytes: store.save("input-staged", &raw)?,
            });
            return Ok(logical);
        }
        let original = ar::parse(&raw)?;
        no_gnu(&original)?;
        let stable = ar::stable(&original.members);
        let canonical = ar::canonical_digest(&stable)?;
        let suffix = p
            .extension()
            .and_then(|x| x.to_str())
            .context("native_archive_suffix")?;
        let logical = format!("archives/{coord}-{canonical}.{suffix}");
        let staged = stage.join(&logical);
        let before_index = ar::render(&stable)?;
        write_new_bytes(&staged, &before_index)?;
        let request = CodeNativeArchiveIndexRequest {
            schema: "oev2.native-archive-index-request/3".into(),
            input_argument_ordinal: i as u64,
            argument_component_ordinal: c.map(|x| x as u64),
            executable: r.index_executable.clone(),
            arguments: vec!["--format=darwin".into(), "sU".into(), logical.clone()],
            cwd: path_text(&stage)?,
            environment: r.index_environment.clone(),
            input: bind_file(&staged)?,
            input_bytes: store.save("archive-before-index", &before_index)?,
            ordered_payload_sha256: stable.iter().map(|m| m.sha256.clone()).collect(),
        };
        let process = indexer(&request)?;
        verify_index_process(root, &request, &process, process_check)?;
        let staged_raw = read_regular_bytes(&staged, FILE_LIMIT)?;
        let after = ar::parse(&staged_raw)?;
        no_gnu(&after)?;
        ar::equivalent(&stable, &after.members)?;
        ar::verify_index(&after)?;
        let evidence = CodeNativeArchiveRebuildEvidence {
            input_argument_ordinal: i as u64,
            argument_component_ordinal: c.map(|x| x as u64),
            original_path: path_text(p)?,
            logical_path: logical.clone(),
            original_file,
            staged_file: bind_file(&staged)?,
            original_bytes,
            staged_bytes: store.save("archive-after-index", &staged_raw)?,
            members: member_facts(&mut store, &original, &after)?,
            structure_before: structure(&mut store, &original)?,
            structure_after: structure(&mut store, &after)?,
            index_request: request,
            index_process: process,
        };
        archives.push(evidence);
        Ok(logical)
    })?;
    let facts = CodeNativeInputPreparationFacts {
        schema: "oev2.native-input-preparation-facts/3".into(),
        request: r.clone(),
        expanded_arguments: expanded,
        executed_arguments: executed,
        direct_objects: objects,
        archives,
        responses,
        uncovered: UNCOVERED.iter().map(|s| (*s).into()).collect(),
    };
    verify_prepared_scoped_verified(root, &facts, approved_caller, process_check)?;
    verify_live_inputs(&facts)?;
    Ok(facts)
}
fn verify_index_process(
    root: &SkillRoot,
    r: &CodeNativeArchiveIndexRequest,
    p: &CodeNativeInputProcessEvidence,
    process_check: &mut dyn FnMut(&CodeNativeInputProcessEvidence) -> Result<()>,
) -> Result<()> {
    r.validate()
        .map_err(|e| anyhow::anyhow!("native_index_request:{e:?}"))?;
    ensure!(
        p.role == "archive_index"
            && p.executable == r.executable
            && p.arguments == r.arguments
            && p.cwd == r.cwd
            && p.environment == r.environment
            && p.status == "completed"
            && p.exit_code == Some(0),
        "native_archive_index_callback_failed_or_mismatched"
    );
    process_check(p)
}
fn same_blob(root: &SkillRoot, r: &ArtifactReference, expected: &[u8]) -> Result<()> {
    ensure!(
        reference_bytes(root, r)? == expected,
        "native_archive_member_or_structure_snapshot_changed"
    );
    Ok(())
}
fn check_structure(
    root: &SkillRoot,
    f: &[CodeNativeArchiveStructureFact],
    a: &ar::Archive,
) -> Result<()> {
    ensure!(
        f.len() == a.structure.len(),
        "native_archive_structure_count_mismatch"
    );
    for (f, s) in f.iter().zip(&a.structure) {
        ensure!(
            f.header_ordinal == s.ordinal as u64
                && f.name == s.name
                && f.payload_sha256 == sha256(&s.payload)
                && f.payload_bytes == s.payload.len() as u64,
            "native_archive_structure_mismatch"
        );
        same_blob(root, &f.header, &s.header)?;
        same_blob(root, &f.name_bytes, &s.name_bytes)?;
        same_blob(root, &f.payload, &s.payload)?;
    }
    Ok(())
}
fn check_archive(
    root: &SkillRoot,
    r: &CodeNativeInputPreparationRequest,
    f: &CodeNativeArchiveRebuildEvidence,
    process_check: &mut dyn FnMut(&CodeNativeInputProcessEvidence) -> Result<()>,
) -> Result<()> {
    let original = ar::parse(&snapshot_matches(
        root,
        &f.original_bytes,
        &f.original_file,
    )?)?;
    let staged_raw = snapshot_matches(root, &f.staged_bytes, &f.staged_file)?;
    let staged = ar::parse(&staged_raw)?;
    no_gnu(&original)?;
    no_gnu(&staged)?;
    let stable = ar::stable(&original.members);
    ar::equivalent(&stable, &staged.members)?;
    ar::verify_index(&staged)?;
    verify_binding(&f.staged_file)?;
    let coord = coordinate(
        usize::try_from(f.input_argument_ordinal)?,
        f.argument_component_ordinal
            .map(usize::try_from)
            .transpose()?,
    );
    let suffix = Path::new(&f.original_path)
        .extension()
        .and_then(|x| x.to_str())
        .context("native_archive_original_suffix")?;
    ensure!(
        ["a", "rlib"].contains(&suffix)
            && f.original_path == f.original_file.path
            && f.logical_path
                == format!(
                    "archives/{coord}-{}.{suffix}",
                    ar::canonical_digest(&stable)?
                )
            && f.staged_file.path == path_text(&Path::new(&r.stage_root).join(&f.logical_path))?,
        "native_archive_canonical_path_mismatch"
    );
    ensure!(
        f.members.len() == original.members.len(),
        "native_archive_member_count_mismatch"
    );
    for ((fact, a), b) in f.members.iter().zip(&original.members).zip(&staged.members) {
        ensure!(
            fact.member_ordinal == a.ordinal as u64
                && fact.original_header_ordinal == a.header_ordinal as u64
                && fact.staged_header_ordinal == b.header_ordinal as u64
                && fact.original_name == a.name
                && fact.staged_name == b.name
                && fact.date == a.date
                && fact.uid == a.uid
                && fact.gid == a.gid
                && fact.mode == a.mode
                && fact.payload_sha256 == a.sha256
                && fact.payload_bytes == a.bytes as u64,
            "native_archive_member_fact_mismatch"
        );
        for (reference, bytes) in [
            (&fact.original_header, &a.header),
            (&fact.staged_header, &b.header),
            (&fact.original_name_bytes, &a.name_bytes),
            (&fact.staged_name_bytes, &b.name_bytes),
            (&fact.original_payload, &a.payload),
            (&fact.staged_payload, &b.payload),
        ] {
            same_blob(root, reference, bytes)?;
        }
    }
    check_structure(root, &f.structure_before, &original)?;
    check_structure(root, &f.structure_after, &staged)?;
    let request = &f.index_request;
    let before = ar::render(&stable)?;
    ensure!(
        request.input_argument_ordinal == f.input_argument_ordinal
            && request.argument_component_ordinal == f.argument_component_ordinal
            && request.executable == r.index_executable
            && request.environment == r.index_environment
            && request.arguments
                == vec![
                    "--format=darwin".to_owned(),
                    "sU".to_owned(),
                    f.logical_path.clone()
                ]
            && request.cwd == r.stage_root
            && request.input.path == f.staged_file.path
            && request.input.mode == 0o600
            && request.ordered_payload_sha256
                == stable.iter().map(|x| x.sha256.clone()).collect::<Vec<_>>(),
        "native_archive_index_request_mismatch"
    );
    ensure!(
        snapshot_matches(root, &request.input_bytes, &request.input)? == before,
        "native_archive_index_before_input_changed"
    );
    verify_index_process(root, request, &f.index_process, process_check)?;
    Ok(())
}
/// Retained original snapshots + all staged files and complete callback facts.
/// This function deliberately makes no claim that temporary originals survived.
pub fn verify_prepared(root: &SkillRoot, f: &CodeNativeInputPreparationFacts) -> Result<()> {
    verify_prepared_scoped(root, f, None)
}
pub(crate) fn verify_prepared_scoped(
    root: &SkillRoot,
    f: &CodeNativeInputPreparationFacts,
    approved_caller: Option<&Path>,
) -> Result<()> {
    verify_prepared_scoped_verified(root, f, approved_caller, &mut |p| {
        verify_process_evidence(root, p)
    })
}
pub(crate) fn verify_prepared_scoped_verified(
    root: &SkillRoot,
    f: &CodeNativeInputPreparationFacts,
    approved_caller: Option<&Path>,
    process_check: &mut dyn FnMut(&CodeNativeInputProcessEvidence) -> Result<()>,
) -> Result<()> {
    f.validate()
        .map_err(|e| anyhow::anyhow!("native_preparation_facts:{e:?}"))?;
    let (cwd, target, stage, _) = request_paths_scoped(root, &f.request, approved_caller)?;
    ensure!(
        stage.is_dir()
            && f.uncovered
                == UNCOVERED
                    .iter()
                    .map(|x| (*x).to_owned())
                    .collect::<Vec<_>>(),
        "native_input_boundary_or_stage_changed"
    );
    let mut next = 0usize;
    let (expanded, responses) = response::expand(&f.request.received_arguments, &cwd, |p| {
        let fact = f
            .responses
            .get(next)
            .context("native_response_fact_missing")?;
        next += 1;
        ensure!(
            fact.file.path == path_text(p)? && p.starts_with(root.path()),
            "native_response_path_mismatch"
        );
        let raw = snapshot_matches(root, &fact.raw_bytes, &fact.file)?;
        Ok((fact.file.clone(), fact.raw_bytes.clone(), raw))
    })?;
    ensure!(
        next == f.responses.len() && responses == f.responses && expanded == f.expanded_arguments,
        "native_response_expansion_fact_mismatch"
    );
    let mut objects = 0;
    let mut archives = 0;
    let executed = normalize(&expanded, &cwd, &target, |i, c, p| {
        if !p.starts_with(&target) {
            let pin = f
                .request
                .external_archives
                .iter()
                .find(|x| Path::new(&x.path) == p)
                .context("native_input_unselected_external_archive")?;
            verify_binding(pin)?;
            return path_text(p);
        }
        if p.extension().is_some_and(|x| x == "o") {
            let fact = f
                .direct_objects
                .get(objects)
                .context("native_direct_input_fact_missing")?;
            objects += 1;
            ensure!(
                fact.input_argument_ordinal == i as u64
                    && fact.argument_component_ordinal == c.map(|x| x as u64)
                    && fact.original_file.path == path_text(p)?,
                "native_direct_input_order_path_changed"
            );
            let original = snapshot_matches(root, &fact.original_bytes, &fact.original_file)?;
            ensure!(
                snapshot_matches(root, &fact.staged_bytes, &fact.staged_file)? == original
                    && fact.logical_path
                        == format!("objects/{}-{}.o", coordinate(i, c), sha256(&original))
                    && fact.staged_file.path == path_text(&stage.join(&fact.logical_path))?,
                "native_direct_input_staged_changed"
            );
            verify_binding(&fact.staged_file)?;
            Ok(fact.logical_path.clone())
        } else {
            let fact = f
                .archives
                .get(archives)
                .context("native_archive_fact_missing")?;
            archives += 1;
            ensure!(
                fact.input_argument_ordinal == i as u64
                    && fact.argument_component_ordinal == c.map(|x| x as u64)
                    && fact.original_path == path_text(p)?,
                "native_archive_input_order_changed"
            );
            check_archive(root, &f.request, fact, process_check)?;
            Ok(fact.logical_path.clone())
        }
    })?;
    ensure!(
        objects == f.direct_objects.len()
            && archives == f.archives.len()
            && executed == f.executed_arguments,
        "native_input_execution_arguments_or_counts_changed"
    );
    Ok(())
}
/// Owner calls at the live boundary, before rustc removes temporary inputs.
pub fn verify_live_inputs(f: &CodeNativeInputPreparationFacts) -> Result<()> {
    for object in &f.direct_objects {
        verify_binding(&object.original_file)?;
    }
    for archive in &f.archives {
        verify_binding(&archive.original_file)?;
    }
    for response in &f.responses {
        verify_binding(&response.file)?;
    }
    for external in &f.request.external_archives {
        verify_binding(external)?;
    }
    verify_binding(&f.request.index_executable)
}

#[cfg(test)]
mod full_stage_tests {
    use super::*;
    #[test]
    fn full_inner_stage_requires_exact_side_role_call_and_prefix() {
        let target = Path::new("/root/var/side-a");
        assert!(full_stage_relation(
            target,
            Path::new("/root/var/side-a-native-stage/build-normalizer/call-000000/oe-native-debug")
        ));
        assert!(full_stage_relation(
            target,
            Path::new("/root/var/side-a-native-stage/build-runner/call-000007/oe-native-debug")
        ));
        for value in [
            "/root/var/side-b-native-stage/build-runner/call-000000/oe-native-debug",
            "/root/var/side-a-native-stage/unselected/call-000000/oe-native-debug",
            "/root/var/side-a-native-stage/build-runner/call-0/oe-native-debug",
            "/root/var/side-a-native-stage/build-runner/call-000000/not-debug",
            "/root/var/side-a-native-stage/build-runner/call-000000/oe-native-debug/extra",
        ] {
            assert!(!full_stage_relation(target, Path::new(value)), "{value}");
        }
    }
}
