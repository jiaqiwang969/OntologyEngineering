//! Finite, source-bound DWARF predicates parsed from real independent observers.
//! UUIDs, file existence, zero exit or a searched name never suffice.
use crate::native_runtime::{
    NativeBuildSession, deadline_timeout, file_binding, read, reference, verify_pins,
};
use crate::*;
use anyhow::{Context, Result, ensure};
use oe_core::process::{CommandSpec, run_command};
use oe_core::{create_new_private_dir, read_regular_bytes, write_new, write_new_bytes};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Clone)]
struct Die {
    offset: u64,
    tag: String,
    depth: usize,
    parent: Option<usize>,
    attrs: BTreeMap<String, String>,
}
fn hex(s: &str) -> Result<u64> {
    Ok(u64::from_str_radix(s.trim().trim_start_matches("0x"), 16)?)
}
fn quoted(s: &str) -> Result<String> {
    let start = s.find('"').context("native_dwarf_quote_missing")?;
    let end = s.rfind('"').context("native_dwarf_quote_missing")?;
    ensure!(end > start, "native_dwarf_quote_invalid");
    let text = &s[start + 1..end];
    ensure!(
        !text.contains(['\\', '"', '\n', '\r']),
        "native_dwarf_escaped_text_uncovered"
    );
    Ok(text.into())
}
fn number(s: &str) -> Result<u64> {
    let value = s
        .trim()
        .strip_prefix('(')
        .context("native_dwarf_number")?
        .split(')')
        .next()
        .context("native_dwarf_number")?;
    if value.starts_with("0x") {
        hex(value)
    } else {
        Ok(value.parse()?)
    }
}
fn dies(text: &str) -> Result<Vec<Die>> {
    let mut out: Vec<Die> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    let mut current = None;
    for line in text.lines() {
        if let Some((prefix, rest)) = line.split_once(':') {
            if prefix.starts_with("0x")
                && (rest.trim_start().starts_with("DW_TAG_") || rest.trim() == "NULL")
            {
                let depth = rest.len() - rest.trim_start().len();
                while stack.last().is_some_and(|i| out[*i].depth >= depth) {
                    stack.pop();
                }
                if rest.trim() == "NULL" {
                    current = None;
                    continue;
                }
                let tag = rest
                    .trim()
                    .split_whitespace()
                    .next()
                    .context("native_dwarf_tag")?
                    .to_owned();
                let index = out.len();
                out.push(Die {
                    offset: hex(prefix)?,
                    tag,
                    depth,
                    parent: stack.last().copied(),
                    attrs: BTreeMap::new(),
                });
                stack.push(index);
                current = Some(index);
                continue;
            }
        }
        let trimmed = line.trim();
        if trimmed.starts_with("DW_AT_") {
            let i = current.context("native_dwarf_attribute_without_die")?;
            let split = trimmed
                .find(char::is_whitespace)
                .context("native_dwarf_attribute")?;
            let name = &trimmed[..split];
            ensure!(
                out[i]
                    .attrs
                    .insert(name.into(), trimmed[split..].trim().into())
                    .is_none(),
                "native_dwarf_duplicate_attribute"
            );
        }
    }
    Ok(out)
}
fn attr<'a>(d: &'a Die, name: &str) -> Result<&'a str> {
    d.attrs
        .get(name)
        .map(String::as_str)
        .context("native_dwarf_required_attribute_missing")
}
fn name(d: &Die) -> Result<String> {
    quoted(attr(d, "DW_AT_name")?)
}
fn qualified(all: &[Die], index: usize) -> Result<String> {
    let mut parts = vec![name(&all[index])?];
    let mut parent = all[index].parent;
    while let Some(i) = parent {
        if [
            "DW_TAG_namespace",
            "DW_TAG_structure_type",
            "DW_TAG_class_type",
        ]
        .contains(&all[i].tag.as_str())
        {
            parts.push(name(&all[i])?);
        }
        parent = all[i].parent;
    }
    parts.reverse();
    Ok(parts.join("::"))
}
fn ancestor(all: &[Die], index: usize, tag: &str) -> Option<usize> {
    let mut p = all[index].parent;
    while let Some(i) = p {
        if all[i].tag == tag {
            return Some(i);
        }
        p = all[i].parent;
    }
    None
}
fn range(d: &Die) -> Result<(u64, u64)> {
    let lo = number(attr(d, "DW_AT_low_pc")?)?;
    let hi = number(attr(d, "DW_AT_high_pc")?)?;
    ensure!(lo < hi, "native_dwarf_empty_range");
    Ok((lo, hi))
}
fn type_ref(d: &Die) -> Result<(u64, String)> {
    let value = attr(d, "DW_AT_type")?;
    let offset = value
        .trim()
        .trim_start_matches('(')
        .split_whitespace()
        .next()
        .context("native_dwarf_type_ref")?;
    Ok((hex(offset)?, quoted(value)?))
}
fn match_type(c: &CodeNativeDebugExpectation, actual: &str) -> Result<()> {
    let expected = c
        .expected_type
        .as_ref()
        .context("native_expected_type_missing")?;
    ensure!(
        if c.matcher == "exact" {
            actual == expected
        } else if c.matcher == "type_contains" {
            actual.contains(expected)
        } else {
            false
        },
        "native_debug_type_mismatched"
    );
    Ok(())
}
fn source_file(root: &SkillRoot, c: &CodeNativeDebugExpectation) -> Result<PathBuf> {
    let path = root.expand(Path::new(&c.source.path))?;
    ensure!(
        path.starts_with(root.path()),
        "native_debug_source_outside_root"
    );
    let b = read_regular_bytes(&path, 16 * 1024 * 1024)?;
    ensure!(sha256(&b) == c.source.sha256, "native_debug_source_changed");
    Ok(path)
}
fn select_function(
    root: &SkillRoot,
    c: &CodeNativeDebugExpectation,
    text: &str,
) -> Result<(Vec<Die>, usize)> {
    let all = dies(text)?;
    let source = source_file(root, c)?;
    let mut candidates = Vec::new();
    for (i, d) in all.iter().enumerate() {
        if d.tag == "DW_TAG_subprogram"
            && qualified(&all, i).ok().as_deref() == Some(c.function.as_str())
            && quoted(attr(d, "DW_AT_decl_file")?).ok().as_deref() == source.to_str()
        {
            range(d)?;
            ensure!(
                attr(d, "DW_AT_frame_base")?.contains("DW_OP_"),
                "native_function_frame_missing"
            );
            candidates.push(i);
        }
    }
    ensure!(
        candidates.len() == 1,
        "native_debug_function_ambiguous_or_missing"
    );
    Ok((all, candidates[0]))
}
fn select_variable(all: &[Die], function: usize, c: &CodeNativeDebugExpectation) -> Result<usize> {
    let tag = if c.kind == "parameter" {
        "DW_TAG_formal_parameter"
    } else {
        "DW_TAG_variable"
    };
    let mut candidates = Vec::new();
    for (i, d) in all.iter().enumerate() {
        if d.tag == tag
            && name(d).ok().as_deref() == Some(c.name.as_str())
            && ancestor(all, i, "DW_TAG_subprogram") == Some(function)
        {
            candidates.push(i);
        }
    }
    ensure!(
        candidates.len() == 1,
        "native_debug_variable_ambiguous_or_missing"
    );
    let index = candidates[0];
    let d = &all[index];
    ensure!(
        attr(d, "DW_AT_location")?.starts_with("(DW_OP_") && attr(d, "DW_AT_location")? != "()",
        "native_debug_variable_location_unknown"
    );
    ensure!(
        quoted(attr(d, "DW_AT_decl_file")?)? == quoted(attr(&all[function], "DW_AT_decl_file")?)?,
        "native_debug_variable_source_mismatched"
    );
    if let Some(line) = c.line {
        ensure!(
            number(attr(d, "DW_AT_decl_line")?)? == line,
            "native_debug_variable_line_mismatched"
        );
    }
    let fr = range(&all[function])?;
    let mut p = d.parent;
    while let Some(i) = p {
        if i == function {
            break;
        }
        ensure!(
            all[i].tag == "DW_TAG_lexical_block",
            "native_debug_variable_scope_unknown"
        );
        let r = range(&all[i])?;
        ensure!(
            r.0 >= fr.0 && r.1 <= fr.1,
            "native_debug_variable_scope_mismatched"
        );
        p = all[i].parent;
    }
    Ok(index)
}
fn normalize(path: &Path) -> Result<PathBuf> {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::ParentDir => ensure!(out.pop(), "native_debug_line_path_escape"),
            Component::CurDir => {}
            _ => out.push(c.as_os_str()),
        }
    }
    Ok(out)
}
fn line_matches(
    c: &CodeNativeDebugExpectation,
    all: &[Die],
    function: usize,
    text: &str,
    source: &Path,
) -> Result<()> {
    let cu = ancestor(all, function, "DW_TAG_compile_unit").context("native_debug_cu_missing")?;
    let offset = number(attr(&all[cu], "DW_AT_stmt_list")?)?;
    let comp_dir = PathBuf::from(quoted(attr(&all[cu], "DW_AT_comp_dir")?)?);
    ensure!(
        text.contains(&format!("debug_line[0x{offset:08x}]")),
        "native_debug_wrong_line_table"
    );
    let mut directories = BTreeMap::<u64, PathBuf>::new();
    let mut files = BTreeMap::<u64, (String, u64)>::new();
    let mut current = None;
    let mut rows = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("include_directories[") {
            let end = t.find(']').context("native_debug_directory")?;
            let idx = t[20..end].trim().parse()?;
            directories.insert(idx, PathBuf::from(quoted(t)?));
        } else if t.starts_with("file_names[") {
            let end = t.find(']').context("native_debug_file")?;
            let idx = t[11..end].trim().parse()?;
            current = Some(idx);
            files.insert(idx, (String::new(), 0));
        } else if t.starts_with("name:") {
            if let Some(i) = current {
                files.get_mut(&i).unwrap().0 = quoted(t)?;
            }
        } else if t.starts_with("dir_index:") {
            if let Some(i) = current {
                files.get_mut(&i).unwrap().1 = t[10..].trim().parse()?;
            }
        } else if t.starts_with("0x") {
            let fields = t.split_whitespace().collect::<Vec<_>>();
            if fields.len() >= 8 {
                rows.push((
                    hex(fields[0])?,
                    fields[1].parse::<u64>()?,
                    fields[3].parse::<u64>()?,
                    fields.contains(&"end_sequence"),
                ));
            }
        }
    }
    let mut indexes = Vec::new();
    for (i, (name, dir)) in files {
        let path = PathBuf::from(name);
        let actual = if path.is_absolute() {
            path
        } else {
            let base = directories.get(&dir).cloned().unwrap_or_default();
            let base = if base.is_absolute() {
                base
            } else {
                comp_dir.join(base)
            };
            base.join(path)
        };
        if normalize(&actual)? == source {
            indexes.push(i);
        }
    }
    let fr = range(&all[function])?;
    let wanted = c.line.context("native_debug_expected_line_missing")?;
    ensure!(
        rows.iter().any(|r| r.1 == wanted
            && indexes.contains(&r.2)
            && r.0 >= fr.0
            && r.0 < fr.1
            && !r.3),
        "native_debug_source_line_mismatched"
    );
    Ok(())
}
fn leaf_type_matches(
    c: &CodeNativeDebugExpectation,
    offset: u64,
    label: &str,
    text: &str,
) -> Result<()> {
    let all = dies(text)?;
    let hits = all
        .iter()
        .enumerate()
        .filter(|(_, d)| d.offset == offset)
        .collect::<Vec<_>>();
    ensure!(hits.len() == 1, "native_debug_type_ref_unresolved");
    let (i, d) = hits[0];
    ensure!(
        [
            "DW_TAG_base_type",
            "DW_TAG_structure_type",
            "DW_TAG_pointer_type",
            "DW_TAG_reference_type",
            "DW_TAG_typedef",
            "DW_TAG_const_type",
            "DW_TAG_array_type",
            "DW_TAG_enumeration_type"
        ]
        .contains(&d.tag.as_str()),
        "native_debug_type_tag_unknown"
    );
    let actual = qualified(&all, i)?;
    ensure!(
        actual == label || name(d)? == label,
        "native_debug_type_reference_label_mismatched"
    );
    if d.tag == "DW_TAG_base_type" {
        let bytes = number(attr(d, "DW_AT_byte_size")?)?;
        ensure!(bytes > 0, "native_debug_empty_base_type");
        let expected = match actual.as_str() {
            "i8" => Some((1, "DW_ATE_signed")),
            "i16" => Some((2, "DW_ATE_signed")),
            "i32" => Some((4, "DW_ATE_signed")),
            "i64" => Some((8, "DW_ATE_signed")),
            "u8" => Some((1, "DW_ATE_unsigned")),
            "u16" => Some((2, "DW_ATE_unsigned")),
            "u32" => Some((4, "DW_ATE_unsigned")),
            "u64" => Some((8, "DW_ATE_unsigned")),
            _ => None,
        };
        if let Some((size, encoding)) = expected {
            ensure!(
                bytes == size && attr(d, "DW_AT_encoding")? == format!("({encoding})"),
                "native_debug_base_type_representation_mismatched"
            );
        }
    } else if d.tag == "DW_TAG_structure_type" {
        ensure!(
            number(attr(d, "DW_AT_byte_size")?)? > 0,
            "native_debug_empty_structure_type"
        );
    }
    match_type(c, &actual)
}
fn archive_member(raw: &[u8], wanted: &str) -> Result<(u64, Vec<u8>)> {
    ensure!(
        raw.starts_with(b"!<arch>\n"),
        "native_debug_archive_format_unsupported"
    );
    let mut pos = 8usize;
    let mut ordinal = 0u64;
    let mut strings = Vec::new();
    let mut found = Vec::new();
    while pos < raw.len() {
        let header = raw
            .get(pos..pos + 60)
            .context("native_debug_archive_header")?;
        ensure!(&header[58..60] == b"`\n", "native_debug_archive_header");
        let bytes = std::str::from_utf8(&header[48..58])?
            .trim()
            .parse::<usize>()?;
        let data = raw
            .get(pos + 60..pos + 60 + bytes)
            .context("native_debug_archive_payload")?;
        let encoded = std::str::from_utf8(&header[..16])?.trim();
        let (name, payload) = if let Some(length) = encoded.strip_prefix("#1/") {
            let length = length.parse::<usize>()?;
            let name =
                std::str::from_utf8(data.get(..length).context("native_debug_archive_name")?)?
                    .trim_end_matches('\0')
                    .to_owned();
            (
                name,
                data.get(length..).context("native_debug_archive_payload")?,
            )
        } else if encoded == "//" {
            strings = data.to_vec();
            (encoded.into(), data)
        } else if let Some(offset) = encoded
            .strip_prefix('/')
            .filter(|s| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()))
        {
            let offset = offset.parse::<usize>()?;
            let name = strings.get(offset..).context("native_debug_archive_name")?;
            let end = name
                .windows(2)
                .position(|w| w == b"/\n")
                .context("native_debug_archive_name")?;
            (std::str::from_utf8(&name[..end])?.into(), data)
        } else {
            (encoded.trim_end_matches('/').into(), data)
        };
        if name == wanted {
            found.push((ordinal, payload.to_vec()));
        }
        pos = pos
            .checked_add(60 + bytes + bytes % 2)
            .context("native_debug_archive_overflow")?;
        ordinal += 1;
    }
    ensure!(
        pos == raw.len() && found.len() == 1,
        "native_debug_archive_member_missing_or_ambiguous"
    );
    Ok(found.remove(0))
}
fn map_entries(
    names: &[String],
    invocation: &CodeNativeInvocation,
    recipe: &CodeNativeBuildRecipe,
) -> Result<Vec<CodeNativeDebugMapEntry>> {
    let mut out = Vec::new();
    for (ordinal, oso_name) in names.iter().enumerate() {
        if oso_name.ends_with(')') {
            let (container, member) = oso_name
                .rsplit_once('(')
                .context("native_debug_archive_oso")?;
            let member = &member[..member.len() - 1];
            let object = recipe
                .external_archive_bindings
                .iter()
                .find(|p| p.path == container)
                .context("native_debug_unbound_archive_oso")?
                .clone();
            crate::native_profile::verify_file(&object)?;
            let bytes = read_regular_bytes(Path::new(container), 512 * 1024 * 1024)?;
            let (index, payload) = archive_member(&bytes, member)?;
            out.push(CodeNativeDebugMapEntry {
                ordinal: ordinal as u64,
                oso_name: oso_name.clone(),
                object,
                archive_member: Some(member.into()),
                archive_member_ordinal: Some(index),
                payload_sha256: sha256(payload),
            });
        } else {
            ensure!(
                !Path::new(oso_name).is_absolute(),
                "native_debug_own_oso_not_logical"
            );
            let mapping = invocation
                .objects
                .iter()
                .find(|m| m.kind == "own_object" && m.logical_name == *oso_name)
                .context("native_debug_missing_own_oso")?;
            let object = mapping
                .staged
                .clone()
                .context("native_debug_missing_staged_oso")?;
            crate::native_profile::verify_file(&object)?;
            ensure!(
                Path::new(&object.path) == Path::new(&invocation.cwd).join(oso_name)
                    && object.sha256 == mapping.payload_sha256,
                "native_debug_wrong_side_oso"
            );
            out.push(CodeNativeDebugMapEntry {
                ordinal: ordinal as u64,
                oso_name: oso_name.clone(),
                payload_sha256: object.sha256.clone(),
                object,
                archive_member: None,
                archive_member_ordinal: None,
            });
        }
    }
    ensure!(!out.is_empty(), "native_debug_empty_map");
    Ok(out)
}
fn oso_names(raw: &[u8]) -> Result<Vec<String>> {
    let text = std::str::from_utf8(raw)?;
    let mut names = Vec::new();
    for line in text.lines() {
        if let Some((prefix, name)) = line.split_once(" OSO ") {
            let fields = prefix.split_whitespace().collect::<Vec<_>>();
            ensure!(
                fields.len() == 4 && fields[1] == "-" && !name.trim().is_empty(),
                "native_debug_oso_format"
            );
            hex(fields[0])?;
            names.push(name.into());
        }
    }
    ensure!(!names.is_empty(), "native_debug_missing_oso");
    Ok(names)
}
fn verify_loads(
    map: &[CodeNativeDebugMapEntry],
    stdout: &[u8],
    stderr: &[u8],
    verified: &[u8],
) -> Result<()> {
    let text = format!(
        "{}\n{}",
        std::str::from_utf8(stdout)?,
        std::str::from_utf8(stderr)?
    );
    let lower = text.to_ascii_lowercase();
    ensure!(
        !lower.contains("warning:")
            && !lower.contains("error:")
            && !lower.contains("no debug symbols"),
        "native_debug_missing_or_invalid_objects"
    );
    for m in map {
        let marker = format!("note: trying to open '{}'", m.oso_name);
        let start = text
            .find(&marker)
            .context("native_debug_object_not_observed")?;
        let remaining = &text[start + marker.len()..];
        let block = remaining
            .split("note: trying to open '")
            .next()
            .unwrap_or(remaining);
        ensure!(
            block.contains(if m.archive_member.is_some() {
                "note: found member in archive."
            } else {
                "note: loaded object."
            }),
            "native_debug_object_not_loaded"
        );
    }
    let verify = std::str::from_utf8(verified)?;
    ensure!(
        verify.contains("No errors.")
            && verify.contains("Verifying unit:")
            && !verify.to_ascii_lowercase().contains("error:"),
        "native_debug_empty_or_invalid_dwarf"
    );
    Ok(())
}
fn runner_invocation<'a>(
    a: &CodeSourceArtifact,
    invocations: &'a [CodeNativeInvocation],
) -> Result<&'a CodeNativeInvocation> {
    let deps = Path::new(&a.runner.path)
        .parent()
        .context("native_debug_runner_parent")?
        .join("deps");
    let mut matches = Vec::new();
    for v in invocations {
        if v.parent_stage != "build-runner" {
            continue;
        }
        for w in v.received_arguments.windows(2) {
            if w[0] != "-o" {
                continue;
            }
            let path = Path::new(&w[1]);
            if w[1] != a.runner.path && path.parent() != Some(deps.as_path()) {
                continue;
            }
            if !path.exists() {
                continue;
            }
            let current = file_binding(path)?;
            if current.sha256 == a.runner.sha256 && current.bytes == a.runner.bytes {
                matches.push(v);
            }
        }
    }
    ensure!(
        matches.len() == 1,
        "native_debug_runner_invocation_ambiguous"
    );
    Ok(matches[0])
}

pub(crate) struct DebugView<'a> {
    pub root: &'a SkillRoot,
    pub tools: &'a [CodeNativeToolPin],
    pub check_pins: &'a dyn Fn() -> Result<()>,
    pub map: &'a dyn Fn(&[String]) -> Result<Vec<CodeNativeDebugMapEntry>>,
    pub cwd: &'a Path,
    pub context: ArtifactReference,
    pub source_profile: &'a ArtifactReference,
    pub side_ref: &'a ArtifactReference,
    pub expectations: &'a ArtifactReference,
    pub binary: CodeNativeFileBinding,
    pub deadline: std::time::Instant,
    pub deadline_epoch_ms: u64,
    pub output_limit: usize,
    pub budget: &'a CodeBudget,
    pub image_role: Option<&'a str>,
}
impl DebugView<'_> {
    fn role(&self, role: &str) -> Result<&CodeNativeFileBinding> {
        self.tools
            .iter()
            .find(|t| t.role == role)
            .map(|t| &t.file)
            .context("native_debug_tool_role")
    }
    fn proof_schema(&self) -> &str {
        if self.image_role.is_some() {
            "oev2.native-debug-proof/3"
        } else {
            "oev2.native-debug-proof/2"
        }
    }
}
struct DebugResult {
    map: Vec<CodeNativeDebugMapEntry>,
    checks: Vec<CodeNativeDebugCheckObservation>,
}
fn observer(
    view: &DebugView<'_>,
    tool_role: &str,
    args: Vec<String>,
    cwd: &Path,
    path: &Path,
) -> Result<ArtifactReference> {
    (view.check_pins)()?;
    let mut spec = CommandSpec {
        executable: view.role(tool_role)?.path.clone().into(),
        arguments: args.iter().map(Into::into).collect(),
        cwd: cwd.into(),
        environment: BTreeMap::from([
            ("PATH".into(), "/usr/bin:/bin".into()),
            ("LC_ALL".into(), "C".into()),
        ]),
        remove_environment: std::env::vars_os()
            .map(|(k, _)| k)
            .filter(|k| k != "PATH" && k != "LC_ALL")
            .collect(),
        timeout: deadline_timeout(view.deadline)?,
        output_limit: view.output_limit,
    };
    crate::native_runtime::freeze_command(view.root, &mut spec, path, view.deadline_epoch_ms)?;
    let result = run_command(&spec, path)?;
    (view.check_pins)()?;
    ensure!(
        result.status == "completed" && result.exit_code == Some(0),
        "native_debug_observer_failed"
    );
    reference(view.root, &path.join("process.json"))
}
fn output(
    view: &DebugView<'_>,
    r: &ArtifactReference,
    tool_role: &str,
    cwd: &Path,
    args: &[String],
) -> Result<Vec<u8>> {
    let context = read(view.root, &view.context)?;
    ensure!(
        context["side_context"] == serde_json::to_value(view.side_ref)?
            && context["deadline_epoch_ms"] == view.deadline_epoch_ms
            && context["output_bytes"] == view.budget.output_bytes,
        "native_debug_context_changed"
    );
    crate::native_runtime::verify_frozen_command(
        view.root,
        r,
        view.budget.output_bytes,
        view.budget.timeout_ms,
        Some(view.deadline_epoch_ms),
    )?;
    Ok(crate::native_runtime::verify_process(view.root, r, view.role(tool_role)?, cwd, args)?.0)
}
fn before_observers(view: &DebugView<'_>, artifact: &ArtifactReference) -> Value {
    let mut value = json!({"schema":"oev2.native-debug-before-observers/1","artifact":artifact,"side_context":view.side_ref,
        "expectations":view.expectations,"deadline_epoch_ms":view.deadline_epoch_ms,"output_bytes":view.output_limit});
    if let Some(role) = view.image_role {
        value["schema"] = json!("oev2.native-debug-before-observers/3");
        value["image_role"] = json!(role);
        value["binary"] = json!(view.binary);
    }
    value
}
fn verify_signature(
    view: &DebugView<'_>,
    r: &ArtifactReference,
    cwd: &Path,
    args: &[String],
) -> Result<()> {
    output(view, r, "signature_verifier", cwd, args)?;
    let (_, stderr) = crate::native_runtime::verify_process(
        view.root,
        r,
        view.role("signature_verifier")?,
        cwd,
        args,
    )?;
    let text = std::str::from_utf8(&stderr)?;
    ensure!(
        text.contains("valid on disk") && text.contains("satisfies its Designated Requirement"),
        "native_signature_positive_observations_missing"
    );
    Ok(())
}

fn dwarf_args(selector: &str, dwarf: &Path) -> Vec<String> {
    vec![
        selector.into(),
        "--show-parents".into(),
        "--show-children".into(),
        dwarf.to_string_lossy().into_owned(),
    ]
}

pub(crate) fn observe(
    session: &NativeBuildSession,
    artifact_ref: &ArtifactReference,
    a: &CodeSourceArtifact,
    invocations: &[CodeNativeInvocation],
    folder: &Path,
) -> Result<CodeNativeDebugObservation> {
    let invocation = runner_invocation(a, invocations)?;
    let cwd = Path::new(&invocation.cwd);
    let pins = || verify_pins(&session.recipe);
    let mapping = |names: &[String]| map_entries(names, invocation, &session.recipe);
    let context = reference(
        &session.root,
        &cwd.parent()
            .context("native_debug_stage")?
            .join("stage-context.json"),
    )?;
    let view = DebugView {
        root: &session.root,
        tools: &session.recipe.tools,
        check_pins: &pins,
        map: &mapping,
        cwd,
        context,
        source_profile: &session.side.source_profile,
        side_ref: &session.side_ref,
        expectations: &session.profile.debug_expectations,
        binary: file_binding(Path::new(&a.runner.path))?,
        deadline: session.deadline,
        deadline_epoch_ms: session.deadline_epoch_ms,
        output_limit: session.output_limit,
        budget: &session.side.body_budget,
        image_role: None,
    };
    let result = observe_core(&view, artifact_ref, folder)?;
    crate::verify_binary(&a.runner)?;
    Ok(CodeNativeDebugObservation{schema:"oev2.native-debug-observation/2".into(),expectations:session.profile.debug_expectations.clone(),
        artifact:artifact_ref.clone(),map:result.map,checks:result.checks,
        uncovered:vec!["Finite DWARF predicate/required N_OSO scope only; full SDK, all runtime behavior and other platforms remain unknown".into()]})
}
fn observe_core(
    view: &DebugView<'_>,
    artifact_ref: &ArtifactReference,
    folder: &Path,
) -> Result<DebugResult> {
    create_new_private_dir(folder)?;
    let root = view.root;
    let cwd = view.cwd;
    let dwarf = folder.join("observer-flat.dwarf");
    write_new(
        &folder.join("expected-before-observers.json"),
        &before_observers(view, artifact_ref),
    )?;
    let expected: CodeNativeDebugExpectations =
        serde_json::from_value(read(root, &*view.expectations)?)?;
    expected
        .validate()
        .map_err(|e| anyhow::anyhow!("native_debug_expectations:{e:?}"))?;
    let signature_args = vec![
        "--verify".into(),
        "--strict".into(),
        "--verbose=4".into(),
        view.binary.path.clone(),
    ];
    let sig = observer(
        view,
        "signature_verifier",
        signature_args.clone(),
        cwd,
        &folder.join("signature"),
    )?;
    let symbols_args = vec!["-a".into(), "-p".into(), view.binary.path.clone()];
    let symbols = observer(
        view,
        "debug_mapper",
        symbols_args.clone(),
        cwd,
        &folder.join("symbols"),
    )?;
    let map = (view.map)(&oso_names(&output(
        view,
        &symbols,
        "debug_mapper",
        cwd,
        &symbols_args,
    )?)?)?;
    verify_signature(view, &sig, cwd, &signature_args)?;
    let dsym_args = vec![
        "--arch".into(),
        "arm64".into(),
        "--num-threads".into(),
        "1".into(),
        "--flat".into(),
        "--out".into(),
        dwarf.to_string_lossy().into_owned(),
        "--verify-dwarf=all".into(),
        "--verbose".into(),
        view.binary.path.clone(),
    ];
    let dsym = observer(
        view,
        "debug_linker",
        dsym_args.clone(),
        cwd,
        &folder.join("dwarf-link"),
    )?;
    let verify_args = vec![
        "--arch=arm64".into(),
        "--verify".into(),
        "--error-display=full".into(),
        dwarf.to_string_lossy().into_owned(),
    ];
    let verify = observer(
        view,
        "dwarf_reader",
        verify_args.clone(),
        cwd,
        &folder.join("dwarf-verify"),
    )?;
    let (dsout, dserr) = crate::native_runtime::verify_process(
        root,
        &dsym,
        view.role("debug_linker")?,
        cwd,
        &dsym_args,
    )?;
    output(view, &dsym, "debug_linker", cwd, &dsym_args)?;
    verify_loads(
        &map,
        &dsout,
        &dserr,
        &output(view, &verify, "dwarf_reader", cwd, &verify_args)?,
    )?;
    let mut checks = Vec::new();
    let mut proofs = Vec::new();
    let mut leaves = Vec::new();
    for m in &map {
        let payload = if let Some(member) = &m.archive_member {
            archive_member(
                &read_regular_bytes(Path::new(&m.object.path), 512 * 1024 * 1024)?,
                member,
            )?
            .1
        } else {
            read_regular_bytes(Path::new(&m.object.path), 512 * 1024 * 1024)?
        };
        ensure!(
            sha256(&payload) == m.payload_sha256,
            "native_debug_leaf_payload_changed"
        );
        let leaf = folder.join(format!("object-{:06}.o", m.ordinal));
        write_new_bytes(&leaf, &payload)?;
        let args = vec![
            "--debug-info".into(),
            "--recurse-depth=0".into(),
            leaf.to_string_lossy().into_owned(),
        ];
        let process = observer(
            view,
            "dwarf_reader",
            args.clone(),
            cwd,
            &folder.join(format!("leaf-{:06}", m.ordinal)),
        )?;
        let text = String::from_utf8(output(view, &process, "dwarf_reader", cwd, &args)?)?;
        ensure!(
            dies(&text)?.iter().any(|d| d.tag == "DW_TAG_compile_unit"
                && d.attrs.contains_key("DW_AT_name")
                && d.attrs.contains_key("DW_AT_producer")
                && d.attrs.contains_key("DW_AT_stmt_list")),
            "native_debug_leaf_no_full_cu"
        );
        leaves.push(json!({"ordinal":m.ordinal,"leaf":file_binding(&leaf)?,"process":process}));
    }
    for (i, c) in expected.checks.iter().enumerate() {
        let dir = folder.join(format!("check-{i:06}"));
        create_new_private_dir(&dir)?;
        let basename = c
            .function
            .rsplit("::")
            .next()
            .context("native_debug_function_name")?;
        let function_args = dwarf_args(&format!("--name={basename}"), &dwarf);
        let process = observer(
            view,
            "dwarf_reader",
            function_args.clone(),
            cwd,
            &dir.join("function"),
        )?;
        let text = String::from_utf8(output(view, &process, "dwarf_reader", cwd, &function_args)?)?;
        let (all, function) = select_function(root, c, &text)?;
        let mut types = Vec::new();
        let mut line = None;
        match c.kind.as_str() {
            "function" => {
                ensure!(
                    name(&all[function])? == c.name || qualified(&all, function)? == c.name,
                    "native_debug_function_name_mismatched"
                );
                if let Some(wanted) = c.line {
                    ensure!(
                        number(attr(&all[function], "DW_AT_decl_line")?)? == wanted,
                        "native_debug_function_line_mismatched"
                    );
                }
            }
            "source_line" => {
                let cu = ancestor(&all, function, "DW_TAG_compile_unit")
                    .context("native_debug_cu_missing")?;
                let offset = number(attr(&all[cu], "DW_AT_stmt_list")?)?;
                let args = vec![
                    format!("--debug-line=0x{offset:x}"),
                    dwarf.to_string_lossy().into_owned(),
                ];
                let p = observer(view, "dwarf_reader", args.clone(), cwd, &dir.join("line"))?;
                let raw = String::from_utf8(output(view, &p, "dwarf_reader", cwd, &args)?)?;
                line_matches(c, &all, function, &raw, &source_file(root, c)?)?;
                line = Some(p);
            }
            "parameter" | "local" => {
                let index = select_variable(&all, function, c)?;
                let (offset, label) = type_ref(&all[index])?;
                let args = dwarf_args(&format!("--debug-info=0x{offset:x}"), &dwarf);
                let p = observer(view, "dwarf_reader", args.clone(), cwd, &dir.join("type"))?;
                let raw = String::from_utf8(output(view, &p, "dwarf_reader", cwd, &args)?)?;
                leaf_type_matches(c, offset, &label, &raw)?;
                types.push(json!({"offset":offset,"process":p}));
            }
            "type_member" => {
                let (structure, member) = c
                    .name
                    .rsplit_once('.')
                    .context("native_debug_member_identity_unsupported")?;
                let basename = structure
                    .rsplit("::")
                    .next()
                    .context("native_debug_type_name")?;
                let args = dwarf_args(&format!("--name={basename}"), &dwarf);
                let p = observer(
                    view,
                    "dwarf_reader",
                    args.clone(),
                    cwd,
                    &dir.join("structure"),
                )?;
                let raw = String::from_utf8(output(view, &p, "dwarf_reader", cwd, &args)?)?;
                let (offset, label) = verify_member(c, &all, function, structure, member, &raw)?;
                let type_args = dwarf_args(&format!("--debug-info=0x{offset:x}"), &dwarf);
                let t = observer(
                    view,
                    "dwarf_reader",
                    type_args.clone(),
                    cwd,
                    &dir.join("member-type"),
                )?;
                leaf_type_matches(
                    c,
                    offset,
                    &label,
                    &String::from_utf8(output(view, &t, "dwarf_reader", cwd, &type_args)?)?,
                )?;
                types.push(json!({"structure":structure,"process":p}));
                types.push(json!({"offset":offset,"process":t}));
            }
            _ => anyhow::bail!("native_debug_check_kind_unknown"),
        }
        checks.push(CodeNativeDebugCheckObservation {
            check_id: c.check_id.clone(),
            status: "matched".into(),
            process: process.clone(),
            stdout: reference(root, &dir.join("function/stdout"))?,
            stderr: reference(root, &dir.join("function/stderr"))?,
        });
        proofs.push(json!({"check_id":c.check_id,"function":process,"types":types,"line":line}));
    }
    crate::native_profile::verify_file(&view.binary)?;
    let mut proof = json!({"schema":view.proof_schema(),"artifact":artifact_ref,"source_profile":view.source_profile,"side_context":view.side_ref,
        "dwarf":file_binding(&dwarf)?,"signature":sig,"symbols":symbols,"dwarf_link":dsym,"dwarf_verify":verify,"leaves":leaves,"checks":proofs});
    if let Some(role) = view.image_role {
        proof["image_role"] = json!(role);
        proof["binary"] = json!(view.binary);
    }
    write_new(&folder.join("proof.json"), &proof)?;
    Ok(DebugResult { map, checks })
}

fn verify_member(
    c: &CodeNativeDebugExpectation,
    function_dies: &[Die],
    function: usize,
    structure: &str,
    member: &str,
    text: &str,
) -> Result<(u64, String)> {
    let all = dies(text)?;
    let mut hits = Vec::new();
    for (i, d) in all.iter().enumerate() {
        if d.tag == "DW_TAG_structure_type"
            && (qualified(&all, i).ok().as_deref() == Some(structure)
                || name(d).ok().as_deref() == Some(structure))
        {
            hits.push(i);
        }
    }
    ensure!(hits.len() == 1, "native_debug_member_structure_ambiguous");
    let s = hits[0];
    let offset = all[s].offset;
    ensure!(
        function_dies.iter().enumerate().any(|(i, d)| ancestor(
            function_dies,
            i,
            "DW_TAG_subprogram"
        ) == Some(function)
            && ["DW_TAG_formal_parameter", "DW_TAG_variable"].contains(&d.tag.as_str())
            && d.attrs
                .get("DW_AT_location")
                .is_some_and(|l| l.starts_with("(DW_OP_"))
            && d.attrs.get("DW_AT_decl_file")
                == function_dies[function].attrs.get("DW_AT_decl_file")
            && type_ref(d).is_ok_and(|t| t.0 == offset)),
        "native_debug_member_type_not_bound_to_function"
    );
    ensure!(
        number(attr(&all[s], "DW_AT_byte_size")?)? > 0,
        "native_debug_member_empty_type"
    );
    let members = all
        .iter()
        .enumerate()
        .filter(|(_, d)| {
            d.tag == "DW_TAG_member"
                && d.parent == Some(s)
                && name(d).ok().as_deref() == Some(member)
        })
        .collect::<Vec<_>>();
    ensure!(
        members.len() == 1,
        "native_debug_member_missing_or_ambiguous"
    );
    let (_, m) = members[0];
    ensure!(
        number(attr(m, "DW_AT_data_member_location")?)?
            < number(attr(&all[s], "DW_AT_byte_size")?)?,
        "native_debug_member_location_invalid"
    );
    let (offset, label) = type_ref(m)?;
    match_type(c, &label)?;
    Ok((offset, label))
}

pub(crate) fn verify_observation(
    root: &SkillRoot,
    side: &CodeNativeSideContext,
    recipe: &CodeNativeBuildRecipe,
    artifact_ref: &ArtifactReference,
    a: &CodeSourceArtifact,
    invocations: &[CodeNativeInvocation],
    observation_ref: &ArtifactReference,
) -> Result<()> {
    let folder = Path::new(&side.build.evidence_path).join("native-debug");
    ensure!(
        observation_ref.path
            == Path::new(&side.build.evidence_path)
                .join("native-debug-observation.json")
                .to_str()
                .context("native_path_utf8")?,
        "native_debug_observation_wrong_side"
    );
    let observation: CodeNativeDebugObservation =
        serde_json::from_value(read(root, observation_ref)?)?;
    observation
        .validate()
        .map_err(|e| anyhow::anyhow!("native_debug_observation:{e:?}"))?;
    let profile = crate::native_profile::inspect_profile(root, &side.source_profile)?;
    let expected: CodeNativeDebugExpectations =
        serde_json::from_value(read(root, &profile.debug_expectations)?)?;
    ensure!(
        observation.schema == "oev2.native-debug-observation/2"
            && observation.artifact == *artifact_ref
            && observation.expectations == profile.debug_expectations
            && expected.all_required_oso
            && expected.signature_required
            && expected.checks.len() == observation.checks.len(),
        "native_debug_binding_changed"
    );
    let invocation = runner_invocation(a, invocations)?;
    let cwd = Path::new(&invocation.cwd);
    let side_ref: ArtifactReference =
        serde_json::from_value(a.generation["native_build"]["side_context"].clone())?;
    let pins = || verify_pins(recipe);
    let mapping = |names: &[String]| map_entries(names, invocation, recipe);
    let context = reference(
        root,
        &cwd.parent()
            .context("native_debug_stage")?
            .join("stage-context.json"),
    )?;
    let end = read(root, &context)?["deadline_epoch_ms"]
        .as_u64()
        .context("native_debug_deadline")?;
    let view = DebugView {
        root,
        tools: &recipe.tools,
        check_pins: &pins,
        map: &mapping,
        cwd,
        context,
        source_profile: &side.source_profile,
        side_ref: &side_ref,
        expectations: &profile.debug_expectations,
        binary: file_binding(Path::new(&a.runner.path))?,
        deadline: std::time::Instant::now(),
        deadline_epoch_ms: end,
        output_limit: usize::try_from(side.body_budget.output_bytes)?,
        budget: &side.body_budget,
        image_role: None,
    };
    verify_core(
        &view,
        artifact_ref,
        &observation.map,
        &observation.checks,
        &folder,
    )?;
    crate::verify_binary(&a.runner)?;
    Ok(())
}
fn verify_core(
    view: &DebugView<'_>,
    artifact_ref: &ArtifactReference,
    observed_map: &[CodeNativeDebugMapEntry],
    observed_checks: &[CodeNativeDebugCheckObservation],
    folder: &Path,
) -> Result<()> {
    let root = view.root;
    let expected: CodeNativeDebugExpectations =
        serde_json::from_value(read(root, view.expectations)?)?;
    expected
        .validate()
        .map_err(|e| anyhow::anyhow!("native_debug_expectations:{e:?}"))?;
    ensure!(
        expected.all_required_oso
            && expected.signature_required
            && expected.checks.len() == observed_checks.len(),
        "native_debug_required_conditions"
    );
    let proof = read(root, &reference(root, &folder.join("proof.json"))?)?;
    ensure!(
        proof["schema"] == view.proof_schema()
            && proof["artifact"] == serde_json::to_value(artifact_ref)?
            && proof["source_profile"] == serde_json::to_value(view.source_profile)?
            && proof["side_context"] == serde_json::to_value(view.side_ref)?,
        "native_debug_proof_changed"
    );
    let cwd = view.cwd;
    ensure!(
        read(
            root,
            &reference(root, &folder.join("expected-before-observers.json"))?
        )? == before_observers(view, artifact_ref),
        "native_debug_frozen_expectations_changed"
    );
    if let Some(role) = view.image_role {
        ensure!(
            proof["image_role"] == role && proof["binary"] == serde_json::to_value(&view.binary)?,
            "native_debug_image_role_or_binary_changed"
        );
    }
    let dwarf: CodeNativeFileBinding = serde_json::from_value(proof["dwarf"].clone())?;
    ensure!(
        Path::new(&dwarf.path) == folder.join("observer-flat.dwarf"),
        "native_debug_dwarf_wrong_side"
    );
    crate::native_profile::verify_file(&dwarf)?;
    let get = |key: &str, path: &Path| -> Result<ArtifactReference> {
        let r: ArtifactReference = serde_json::from_value(proof[key].clone())?;
        ensure!(
            r == reference(root, path)?,
            "native_debug_process_wrong_side"
        );
        Ok(r)
    };
    let signature_args = vec![
        "--verify".into(),
        "--strict".into(),
        "--verbose=4".into(),
        view.binary.path.clone(),
    ];
    let sig = get("signature", &folder.join("signature/process.json"))?;
    let _ = output(view, &sig, "signature_verifier", cwd, &signature_args)?;
    let symbols_args = vec!["-a".into(), "-p".into(), view.binary.path.clone()];
    let symbols = get("symbols", &folder.join("symbols/process.json"))?;
    let map = (view.map)(&oso_names(&output(
        view,
        &symbols,
        "debug_mapper",
        cwd,
        &symbols_args,
    )?)?)?;
    verify_signature(view, &sig, cwd, &signature_args)?;
    ensure!(map == *observed_map, "native_debug_map_changed");
    let dsym_args = vec![
        "--arch".into(),
        "arm64".into(),
        "--num-threads".into(),
        "1".into(),
        "--flat".into(),
        "--out".into(),
        dwarf.path.clone(),
        "--verify-dwarf=all".into(),
        "--verbose".into(),
        view.binary.path.clone(),
    ];
    let dsym = get("dwarf_link", &folder.join("dwarf-link/process.json"))?;
    let verify_args = vec![
        "--arch=arm64".into(),
        "--verify".into(),
        "--error-display=full".into(),
        dwarf.path.clone(),
    ];
    let verify = get("dwarf_verify", &folder.join("dwarf-verify/process.json"))?;
    let (stdout, stderr) = crate::native_runtime::verify_process(
        root,
        &dsym,
        view.role("debug_linker")?,
        cwd,
        &dsym_args,
    )?;
    output(view, &dsym, "debug_linker", cwd, &dsym_args)?;
    verify_loads(
        &map,
        &stdout,
        &stderr,
        &output(view, &verify, "dwarf_reader", cwd, &verify_args)?,
    )?;
    let leaves = proof["leaves"]
        .as_array()
        .context("native_debug_leaves_missing")?;
    ensure!(
        leaves.len() == map.len(),
        "native_debug_leaf_inventory_changed"
    );
    for (m, p) in map.iter().zip(leaves) {
        ensure!(p["ordinal"] == m.ordinal, "native_debug_leaf_order_changed");
        let leaf: CodeNativeFileBinding = serde_json::from_value(p["leaf"].clone())?;
        ensure!(
            leaf.path
                == folder
                    .join(format!("object-{:06}.o", m.ordinal))
                    .to_str()
                    .context("native_path_utf8")?
                && leaf.sha256 == m.payload_sha256,
            "native_debug_leaf_changed"
        );
        crate::native_profile::verify_file(&leaf)?;
        let r: ArtifactReference = serde_json::from_value(p["process"].clone())?;
        ensure!(
            r == reference(
                root,
                &folder.join(format!("leaf-{:06}/process.json", m.ordinal))
            )?,
            "native_debug_leaf_process_changed"
        );
        let args = vec![
            "--debug-info".into(),
            "--recurse-depth=0".into(),
            leaf.path.clone(),
        ];
        let text = String::from_utf8(output(view, &r, "dwarf_reader", cwd, &args)?)?;
        ensure!(
            dies(&text)?.iter().any(|d| d.tag == "DW_TAG_compile_unit"
                && d.attrs.contains_key("DW_AT_name")
                && d.attrs.contains_key("DW_AT_producer")
                && d.attrs.contains_key("DW_AT_stmt_list")),
            "native_debug_leaf_no_full_cu"
        );
    }
    let proofs = proof["checks"]
        .as_array()
        .context("native_debug_check_proofs_missing")?;
    ensure!(
        proofs.len() == expected.checks.len(),
        "native_debug_check_inventory_changed"
    );
    for (i, ((c, check), p)) in expected
        .checks
        .iter()
        .zip(observed_checks)
        .zip(proofs)
        .enumerate()
    {
        let dir = folder.join(format!("check-{i:06}"));
        ensure!(
            check.check_id == c.check_id
                && p["check_id"] == c.check_id
                && check.status == "matched"
                && check.process == reference(root, &dir.join("function/process.json"))?
                && check.stdout == reference(root, &dir.join("function/stdout"))?
                && check.stderr == reference(root, &dir.join("function/stderr"))?
                && p["function"] == serde_json::to_value(&check.process)?,
            "native_debug_check_binding_changed"
        );
        let args = dwarf_args(
            &format!(
                "--name={}",
                c.function
                    .rsplit("::")
                    .next()
                    .context("native_debug_function")?
            ),
            Path::new(&dwarf.path),
        );
        let text = String::from_utf8(output(view, &check.process, "dwarf_reader", cwd, &args)?)?;
        let (all, function) = select_function(root, c, &text)?;
        let types = p["types"].as_array().context("native_debug_type_proofs")?;
        match c.kind.as_str() {
            "function" => {
                ensure!(
                    types.is_empty()
                        && p["line"].is_null()
                        && (name(&all[function])? == c.name
                            || qualified(&all, function)? == c.name),
                    "native_debug_function_mismatched"
                );
                if let Some(line) = c.line {
                    ensure!(
                        number(attr(&all[function], "DW_AT_decl_line")?)? == line,
                        "native_debug_function_line_mismatched"
                    );
                }
            }
            "source_line" => {
                ensure!(types.is_empty(), "native_debug_unexpected_type_proof");
                let cu =
                    ancestor(&all, function, "DW_TAG_compile_unit").context("native_debug_cu")?;
                let offset = number(attr(&all[cu], "DW_AT_stmt_list")?)?;
                let r: ArtifactReference = serde_json::from_value(p["line"].clone())?;
                ensure!(
                    r == reference(root, &dir.join("line/process.json"))?,
                    "native_debug_line_process_changed"
                );
                let args = vec![format!("--debug-line=0x{offset:x}"), dwarf.path.clone()];
                let text = String::from_utf8(output(view, &r, "dwarf_reader", cwd, &args)?)?;
                line_matches(c, &all, function, &text, &source_file(root, c)?)?;
            }
            "parameter" | "local" => {
                ensure!(
                    types.len() == 1 && p["line"].is_null(),
                    "native_debug_type_inventory_changed"
                );
                let index = select_variable(&all, function, c)?;
                let (offset, label) = type_ref(&all[index])?;
                ensure!(
                    types[0]["offset"] == offset,
                    "native_debug_type_offset_changed"
                );
                let r: ArtifactReference = serde_json::from_value(types[0]["process"].clone())?;
                ensure!(
                    r == reference(root, &dir.join("type/process.json"))?,
                    "native_debug_type_process_changed"
                );
                let args = dwarf_args(
                    &format!("--debug-info=0x{offset:x}"),
                    Path::new(&dwarf.path),
                );
                leaf_type_matches(
                    c,
                    offset,
                    &label,
                    &String::from_utf8(output(view, &r, "dwarf_reader", cwd, &args)?)?,
                )?;
            }
            "type_member" => {
                ensure!(
                    types.len() == 2 && p["line"].is_null(),
                    "native_debug_type_inventory_changed"
                );
                let (structure, member) = c
                    .name
                    .rsplit_once('.')
                    .context("native_debug_member_identity")?;
                ensure!(
                    types[0]["structure"] == structure,
                    "native_debug_member_structure_changed"
                );
                let r: ArtifactReference = serde_json::from_value(types[0]["process"].clone())?;
                ensure!(
                    r == reference(root, &dir.join("structure/process.json"))?,
                    "native_debug_member_process_changed"
                );
                let args = dwarf_args(
                    &format!(
                        "--name={}",
                        structure
                            .rsplit("::")
                            .next()
                            .context("native_debug_type_name")?
                    ),
                    Path::new(&dwarf.path),
                );
                let (offset, label) = verify_member(
                    c,
                    &all,
                    function,
                    structure,
                    member,
                    &String::from_utf8(output(view, &r, "dwarf_reader", cwd, &args)?)?,
                )?;
                ensure!(
                    types[1]["offset"] == offset,
                    "native_debug_member_type_offset_changed"
                );
                let t: ArtifactReference = serde_json::from_value(types[1]["process"].clone())?;
                ensure!(
                    t == reference(root, &dir.join("member-type/process.json"))?,
                    "native_debug_member_type_process_changed"
                );
                let args = dwarf_args(
                    &format!("--debug-info=0x{offset:x}"),
                    Path::new(&dwarf.path),
                );
                leaf_type_matches(
                    c,
                    offset,
                    &label,
                    &String::from_utf8(output(view, &t, "dwarf_reader", cwd, &args)?)?,
                )?;
            }
            _ => anyhow::bail!("native_debug_kind_unknown"),
        }
    }
    crate::native_profile::verify_file(&view.binary)?;
    (view.check_pins)()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const FUNCTION: &str = r#"0x00000001: DW_TAG_compile_unit
                DW_AT_comp_dir ("/source")
                DW_AT_stmt_list (0x00000010)
0x00000002:   DW_TAG_namespace
                  DW_AT_name ("probe")
0x00000003:     DW_TAG_subprogram
                    DW_AT_name ("probe_sum")
                    DW_AT_decl_file ("/source/src/main.rs")
                    DW_AT_decl_line (8)
                    DW_AT_low_pc (0x00000100)
                    DW_AT_high_pc (0x00000120)
                    DW_AT_frame_base (DW_OP_reg29 W29)
0x00000004:       DW_TAG_formal_parameter
                      DW_AT_name ("point")
                      DW_AT_decl_file ("/source/src/main.rs")
                      DW_AT_decl_line (8)
                      DW_AT_location (DW_OP_breg31 WSP+4)
                      DW_AT_type (0x00000020 "probe::ProbePoint")
0x00000005:       DW_TAG_lexical_block
                      DW_AT_low_pc (0x00000110)
                      DW_AT_high_pc (0x00000118)
0x00000006:         DW_TAG_variable
                        DW_AT_name ("total")
                        DW_AT_decl_file ("/source/src/main.rs")
                        DW_AT_decl_line (9)
                        DW_AT_location (DW_OP_fbreg -4)
                        DW_AT_type (0x00000030 "i32")
"#;
    const STRUCTURE: &str = r#"0x00000001: DW_TAG_compile_unit
0x00000002:   DW_TAG_namespace
                  DW_AT_name ("probe")
0x00000020:     DW_TAG_structure_type
                    DW_AT_name ("ProbePoint")
                    DW_AT_byte_size (0x08)
0x00000021:       DW_TAG_member
                      DW_AT_name ("x")
                      DW_AT_data_member_location (0x00)
                      DW_AT_type (0x00000030 "i32")
0x00000022:       DW_TAG_member
                      DW_AT_name ("y")
                      DW_AT_data_member_location (0x04)
                      DW_AT_type (0x00000030 "i32")
"#;
    fn check(kind: &str, name: &str, line: u64, expected: &str) -> CodeNativeDebugExpectation {
        serde_json::from_value(json!({"check_id":"fixed","kind":kind,"source":{"path":"/source/src/main.rs","sha256":"0".repeat(64)},"function":"probe::probe_sum","line":line,"name":name,"expected_type":expected,"matcher":"exact"})).unwrap()
    }
    #[test]
    fn variable_requires_target_scope_source_line_and_location() {
        let all = dies(FUNCTION).unwrap();
        let c = check("local", "total", 9, "i32");
        assert_eq!(select_variable(&all, 2, &c).unwrap(), 5);
        for bad in [
            FUNCTION.replace("DW_OP_fbreg -4", ""),
            FUNCTION.replace("DW_AT_decl_line (9)", "DW_AT_decl_line (10)"),
            FUNCTION.replace("0x00000118", "0x00000130"),
            FUNCTION.replace("DW_TAG_lexical_block", "DW_TAG_inlined_subroutine"),
        ] {
            assert!(select_variable(&dies(&bad).unwrap(), 2, &c).is_err());
        }
        assert!(select_variable(&all, 1, &c).is_err());
    }
    #[test]
    fn member_requires_function_type_and_real_referenced_type() {
        let f = dies(FUNCTION).unwrap();
        let c = check("type_member", "probe::ProbePoint.x", 8, "i32");
        let (offset, label) =
            verify_member(&c, &f, 2, "probe::ProbePoint", "x", STRUCTURE).unwrap();
        assert_eq!(offset, 0x30);
        assert_eq!(label, "i32");
        let scalar = "0x00000030: DW_TAG_base_type\n                DW_AT_name (\"i32\")\n                DW_AT_byte_size (0x04)\n                DW_AT_encoding (DW_ATE_signed)\n";
        leaf_type_matches(&c, offset, &label, scalar).unwrap();
        assert!(leaf_type_matches(&c, offset + 1, &label, scalar).is_err());
        assert!(leaf_type_matches(&c, offset, &label, &scalar.replace("i32", "u32")).is_err());
        assert!(
            verify_member(
                &c,
                &f,
                2,
                "probe::ProbePoint",
                "x",
                &STRUCTURE.replace("(0x00)", "(0x08)")
            )
            .is_err()
        );
        assert!(
            verify_member(
                &c,
                &dies(&FUNCTION.replace("0x00000020", "0x00000040")).unwrap(),
                2,
                "probe::ProbePoint",
                "x",
                STRUCTURE
            )
            .is_err()
        );
    }
    #[test]
    fn line_must_resolve_cu_file_and_live_function_address() {
        let all = dies(FUNCTION).unwrap();
        let c = check("source_line", "line", 9, "i32");
        let line = "debug_line[0x00000010]\ninclude_directories[ 1] = \"src\"\nfile_names[ 1]:\nname: \"main.rs\"\ndir_index: 1\n0x00000110 9 2 1 0 0 0 is_stmt\n";
        line_matches(&c, &all, 2, line, Path::new("/source/src/main.rs")).unwrap();
        for bad in [
            line.replace("is_stmt", "end_sequence"),
            line.replace("0x00000110", "0x00000120"),
            line.replace("main.rs", "other.rs"),
            line.replace("0x00000010", "0x00000011"),
        ] {
            assert!(line_matches(&c, &all, 2, &bad, Path::new("/source/src/main.rs")).is_err());
        }
    }
    #[test]
    fn oso_loads_require_nonempty_units_and_every_object() {
        let object = CodeNativeFileBinding {
            path: "/object.o".into(),
            sha256: "0".repeat(64),
            bytes: 1,
            mode: 0o600,
        };
        let map = vec![CodeNativeDebugMapEntry {
            ordinal: 0,
            oso_name: "objects/a.o".into(),
            object,
            archive_member: None,
            archive_member_ordinal: None,
            payload_sha256: "0".repeat(64),
        }];
        let notes = b"note: trying to open 'objects/a.o'\nnote: loaded object.\n";
        let verify = b"Verifying unit: source\nNo errors.\n";
        verify_loads(&map, b"", notes, verify).unwrap();
        assert!(
            verify_loads(
                &map,
                b"",
                b"note: trying to open 'objects/b.o'\nnote: loaded object.\n",
                verify
            )
            .is_err()
        );
        assert!(verify_loads(&map, b"", notes, b"No errors.\n").is_err());
        assert!(verify_loads(&map, b"warning: missing object", notes, verify).is_err());
        let raw=b"0000000000000000 - 00 0001 OSO objects/a.o\n0000000000000000 - 00 0001 OSO objects/a.o\n";
        assert_eq!(oso_names(raw).unwrap().len(), 2);
    }
}

fn full_image_binary(
    a: &CodeSourceArtifact,
    image: &CodeNativeImageDebugExpectations3,
) -> Result<CodeNativeFileBinding> {
    match (image.image_role.as_str(), image.package.as_str()) {
        ("normalizer", "oe-design") => {
            let normalizer = a.tools.get(3).context("native3_normalizer_binary")?;
            crate::verify_binary(normalizer)?;
            file_binding(Path::new(&normalizer.path))
        }
        ("runner", "oe-code-runner") => {
            crate::verify_binary(&a.runner)?;
            file_binding(Path::new(&a.runner.path))
        }
        _ => anyhow::bail!("native3_debug_image_role_package_unsupported"),
    }
}
fn full_image_invocation<'a>(
    binary: &CodeNativeFileBinding,
    image: &CodeNativeImageDebugExpectations3,
    invocations: &'a [CodeNativeArchiveResponseInvocation],
) -> Result<&'a CodeNativeArchiveResponseInvocation> {
    let stage = if image.image_role == "normalizer" {
        "build-normalizer"
    } else if image.image_role == "runner" {
        "build-runner"
    } else {
        anyhow::bail!("native3_image_role")
    };
    let deps = Path::new(&binary.path)
        .parent()
        .context("native3_image_binary_parent")?
        .join("deps");
    let mut matches = Vec::new();
    for v in invocations.iter().filter(|v| v.parent_stage == stage) {
        for window in v
            .inputs
            .expanded_arguments
            .windows(2)
            .filter(|w| w[0] == "-o")
        {
            let path = Path::new(&window[1]);
            if path != Path::new(&binary.path) && path.parent() != Some(deps.as_path()) {
                continue;
            }
            if !path.exists() {
                continue;
            }
            let current = file_binding(path)?;
            if current.sha256 == binary.sha256 && current.bytes == binary.bytes {
                matches.push(v);
            }
        }
    }
    ensure!(
        matches.len() == 1,
        "native3_image_invocation_missing_or_ambiguous"
    );
    Ok(matches[0])
}
fn full_map_entries(
    names: &[String],
    invocation: &CodeNativeArchiveResponseInvocation,
    recipe: &CodeNativeBuildRecipe3,
) -> Result<Vec<CodeNativeDebugMapEntry>> {
    let cwd = Path::new(&invocation.native_process.cwd);
    let mut result = Vec::new();
    for (ordinal, oso) in names.iter().enumerate() {
        let entry = if oso.ends_with(')') {
            let (container, member) = oso.rsplit_once('(').context("native3_archive_oso")?;
            let member = &member[..member.len() - 1];
            let (object, expected) = if Path::new(container).is_absolute() {
                let pin = recipe
                    .external_archive_bindings
                    .iter()
                    .find(|p| p.path == container)
                    .context("native3_external_oso_unselected")?;
                (pin.clone(), None)
            } else {
                let matches = invocation
                    .inputs
                    .archives
                    .iter()
                    .filter(|a| format!("oe-native-debug/{}", a.logical_path) == container)
                    .collect::<Vec<_>>();
                ensure!(matches.len() == 1, "native3_own_archive_oso_unbound");
                let archive = matches[0];
                ensure!(
                    Path::new(&archive.staged_file.path) == cwd.join(container),
                    "native3_archive_oso_wrong_side"
                );
                let matches = archive
                    .members
                    .iter()
                    .filter(|m| m.staged_name == member)
                    .collect::<Vec<_>>();
                ensure!(matches.len() == 1, "native3_archive_oso_member_ambiguous");
                (archive.staged_file.clone(), Some(matches[0]))
            };
            crate::native_profile::verify_file(&object)?;
            let (header_ordinal, payload) = archive_member(
                &read_regular_bytes(Path::new(&object.path), 512 * 1024 * 1024)?,
                member,
            )?;
            if let Some(m) = expected {
                ensure!(
                    header_ordinal == m.staged_header_ordinal
                        && payload.len() as u64 == m.payload_bytes
                        && sha256(&payload) == m.payload_sha256,
                    "native3_archive_oso_payload_or_header_ordinal_changed"
                );
            }
            CodeNativeDebugMapEntry {
                ordinal: ordinal as u64,
                oso_name: oso.clone(),
                object,
                archive_member: Some(member.into()),
                archive_member_ordinal: Some(header_ordinal),
                payload_sha256: sha256(&payload),
            }
        } else {
            ensure!(!Path::new(oso).is_absolute(), "native3_own_oso_not_logical");
            let matches = invocation
                .inputs
                .direct_objects
                .iter()
                .filter(|m| format!("oe-native-debug/{}", m.logical_path) == *oso)
                .collect::<Vec<_>>();
            ensure!(
                matches.len() == 1,
                "native3_direct_oso_unbound_or_ambiguous"
            );
            let object = matches[0].staged_file.clone();
            crate::native_profile::verify_file(&object)?;
            ensure!(
                Path::new(&object.path) == cwd.join(oso)
                    && object.sha256 == matches[0].original_file.sha256
                    && object.bytes == matches[0].original_file.bytes,
                "native3_direct_oso_wrong_side_or_payload"
            );
            CodeNativeDebugMapEntry {
                ordinal: ordinal as u64,
                oso_name: oso.clone(),
                payload_sha256: object.sha256.clone(),
                object,
                archive_member: None,
                archive_member_ordinal: None,
            }
        };
        result.push(entry);
    }
    ensure!(!result.is_empty(), "native3_debug_map_empty");
    Ok(result)
}
pub(crate) fn observe_full(
    session: &crate::native_runtime3::NativeBuildSession3,
    artifact_ref: &ArtifactReference,
    a: &CodeSourceArtifact,
    invocations: &[CodeNativeArchiveResponseInvocation],
    image: &CodeNativeImageDebugExpectations3,
    folder: &Path,
) -> Result<CodeNativeDebugImageObservation3> {
    crate::native_profile3::inspect_debug_image(&session.root, &session.recipe, image)?;
    let binary = full_image_binary(a, image)?;
    let invocation = full_image_invocation(&binary, image, invocations)?;
    let cwd = Path::new(&invocation.native_process.cwd);
    let pins = || crate::native_runtime3::verify_pins(&session.root, &session.recipe).map(|_| ());
    let mapping = |names: &[String]| full_map_entries(names, invocation, &session.recipe);
    let context = reference(
        &session.root,
        &cwd.parent()
            .context("native3_debug_stage")?
            .join("stage-context.json"),
    )?;
    let view = DebugView {
        root: &session.root,
        tools: &session.recipe.tools,
        check_pins: &pins,
        map: &mapping,
        cwd,
        context,
        source_profile: &session.side.source_profile,
        side_ref: &session.side_ref,
        expectations: &image.expectations,
        binary: binary.clone(),
        deadline: session.deadline,
        deadline_epoch_ms: session.deadline_epoch_ms,
        output_limit: session.output_limit,
        budget: &session.side.body_budget,
        image_role: Some(&image.image_role),
    };
    let result = observe_core(&view, artifact_ref, folder)?;
    Ok(CodeNativeDebugImageObservation3{schema:"oev2.native-debug-image-observation/3".into(),image_role:image.image_role.clone(),expectations:image.expectations.clone(),
        artifact:artifact_ref.clone(),binary,map:result.map,checks:result.checks,
        uncovered:vec!["Finite frozen debug/source/type/location/required OSO and signature conditions; no full runtime behavior or engineering adoption authority".into()]})
}
pub(crate) fn verify_full(
    root: &SkillRoot,
    side_ref: &ArtifactReference,
    side: &CodeNativeSideContext3,
    recipe: &CodeNativeBuildRecipe3,
    artifact_ref: &ArtifactReference,
    a: &CodeSourceArtifact,
    invocations: &[CodeNativeArchiveResponseInvocation],
    image: &CodeNativeImageDebugExpectations3,
    observation_ref: &ArtifactReference,
) -> Result<()> {
    crate::native_profile3::inspect_debug_image(root, recipe, image)?;
    let evidence = Path::new(&side.build.evidence_path);
    let folder = evidence.join("native-debug").join(&image.image_role);
    ensure!(
        observation_ref
            == &reference(
                root,
                &evidence.join(format!(
                    "native-debug-{}-observation.json",
                    image.image_role
                ))
            )?,
        "native3_image_observation_wrong_side"
    );
    let observation: CodeNativeDebugImageObservation3 =
        serde_json::from_value(read(root, observation_ref)?)?;
    observation
        .validate()
        .map_err(|e| anyhow::anyhow!("native3_debug_observation:{e:?}"))?;
    let binary = full_image_binary(a, image)?;
    ensure!(
        observation.schema == "oev2.native-debug-image-observation/3"
            && observation.image_role == image.image_role
            && observation.expectations == image.expectations
            && observation.artifact == *artifact_ref
            && observation.binary == binary,
        "native3_debug_image_binding_changed"
    );
    let invocation = full_image_invocation(&binary, image, invocations)?;
    let cwd = Path::new(&invocation.native_process.cwd);
    let pins = || crate::native_runtime3::verify_pins(root, recipe).map(|_| ());
    let mapping = |names: &[String]| full_map_entries(names, invocation, recipe);
    let context = reference(
        root,
        &cwd.parent()
            .context("native3_debug_stage")?
            .join("stage-context.json"),
    )?;
    let end = read(root, &context)?["deadline_epoch_ms"]
        .as_u64()
        .context("native3_debug_deadline")?;
    let view = DebugView {
        root,
        tools: &recipe.tools,
        check_pins: &pins,
        map: &mapping,
        cwd,
        context,
        source_profile: &side.source_profile,
        side_ref,
        expectations: &image.expectations,
        binary,
        deadline: std::time::Instant::now(),
        deadline_epoch_ms: end,
        output_limit: usize::try_from(side.body_budget.output_bytes)?,
        budget: &side.body_budget,
        image_role: Some(&image.image_role),
    };
    verify_core(
        &view,
        artifact_ref,
        &observation.map,
        &observation.checks,
        &folder,
    )
}

// Explicit loader6 delegates actual debug/signature checks to the same DebugView core.
fn loader6_map_entries(
    names: &[String],
    invocation: &CodeNativeArchiveResponseInvocation,
    recipe: &CodeNativeBuildRecipe6,
) -> Result<Vec<CodeNativeDebugMapEntry>> {
    loader_map_inputs(
        names,
        &invocation.inputs,
        Path::new(&invocation.native_process.cwd),
        recipe,
    )
}
fn loader_map_inputs(
    names: &[String],
    inputs: &CodeNativeInputPreparationFacts,
    cwd: &Path,
    recipe: &CodeNativeBuildRecipe6,
) -> Result<Vec<CodeNativeDebugMapEntry>> {
    let mut result = Vec::new();
    for (ordinal, oso) in names.iter().enumerate() {
        let entry = if oso.ends_with(')') {
            let (container, member) = oso.rsplit_once('(').context("native3_archive_oso")?;
            let member = &member[..member.len() - 1];
            let (object, expected) = if Path::new(container).is_absolute() {
                let pin = recipe
                    .external_archive_bindings
                    .iter()
                    .find(|p| p.path == container)
                    .context("native3_external_oso_unselected")?;
                (pin.clone(), None)
            } else {
                let matches = inputs
                    .archives
                    .iter()
                    .filter(|a| format!("oe-native-debug/{}", a.logical_path) == container)
                    .collect::<Vec<_>>();
                ensure!(matches.len() == 1, "native3_own_archive_oso_unbound");
                let archive = matches[0];
                ensure!(
                    Path::new(&archive.staged_file.path) == cwd.join(container),
                    "native3_archive_oso_wrong_side"
                );
                let matches = archive
                    .members
                    .iter()
                    .filter(|m| m.staged_name == member)
                    .collect::<Vec<_>>();
                ensure!(matches.len() == 1, "native3_archive_oso_member_ambiguous");
                (archive.staged_file.clone(), Some(matches[0]))
            };
            crate::native_profile::verify_file(&object)?;
            let (header_ordinal, payload) = archive_member(
                &read_regular_bytes(Path::new(&object.path), 512 * 1024 * 1024)?,
                member,
            )?;
            if let Some(m) = expected {
                ensure!(
                    header_ordinal == m.staged_header_ordinal
                        && payload.len() as u64 == m.payload_bytes
                        && sha256(&payload) == m.payload_sha256,
                    "native3_archive_oso_payload_or_header_ordinal_changed"
                );
            }
            CodeNativeDebugMapEntry {
                ordinal: ordinal as u64,
                oso_name: oso.clone(),
                object,
                archive_member: Some(member.into()),
                archive_member_ordinal: Some(header_ordinal),
                payload_sha256: sha256(&payload),
            }
        } else {
            ensure!(!Path::new(oso).is_absolute(), "native3_own_oso_not_logical");
            let matches = inputs
                .direct_objects
                .iter()
                .filter(|m| format!("oe-native-debug/{}", m.logical_path) == *oso)
                .collect::<Vec<_>>();
            ensure!(
                matches.len() == 1,
                "native3_direct_oso_unbound_or_ambiguous"
            );
            let object = matches[0].staged_file.clone();
            crate::native_profile::verify_file(&object)?;
            ensure!(
                Path::new(&object.path) == cwd.join(oso)
                    && object.sha256 == matches[0].original_file.sha256
                    && object.bytes == matches[0].original_file.bytes,
                "native3_direct_oso_wrong_side_or_payload"
            );
            CodeNativeDebugMapEntry {
                ordinal: ordinal as u64,
                oso_name: oso.clone(),
                payload_sha256: object.sha256.clone(),
                object,
                archive_member: None,
                archive_member_ordinal: None,
            }
        };
        result.push(entry);
    }
    ensure!(!result.is_empty(), "native3_debug_map_empty");
    Ok(result)
}
pub(crate) fn observe_loader6(
    session: &crate::native_runtime6::NativeBuildSession6,
    artifact_ref: &ArtifactReference,
    a: &CodeSourceArtifact,
    invocations: &[CodeNativeArchiveResponseInvocation],
    image: &CodeNativeImageDebugExpectations3,
    folder: &Path,
) -> Result<CodeNativeDebugImageObservation3> {
    crate::native_profile6::inspect_debug_image(&session.root, &session.recipe, image)?;
    let binary = full_image_binary(a, image)?;
    let invocation = full_image_invocation(&binary, image, invocations)?;
    let cwd = Path::new(&invocation.native_process.cwd);
    let pins = || crate::native_runtime6::verify_pins(&session.root, &session.recipe).map(|_| ());
    let mapping = |names: &[String]| loader6_map_entries(names, invocation, &session.recipe);
    let context = reference(
        &session.root,
        &cwd.parent()
            .context("native3_debug_stage")?
            .join("stage-context.json"),
    )?;
    let view = DebugView {
        root: &session.root,
        tools: &session.recipe.tools,
        check_pins: &pins,
        map: &mapping,
        cwd,
        context,
        source_profile: &session.side.source_profile,
        side_ref: &session.side_ref,
        expectations: &image.expectations,
        binary: binary.clone(),
        deadline: session.deadline,
        deadline_epoch_ms: session.deadline_epoch_ms,
        output_limit: session.output_limit,
        budget: &session.side.body_budget,
        image_role: Some(&image.image_role),
    };
    let result = observe_core(&view, artifact_ref, folder)?;
    Ok(CodeNativeDebugImageObservation3{schema:"oev2.native-debug-image-observation/3".into(),image_role:image.image_role.clone(),expectations:image.expectations.clone(),
        artifact:artifact_ref.clone(),binary,map:result.map,checks:result.checks,
        uncovered:vec!["Finite frozen debug/source/type/location/required OSO and signature conditions; no full runtime behavior or engineering adoption authority".into()]})
}
pub(crate) fn verify_loader6(
    root: &SkillRoot,
    side_ref: &ArtifactReference,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    artifact_ref: &ArtifactReference,
    a: &CodeSourceArtifact,
    invocations: &[CodeNativeArchiveResponseInvocation],
    image: &CodeNativeImageDebugExpectations3,
    observation_ref: &ArtifactReference,
) -> Result<()> {
    crate::native_profile6::inspect_debug_image(root, recipe, image)?;
    let evidence = Path::new(&side.build.evidence_path);
    let folder = evidence.join("native-debug").join(&image.image_role);
    ensure!(
        observation_ref
            == &reference(
                root,
                &evidence.join(format!(
                    "native-debug-{}-observation.json",
                    image.image_role
                ))
            )?,
        "native3_image_observation_wrong_side"
    );
    let observation: CodeNativeDebugImageObservation3 =
        serde_json::from_value(read(root, observation_ref)?)?;
    observation
        .validate()
        .map_err(|e| anyhow::anyhow!("native3_debug_observation:{e:?}"))?;
    let binary = full_image_binary(a, image)?;
    ensure!(
        observation.schema == "oev2.native-debug-image-observation/3"
            && observation.image_role == image.image_role
            && observation.expectations == image.expectations
            && observation.artifact == *artifact_ref
            && observation.binary == binary,
        "native3_debug_image_binding_changed"
    );
    let invocation = full_image_invocation(&binary, image, invocations)?;
    let cwd = Path::new(&invocation.native_process.cwd);
    let pins = || crate::native_runtime6::verify_pins(root, recipe).map(|_| ());
    let mapping = |names: &[String]| loader6_map_entries(names, invocation, recipe);
    let context = reference(
        root,
        &cwd.parent()
            .context("native3_debug_stage")?
            .join("stage-context.json"),
    )?;
    let end = read(root, &context)?["deadline_epoch_ms"]
        .as_u64()
        .context("native3_debug_deadline")?;
    let view = DebugView {
        root,
        tools: &recipe.tools,
        check_pins: &pins,
        map: &mapping,
        cwd,
        context,
        source_profile: &side.source_profile,
        side_ref,
        expectations: &image.expectations,
        binary,
        deadline: std::time::Instant::now(),
        deadline_epoch_ms: end,
        output_limit: usize::try_from(side.body_budget.output_bytes)?,
        budget: &side.body_budget,
        image_role: Some(&image.image_role),
    };
    verify_core(
        &view,
        artifact_ref,
        &observation.map,
        &observation.checks,
        &folder,
    )
}

pub(crate) fn observe_sdk8(
    session: &crate::native_runtime6::NativeBuildSession6,
    artifact_ref: &ArtifactReference,
    a: &CodeSourceArtifact,
    invocations: &[CodeNativeInvocationEnvelope8],
    image: &CodeNativeImageDebugExpectations3,
    folder: &Path,
) -> Result<CodeNativeDebugImageObservation3> {
    crate::native_profile6::inspect_debug_image(&session.root, &session.recipe, image)?;
    let binary = full_image_binary(a, image)?;
    let invocation = sdk8_image_producer(&binary, image, invocations)?;
    let (inputs, process) = crate::native_link8::borrowed_inputs(invocation)?;
    let cwd = Path::new(&process.cwd);
    let pins = || crate::native_runtime6::verify_pins(&session.root, &session.recipe).map(|_| ());
    let mapping = |names: &[String]| loader_map_inputs(names, inputs, cwd, &session.recipe);
    let context = reference(
        &session.root,
        &cwd.parent()
            .context("native3_debug_stage")?
            .join("stage-context.json"),
    )?;
    let view = DebugView {
        root: &session.root,
        tools: &session.recipe.tools,
        check_pins: &pins,
        map: &mapping,
        cwd,
        context,
        source_profile: &session.side.source_profile,
        side_ref: &session.side_ref,
        expectations: &image.expectations,
        binary: binary.clone(),
        deadline: session.deadline,
        deadline_epoch_ms: session.deadline_epoch_ms,
        output_limit: session.output_limit,
        budget: &session.side.body_budget,
        image_role: Some(&image.image_role),
    };
    let result = observe_core(&view, artifact_ref, folder)?;
    Ok(CodeNativeDebugImageObservation3{schema:"oev2.native-debug-image-observation/3".into(),image_role:image.image_role.clone(),expectations:image.expectations.clone(),
        artifact:artifact_ref.clone(),binary,map:result.map,checks:result.checks,
        uncovered:vec!["Finite frozen debug/source/type/location/required OSO and signature conditions; no full runtime behavior or engineering adoption authority".into()]})
}
pub(crate) fn verify_sdk8(
    root: &SkillRoot,
    side_ref: &ArtifactReference,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    artifact_ref: &ArtifactReference,
    a: &CodeSourceArtifact,
    invocations: &[CodeNativeInvocationEnvelope8],
    image: &CodeNativeImageDebugExpectations3,
    observation_ref: &ArtifactReference,
) -> Result<()> {
    crate::native_profile6::inspect_debug_image(root, recipe, image)?;
    let evidence = Path::new(&side.build.evidence_path);
    let folder = evidence.join("native-debug").join(&image.image_role);
    ensure!(
        observation_ref
            == &reference(
                root,
                &evidence.join(format!(
                    "native-debug-{}-observation.json",
                    image.image_role
                ))
            )?,
        "native3_image_observation_wrong_side"
    );
    let observation: CodeNativeDebugImageObservation3 =
        serde_json::from_value(read(root, observation_ref)?)?;
    observation
        .validate()
        .map_err(|e| anyhow::anyhow!("native3_debug_observation:{e:?}"))?;
    let binary = full_image_binary(a, image)?;
    ensure!(
        observation.schema == "oev2.native-debug-image-observation/3"
            && observation.image_role == image.image_role
            && observation.expectations == image.expectations
            && observation.artifact == *artifact_ref
            && observation.binary == binary,
        "native3_debug_image_binding_changed"
    );
    let invocation = sdk8_image_producer(&binary, image, invocations)?;
    let (inputs, process) = crate::native_link8::borrowed_inputs(invocation)?;
    let cwd = Path::new(&process.cwd);
    let pins = || crate::native_runtime6::verify_pins(root, recipe).map(|_| ());
    let mapping = |names: &[String]| loader_map_inputs(names, inputs, cwd, recipe);
    let context = reference(
        root,
        &cwd.parent()
            .context("native3_debug_stage")?
            .join("stage-context.json"),
    )?;
    let end = read(root, &context)?["deadline_epoch_ms"]
        .as_u64()
        .context("native3_debug_deadline")?;
    let view = DebugView {
        root,
        tools: &recipe.tools,
        check_pins: &pins,
        map: &mapping,
        cwd,
        context,
        source_profile: &side.source_profile,
        side_ref,
        expectations: &image.expectations,
        binary,
        deadline: std::time::Instant::now(),
        deadline_epoch_ms: end,
        output_limit: usize::try_from(side.body_budget.output_bytes)?,
        budget: &side.body_budget,
        image_role: Some(&image.image_role),
    };
    verify_core(
        &view,
        artifact_ref,
        &observation.map,
        &observation.checks,
        &folder,
    )
}

fn sdk8_image_producer<'a>(
    binary: &CodeNativeFileBinding,
    image: &CodeNativeImageDebugExpectations3,
    all: &'a [CodeNativeInvocationEnvelope8],
) -> Result<&'a CodeNativeInvocationEnvelope8> {
    let stage = match image.image_role.as_str() {
        "normalizer" => "build-normalizer",
        "runner" => "build-runner",
        _ => anyhow::bail!("sdk8_image_role"),
    };
    let deps = Path::new(&binary.path)
        .parent()
        .context("sdk8_image_parent")?
        .join("deps");
    let mut hits = Vec::new();
    for v in all {
        crate::native_link8::inspect_envelope(v)?;
        if crate::native_link8::parent_stage(v)? != stage || v.compiler7.is_some() {
            continue;
        }
        let arguments = if let Some(a) = &v.archive3 {
            &a.inputs.expanded_arguments
        } else {
            &v.sdk8
                .as_ref()
                .unwrap()
                .inputs
                .original_request
                .received_arguments
        };
        for window in arguments.windows(2).filter(|w| w[0] == "-o") {
            let path = Path::new(&window[1]);
            if path != Path::new(&binary.path) && path.parent() != Some(deps.as_path()) {
                continue;
            }
            if !path.exists() {
                continue;
            }
            let actual = file_binding(path)?;
            if actual.sha256 == binary.sha256 && actual.bytes == binary.bytes {
                hits.push(v);
            }
        }
    }
    ensure!(
        hits.len() == 1,
        "sdk8_actual_image_producer_missing_or_ambiguous"
    );
    Ok(hits[0])
}
