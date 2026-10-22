//! Read-only retrospective dialogue sources, not reconstructed historical runs.
//! Visible text and tool observations keep exact raw-record locators. Reasoning,
//! runtime instructions, snapshots and duplicate event mirrors are not projected.
use anyhow::{Context, Result, ensure};
use oe_core::{open_regular_file, sha256};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{BufRead, BufReader},
    path::Path,
};

pub const SCHEMA: &str = "ontology-engineering.conversation-source/v1";
pub const MAX_RECORD_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_STREAM_RECORD_BYTES: usize = 64 * 1024 * 1024;

/// Full-corpus callers stream entries and gaps instead of accumulating a file.
/// The captured byte limit and per-record memory guard remain explicit.
pub struct ScanOptions {
    pub max_bytes: u64,
    pub max_record_bytes: usize,
    pub include_tool_arguments: bool,
}

pub enum ScanEvent {
    Entry(Entry),
    Gap(Value),
}

/// A project-specific requirement can still merit skill source review. This
/// composition never accepts facts, adopts a guide or transfers authorization.
pub fn review_disposition(value: Option<&str>, focus: Option<&str>) -> Result<&'static str> {
    ensure!(
        value.is_none_or(|v| matches!(
            v,
            "reusable" | "project_specific" | "unrelated" | "not_established"
        )),
        "invalid_experience_value_choice"
    );
    ensure!(
        focus.is_none_or(|v| matches!(
            v,
            "architecture" | "content" | "both" | "neither" | "not_established"
        )),
        "invalid_experience_focus_choice"
    );
    Ok(
        if value.is_none()
            || focus.is_none()
            || value == Some("not_established")
            || focus == Some("not_established")
        {
            "needs_context"
        } else if value == Some("reusable")
            || matches!(focus, Some("architecture" | "content" | "both"))
        {
            "source_review"
        } else if value == Some("project_specific") {
            "project_history"
        } else {
            "no_relevant_delta"
        },
    )
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub line: u64,
    pub byte_start: u64,
    pub byte_end: u64,
    pub raw_sha256: String,
    pub record_id: Option<String>,
    pub parent_id: Option<String>,
    pub timestamp: Option<String>,
    pub kind: String,
    pub role: String,
    pub text: String,
    pub call_id: Option<String>,
}

fn string(v: &Value, name: &str) -> Option<String> {
    v.get(name).and_then(Value::as_str).map(str::to_owned)
}

fn text_blocks(value: &Value) -> String {
    if let Some(text) = value.as_str() {
        return text.to_owned();
    }
    value.as_array().map_or_else(String::new, |blocks| {
        blocks
            .iter()
            .filter_map(|b| {
                matches!(
                    b["type"].as_str(),
                    Some("text" | "input_text" | "output_text")
                )
                .then(|| b["text"].as_str())
                .flatten()
            })
            .collect::<Vec<_>>()
            .join("\n")
    })
}

/// Parse a raw record structurally. Archived text is evidence, never a policy.
pub fn visible_record(record: &Value) -> Vec<Value> {
    if record["type"] == "response_item" {
        let p = &record["payload"];
        match p["type"].as_str() {
            Some("message")
                if matches!(p["role"].as_str(), Some("user" | "assistant"))
                    && !matches!(p["channel"].as_str(), Some("analysis" | "summary")) =>
            {
                let text = text_blocks(&p["content"]);
                if text.is_empty() {
                    return Vec::new();
                }
                // Codex archives also encode injected environment/AGENTS and
                // automatic goal continuations as user-role messages. Keep
                // their source bytes, but never count them as a new correction
                // or an independent user requirement.
                let runtime_context = p["role"] == "user"
                    && [
                        "# AGENTS.md instructions",
                        "<environment_context>",
                        "<codex_internal_context",
                    ]
                    .iter()
                    .any(|prefix| text.trim_start().starts_with(prefix));
                return vec![
                    json!({"kind":if runtime_context{"runtime_context"}else{"message"},
                    "role":if runtime_context{json!("context")}else{p["role"].clone()}, "text":text,
                    "record_id":p["id"], "call_id":null}),
                ];
            }
            Some("function_call" | "custom_tool_call") => {
                return vec![json!({
                "kind":"tool_call", "role":"tool", "text":p["name"].as_str().unwrap_or("unknown"),
                "record_id":p["id"], "call_id":p["call_id"]})];
            }
            Some("function_call_output" | "custom_tool_call_output") => {
                return vec![json!({
                "kind":"tool_result", "role":"tool", "text":text_blocks(&p["output"]),
                "record_id":p["id"], "call_id":p["call_id"]})];
            }
            _ => return Vec::new(),
        }
    }
    if !matches!(record["type"].as_str(), Some("user" | "assistant")) {
        return Vec::new();
    }
    let message = &record["message"];
    if !matches!(message["role"].as_str(), Some("user" | "assistant")) {
        return Vec::new();
    }
    let content = &message["content"];
    let runtime_context = record["promptSource"] == "system" || record["isMeta"] == true;
    let role = if runtime_context {
        json!("context")
    } else {
        message["role"].clone()
    };
    let kind = if runtime_context {
        "runtime_context"
    } else {
        "message"
    };
    let mut result = Vec::new();
    if content.is_string() {
        result.push(json!({"kind":kind, "role":role, "text":content,
            "record_id":record["uuid"], "call_id":null}));
    } else if let Some(blocks) = content.as_array() {
        for block in blocks {
            match block["type"].as_str() {
                Some("text") => result.push(json!({"kind":kind, "role":role,
                    "text":block["text"], "record_id":record["uuid"], "call_id":null})),
                Some("tool_use") => result.push(json!({"kind":"tool_call", "role":"tool",
                    "text":block["name"], "record_id":record["uuid"], "call_id":block["id"]})),
                Some("tool_result") => result.push(json!({"kind":"tool_result", "role":"tool",
                    "text":text_blocks(&block["content"]), "record_id":record["uuid"],
                    "call_id":block["tool_use_id"]})),
                _ => {} // Images and private thinking stay at their original source.
            }
        }
    }
    result
}

/// Explicit corpus intake can also retain visible intervention arguments. They
/// remain untrusted quoted source text and pass the same model privacy boundary.
/// Appending them keeps the original visible block locators stable.
pub fn visible_record_with_arguments(record: &Value) -> Vec<Value> {
    let mut result = visible_record(record);
    if record["type"] == "response_item" {
        let p = &record["payload"];
        if matches!(
            p["type"].as_str(),
            Some("function_call" | "custom_tool_call")
        ) {
            let args = p.get("arguments").or_else(|| p.get("input"));
            if let Some(args) = args {
                let text = if let Some(text) = args.as_str() {
                    match oe_core::json::strict_json(text.as_bytes()) {
                        Ok(parsed) => {
                            serde_json::to_string_pretty(&parsed).unwrap_or_else(|_| text.into())
                        }
                        Err(_) => text.into(),
                    }
                } else {
                    serde_json::to_string_pretty(args).unwrap_or_default()
                };
                result.push(
                    json!({"kind":"tool_call_arguments","role":"tool","text":text,
                    "record_id":p["id"],"call_id":p["call_id"]}),
                );
            }
        }
    } else if matches!(record["type"].as_str(), Some("user" | "assistant"))
        && matches!(
            record["message"]["role"].as_str(),
            Some("user" | "assistant")
        )
        && let Some(blocks) = record["message"]["content"].as_array()
    {
        for block in blocks {
            if block["type"] == "tool_use" && !block["input"].is_null() {
                result.push(json!({"kind":"tool_call_arguments","role":"tool",
                    "text":serde_json::to_string_pretty(&block["input"]).unwrap_or_default(),
                    "record_id":record["uuid"],"call_id":block["id"]}));
            }
        }
    }
    result
}

/// Scan complete records within a byte budget, with bounded memory per record.
/// Appended bytes belong to a later intake. Mutation inside the captured prefix
/// is detected by verify_entry rather than inheriting the old observation.
pub fn read_archive(path: &Path, max_bytes: u64) -> Result<Value> {
    ensure!(
        (1..=512 * 1024 * 1024).contains(&max_bytes),
        "invalid_archive_scan_budget"
    );
    let mut entries = Vec::new();
    let mut issues = Vec::new();
    let mut report = visit_archive(
        path,
        ScanOptions {
            max_bytes,
            max_record_bytes: MAX_RECORD_BYTES,
            include_tool_arguments: false,
        },
        |event| {
            match event {
                ScanEvent::Entry(entry) => entries.push(entry),
                ScanEvent::Gap(issue) => issues.push(issue),
            }
            Ok(())
        },
    )?;
    report["entries"] = serde_json::to_value(entries)?;
    report["issues"] = serde_json::to_value(issues)?;
    Ok(report)
}

/// Visit every complete captured record, retaining gaps without claiming that
/// snapshots, private reasoning, attachments or unsupported formats were read.
pub fn visit_archive(
    path: &Path,
    options: ScanOptions,
    visit: impl FnMut(ScanEvent) -> Result<()>,
) -> Result<Value> {
    visit_range(path, options, None, visit)
}

/// An explicit record-aligned range. Byte locators remain absolute; line numbers
/// are relative to the selected range and never masquerade as archive lines.
pub fn read_window(path: &Path, start: u64, end: u64) -> Result<Value> {
    ensure!(
        end > start && end - start <= 512 * 1024 * 1024,
        "invalid_archive_window_budget"
    );
    let mut entries = Vec::new();
    let mut issues = Vec::new();
    let mut report = visit_range(
        path,
        ScanOptions {
            max_bytes: end - start,
            max_record_bytes: MAX_RECORD_BYTES,
            include_tool_arguments: false,
        },
        Some((start, end)),
        |event| {
            match event {
                ScanEvent::Entry(e) => entries.push(e),
                ScanEvent::Gap(i) => issues.push(i),
            };
            Ok(())
        },
    )?;
    report["entries"] = serde_json::to_value(entries)?;
    report["issues"] = serde_json::to_value(issues)?;
    Ok(report)
}

fn visit_range(
    path: &Path,
    options: ScanOptions,
    range: Option<(u64, u64)>,
    mut visit: impl FnMut(ScanEvent) -> Result<()>,
) -> Result<Value> {
    use std::io::{Read, Seek, SeekFrom};
    ensure!(options.max_bytes > 0, "invalid_archive_scan_budget");
    ensure!(
        (MAX_RECORD_BYTES..=MAX_STREAM_RECORD_BYTES).contains(&options.max_record_bytes),
        "invalid_stream_record_guard"
    );
    let mut file = open_regular_file(path, u64::MAX)?;
    let original_size = file.metadata()?.len();
    let (range_start, scan_limit) = range.unwrap_or((0, options.max_bytes.min(original_size)));
    if range.is_some() {
        ensure!(scan_limit <= original_size, "archive_window_outside_source");
        for boundary in [range_start, scan_limit] {
            if boundary > 0 {
                file.seek(SeekFrom::Start(boundary - 1))?;
                let mut byte = [0];
                file.read_exact(&mut byte)?;
                ensure!(byte[0] == b'\n', "archive_window_not_record_aligned");
            }
        }
        file.seek(SeekFrom::Start(range_start))?;
    }
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut line = 0u64;
    let mut offset = range_start;
    let mut digest = Sha256::new();
    let mut entry_count = 0u64;
    let mut duplicate_items = 0u64;
    let mut issue_count = 0u64;
    let mut record_types = BTreeMap::<String, u64>::new();
    let mut seen = BTreeSet::new();
    let mut metadata = json!({});
    let mut status = "complete";
    loop {
        if offset >= scan_limit {
            if range.is_some() {
                status = "window_complete";
            } else if offset < original_size {
                status = "scan_budget_reached";
            }
            break;
        }
        let start = offset;
        let mut bytes = Vec::new();
        let mut oversized = false;
        let mut terminated = false;
        let mut record_digest = Sha256::new();
        loop {
            let available = reader.fill_buf()?;
            if available.is_empty() {
                break;
            }
            let remaining = usize::try_from(scan_limit - offset)?.min(available.len());
            let window = &available[..remaining];
            let amount = window
                .iter()
                .position(|b| *b == b'\n')
                .map_or(window.len(), |n| n + 1);
            let chunk = &window[..amount];
            digest.update(chunk);
            record_digest.update(chunk);
            if bytes.len() + amount <= options.max_record_bytes && !oversized {
                bytes.extend_from_slice(chunk);
            } else {
                oversized = true;
                bytes.clear();
            }
            terminated = chunk.last() == Some(&b'\n');
            reader.consume(amount);
            offset += u64::try_from(amount)?;
            if terminated || offset >= scan_limit {
                break;
            }
        }
        if offset == start {
            break;
        }
        line += 1;
        let raw_sha256 = format!("{:x}", record_digest.finalize());
        if !terminated {
            issue_count += 1;
            visit(ScanEvent::Gap(
                json!({"line":line,"byte_start":start,"byte_end":offset,
                "raw_sha256":raw_sha256,
                "code":"unfinished_record","retained_at_original_source":true}),
            ))?;
            status = "partial";
            break;
        }
        if oversized {
            issue_count += 1;
            visit(ScanEvent::Gap(
                json!({"line":line,"byte_start":start,"byte_end":offset,
                "raw_sha256":raw_sha256,"code":"record_exceeds_memory_guard",
                "record_memory_guard":options.max_record_bytes,"retained_at_original_source":true}),
            ))?;
            continue;
        }
        let record = match oe_core::json::strict_json(&bytes) {
            Ok(v) => v,
            Err(_) => {
                issue_count += 1;
                visit(ScanEvent::Gap(
                    json!({"line":line,"byte_start":start,"byte_end":offset,
                    "raw_sha256":raw_sha256,"code":"invalid_json_record",
                    "retained_at_original_source":true}),
                ))?;
                continue;
            }
        };
        let record_type = format!(
            "{}:{}",
            record["type"].as_str().unwrap_or("untyped"),
            record["payload"]["type"].as_str().unwrap_or("")
        );
        *record_types.entry(record_type).or_default() += 1;
        if record["type"] == "session_meta" {
            let p = &record["payload"];
            metadata = json!({"session_id":p["id"],"cwd":p["cwd"],"timestamp":p["timestamp"],"format":"codex",
                "source":p["source"],"forked_from_id":p["forked_from_id"],"originator":p["originator"]});
        } else if metadata["session_id"].is_null() && record["sessionId"].is_string() {
            metadata = json!({"session_id":record["sessionId"],"cwd":record["cwd"],"timestamp":record["timestamp"],"format":"claude"});
        }
        if metadata["format"] == "claude" && metadata["cwd"].is_null() && record["cwd"].is_string()
        {
            metadata["cwd"] = record["cwd"].clone();
            metadata["timestamp"] = record["timestamp"].clone();
        }
        let visible = if options.include_tool_arguments {
            visible_record_with_arguments(&record)
        } else {
            visible_record(&record)
        };
        for (block, item) in visible.into_iter().enumerate() {
            let text = item["text"].as_str().unwrap_or("");
            if text.is_empty() {
                continue;
            }
            // De-duplicate only an exact identified item; equal text in two
            // different turns may have different meaning and is retained.
            let identity = json!([item["record_id"], block, item["kind"], sha256(text)]);
            if item["record_id"].is_string() && !seen.insert(identity.to_string()) {
                duplicate_items += 1;
                continue;
            }
            entry_count += 1;
            visit(ScanEvent::Entry(Entry {
                id: format!("r{line}b{block}"),
                line,
                byte_start: start,
                byte_end: offset,
                raw_sha256: raw_sha256.clone(),
                record_id: string(&item, "record_id"),
                parent_id: string(&record, "parentUuid"),
                timestamp: string(&record, "timestamp"),
                kind: item["kind"].as_str().context("entry_kind_missing")?.into(),
                role: item["role"].as_str().context("entry_role_missing")?.into(),
                text: text.into(),
                call_id: string(&item, "call_id"),
            }))?;
        }
    }
    let scanned_sha256 = format!("{:x}", digest.finalize());
    if offset < scan_limit {
        issue_count += 1;
        visit(ScanEvent::Gap(
            json!({"byte_start":offset,"byte_end":scan_limit,
            "code":"source_ended_before_snapshot_limit","retained_at_original_source":true}),
        ))?;
        status = "partial";
    }
    if issue_count > 0 && ["complete", "window_complete"].contains(&status) {
        status = "partial";
    }
    Ok(
        json!({"schema":SCHEMA,"source_path":path,"original_size":original_size,
        "scanned_bytes":offset-range_start,"scanned_prefix_sha256":if range.is_none(){json!(scanned_sha256)}else{Value::Null},"scanned_range_sha256":scanned_sha256,"lines":line,
        "window":range.map(|(start,end)|json!({"byte_start":start,"byte_end":end,"line_origin":"range_relative","outside_window":"not_scanned"})),
        "status":status,"metadata":metadata,"entry_count":entry_count,"issue_count":issue_count,
        "exact_identified_duplicate_items":duplicate_items,"record_types":record_types,
        "record_memory_guard":options.max_record_bytes,
        "source_kind":"retrospective_archive","historical_execution_reconstructed":false,
        "engineering_acceptance":"not_established","ontology_promotion":"not_performed"}),
    )
}

/// Verify the original raw record before using a selected historical entry.
pub fn verify_entry(path: &Path, entry: &Entry) -> Result<()> {
    use std::io::{Read, Seek, SeekFrom};
    ensure!(entry.byte_end > entry.byte_start, "invalid_record_range");
    let len = entry.byte_end - entry.byte_start;
    ensure!(
        len <= MAX_STREAM_RECORD_BYTES as u64,
        "record_range_exceeds_guard"
    );
    let mut file = open_regular_file(path, u64::MAX)?;
    file.seek(SeekFrom::Start(entry.byte_start))?;
    let mut bytes = vec![0; usize::try_from(len)?];
    file.read_exact(&mut bytes)?;
    ensure!(
        sha256(&bytes) == entry.raw_sha256,
        "historical_record_changed"
    );
    let record = oe_core::json::strict_json(&bytes)?;
    let values = visible_record_with_arguments(&record);
    let block = entry
        .id
        .rsplit_once('b')
        .context("entry_id_missing_block")?
        .1
        .parse::<usize>()?;
    let item = values.get(block).context("selected_block_not_visible")?;
    ensure!(
        item["text"] == entry.text
            && item["kind"] == entry.kind
            && item["role"] == entry.role
            && string(item, "record_id") == entry.record_id
            && string(item, "call_id") == entry.call_id
            && string(&record, "parentUuid") == entry.parent_id
            && string(&record, "timestamp") == entry.timestamp
            && entry.id == format!("r{}b{block}", entry.line),
        "historical_projection_changed"
    );
    Ok(())
}

/// Replace personal home prefixes in a model projection, retaining the source.
/// Other findings from the shared scanner block transmission for local review.
pub fn model_text(text: &str) -> Result<String> {
    let mut projected = text.to_owned();
    for prefix in ["/Users/", "/home/", "/var/folders/"] {
        while let Some(start) = projected.find(prefix) {
            let tail = &projected[start + prefix.len()..];
            // A wrapped home component can be separated by a newline. Mask it
            // only when the continuation is actually followed by a path slash;
            // otherwise mask the bare prefix and continue to later occurrences.
            let leading = tail.len() - tail.trim_start_matches(char::is_whitespace).len();
            let continued = &tail[leading..];
            let len = continued
                .chars()
                .take_while(|c| {
                    !c.is_whitespace() && !matches!(c, '/' | '\\' | '"' | '\'' | '<' | '>' | '`')
                })
                .map(char::len_utf8)
                .sum::<usize>();
            let wrapped_path = leading > 0 && continued.as_bytes().get(len) == Some(&b'/');
            let remove = if leading == 0 {
                len
            } else if wrapped_path {
                leading + len
            } else {
                0
            };
            projected.replace_range(start..start + prefix.len() + remove, "[private-root]");
        }
    }
    ensure!(
        oe_core::privacy::inspect_text(&projected).is_empty(),
        "model_projection_requires_local_privacy_review"
    );
    Ok(projected)
}

/// A redaction is a visible context gap, not a negative semantic judgment.
/// Entire secret-bearing lines stay at the source; private-key material blocks
/// the whole entry so masking its header cannot expose its unmarked body.
pub fn project_visible_text(text: &str) -> Result<Value> {
    if oe_core::privacy::inspect_text(text)
        .iter()
        .any(|f| f.rule == "private key material")
    {
        return Ok(
            json!({"text":"[source entry withheld: private key material]",
            "omissions":[{"byte_start":0,"byte_end":text.len(),"rules":["private key material"]}]}),
        );
    }
    let mut output = String::new();
    let mut omissions = Vec::new();
    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        match model_text(line) {
            Ok(projected) => output.push_str(&projected),
            Err(_) => {
                let rules: BTreeSet<_> = oe_core::privacy::inspect_text(line)
                    .into_iter()
                    .map(|f| f.rule)
                    .collect();
                omissions
                    .push(json!({"byte_start":offset,"byte_end":offset+line.len(),"rules":rules}));
                output.push_str("[source line withheld: privacy gap]\n");
            }
        }
        offset += line.len();
    }
    ensure!(
        oe_core::privacy::inspect_text(&output).is_empty(),
        "redaction_requires_local_privacy_review"
    );
    Ok(json!({"text":output,"omissions":omissions}))
}
