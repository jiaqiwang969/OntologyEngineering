//! Engineering-memory is an application of the v2 design chain. This adapter
//! reads original records, independently of the derived search index.
#[rustfmt::skip]
pub mod contracts;

use anyhow::{Context, Result, ensure};
use clap::Args;
use contracts::SourceAnchor;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs::{File, Metadata};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const WINDOW_LIMIT: u64 = 16 * 1024 * 1024;
const HEADER_LIMIT: u64 = 1024 * 1024;

#[derive(Args, Debug, Clone)]
pub struct EvidenceArgs {
    /// Explicit original JSONL source; never inferred from a search row number.
    #[arg(long)]
    pub source: PathBuf,
    #[arg(long, value_parser = ["codex", "claude"])]
    pub provider: String,
    /// Caller-declared profile label, never a Project identity.
    #[arg(long)]
    pub profile: Option<String>,
    #[arg(long, default_value_t = 0)]
    pub start_byte: u64,
    /// Exclusive byte boundary. Default reads at most 16 MiB.
    #[arg(long)]
    pub end_byte: Option<u64>,
    #[arg(long, default_value_t = 2 * 1024 * 1024)]
    pub max_record_bytes: u64,
    /// Output is always JSON; accepted for robot CLI consistency.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Serialize)]
pub struct Gap {
    pub byte_start: u64,
    pub byte_end: u64,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct EvidenceWindow {
    pub schema: &'static str,
    pub source_path: String,
    pub source_size: u64,
    pub byte_start: u64,
    pub byte_end: u64,
    pub window_sha256: String,
    pub next_byte: Option<u64>,
    pub records: Vec<SourceAnchor>,
    pub gaps: Vec<Gap>,
    pub excluded_records: u64,
    pub complete_visible_window: bool,
    pub whole_session_reviewed: bool,
    pub source_appended_after_snapshot: bool,
    pub scope: &'static str,
}

fn string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)?
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .map(str::to_owned)
}

fn visible_text(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return Some(text.to_owned());
    }
    let blocks = value.as_array()?;
    let parts: Vec<&str> = blocks
        .iter()
        .filter_map(|block| {
            matches!(
                block["type"].as_str(),
                Some("text" | "input_text" | "output_text")
            )
            .then(|| block["text"].as_str())
            .flatten()
        })
        .collect();
    (!parts.is_empty()).then(|| parts.join("\n"))
}

/// Only explicit user/assistant message text is eligible. Runtime records,
/// tool results, event mirrors and hidden analysis are excluded, not summarized.
fn message<'a>(value: &'a Value, provider: &str) -> Option<(&'a Value, String)> {
    let body = match provider {
        "codex" if value["type"] == "response_item" && value["payload"]["type"] == "message" => {
            &value["payload"]
        }
        "claude" if matches!(value["type"].as_str(), Some("user" | "assistant")) => {
            &value["message"]
        }
        _ => return None,
    };
    if !matches!(body["role"].as_str(), Some("user" | "assistant"))
        || matches!(body["channel"].as_str(), Some("analysis" | "summary"))
        || matches!(body["phase"].as_str(), Some("analysis" | "summary"))
    {
        return None;
    }
    Some((body, visible_text(&body["content"])?))
}

fn same_file(before: &Metadata, after: &Metadata) -> Result<()> {
    ensure!(
        after.is_file() && after.len() >= before.len(),
        "source_changed_during_read"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(
            before.dev() == after.dev() && before.ino() == after.ino(),
            "source_replaced_during_read"
        );
    }
    #[cfg(not(unix))]
    ensure!(
        before.len() == after.len() && before.modified()? == after.modified()?,
        "source_changed_during_read"
    );
    Ok(())
}

fn verify_window_snapshot(
    source: &Path,
    before: &Metadata,
    start: u64,
    bytes: &[u8],
    header: &[u8],
) -> Result<bool> {
    // Reopen the actual path and reread exactly the identity header and selected
    // bytes. Appends elsewhere do not invalidate a stable historical window.
    let mut current = File::open(source)?;
    same_file(before, &current.metadata()?)?;
    let mut buffer = [0u8; 65536];
    for (offset, expected) in [(0, header), (start, bytes)] {
        current.seek(SeekFrom::Start(offset))?;
        for chunk in expected.chunks(buffer.len()) {
            current.read_exact(&mut buffer[..chunk.len()])?;
            ensure!(
                buffer[..chunk.len()] == *chunk,
                "source_changed_during_read"
            );
        }
    }
    let after = current.metadata()?;
    same_file(before, &after)?;
    same_file(before, &source.metadata()?)?;
    Ok(after.len() > before.len())
}

pub fn read_window(args: &EvidenceArgs) -> Result<EvidenceWindow> {
    ensure!(
        matches!(args.provider.as_str(), "codex" | "claude"),
        "unsupported_provider"
    );
    ensure!(
        (1..=WINDOW_LIMIT).contains(&args.max_record_bytes),
        "record_budget_invalid"
    );
    let source = args.source.canonicalize().context("source_unavailable")?;
    ensure!(source.metadata()?.is_file(), "source_not_regular");
    let mut file = File::open(&source)?;
    let before = file.metadata()?;
    ensure!(before.is_file(), "source_not_regular");
    let end = args.end_byte.unwrap_or(
        before
            .len()
            .min(args.start_byte.saturating_add(WINDOW_LIMIT)),
    );
    ensure!(
        args.start_byte <= end && end <= before.len(),
        "source_range_invalid"
    );
    ensure!(
        end - args.start_byte <= WINDOW_LIMIT,
        "window_budget_exceeded"
    );
    if args.start_byte > 0 {
        file.seek(SeekFrom::Start(args.start_byte - 1))?;
        let mut previous = [0];
        file.read_exact(&mut previous)?;
        ensure!(previous[0] == b'\n', "start_not_record_boundary");
    }
    // Codex session identity is declared in the source header, never supplied
    // by cwd, profile or filename. Claude keeps sessionId in every message.
    let mut session = None;
    let mut header = Vec::new();
    if args.provider == "codex" {
        file.seek(SeekFrom::Start(0))?;
        Read::by_ref(&mut file)
            .take(HEADER_LIMIT.min(before.len()))
            .read_to_end(&mut header)?;
        for raw in header.split_inclusive(|b| *b == b'\n') {
            if let Ok(value) = serde_json::from_slice::<Value>(raw)
                && value["type"] == "session_meta"
            {
                session = string(&value["payload"], "id");
                break;
            }
        }
    }
    file.seek(SeekFrom::Start(args.start_byte))?;
    let mut bytes = vec![0; usize::try_from(end - args.start_byte)?];
    file.read_exact(&mut bytes)?;
    let mut result = EvidenceWindow {
        schema: "engineering-memory.evidence-window/1",
        source_path: source.to_string_lossy().into_owned(),
        source_size: before.len(),
        byte_start: args.start_byte,
        byte_end: end,
        window_sha256: hex::encode(Sha256::digest(&bytes)),
        next_byte: None,
        records: Vec::new(),
        gaps: Vec::new(),
        excluded_records: 0,
        complete_visible_window: false,
        whole_session_reviewed: false,
        source_appended_after_snapshot: false,
        scope: "visible selected records only; hashes establish byte identity, not truth or engineering adoption",
    };
    let mut offset = args.start_byte;
    for raw in bytes.split_inclusive(|b| *b == b'\n') {
        if result.records.len() + result.gaps.len() >= 256 {
            result.gaps.push(Gap {
                byte_start: offset,
                byte_end: end,
                reason: "result_count_budget".into(),
            });
            result.next_byte = Some(offset);
            break;
        }
        let start = offset;
        offset += raw.len() as u64;
        let truncated = offset == end && end < before.len() && raw.last() != Some(&b'\n');
        let reason = if truncated {
            Some("truncated_record")
        } else if raw.len() as u64 > args.max_record_bytes {
            Some("record_budget_exceeded")
        } else {
            None
        };
        if let Some(reason) = reason {
            result.gaps.push(Gap {
                byte_start: start,
                byte_end: offset,
                reason: reason.into(),
            });
            if truncated {
                result.next_byte = (start > args.start_byte).then_some(start);
                break;
            }
            continue;
        }
        let value: Value = match serde_json::from_slice(raw) {
            Ok(value) => value,
            Err(_) => {
                result.gaps.push(Gap {
                    byte_start: start,
                    byte_end: offset,
                    reason: "invalid_json".into(),
                });
                continue;
            }
        };
        let Some((body, text)) = message(&value, &args.provider) else {
            result.excluded_records += 1;
            continue;
        };
        let source_session = if args.provider == "claude" {
            string(&value, "sessionId")
        } else {
            session.clone()
        };
        let Some(session_id) = source_session else {
            result.gaps.push(Gap {
                byte_start: start,
                byte_end: offset,
                reason: "session_identity_missing".into(),
            });
            continue;
        };
        result.records.push(SourceAnchor {
            provider: args.provider.clone(),
            profile: args.profile.clone(),
            session_id,
            source_path: result.source_path.clone(),
            byte_start: start,
            byte_end: offset,
            sha256: hex::encode(Sha256::digest(raw)),
            role: string(body, "role").context("role_missing")?,
            text,
            event_at: string(&value, "timestamp"),
            message_id: if args.provider == "claude" {
                string(&value, "uuid").or_else(|| string(body, "id"))
            } else {
                string(body, "id")
            },
            parent_id: string(&value, "parentUuid").or_else(|| string(&value, "parent_id")),
        });
    }
    result.source_appended_after_snapshot =
        verify_window_snapshot(&source, &before, args.start_byte, &bytes, &header)?;
    result.complete_visible_window = result.gaps.is_empty();
    if result.next_byte.is_none() && end < before.len() && bytes.last() == Some(&b'\n') {
        result.next_byte = Some(end);
    }
    Ok(result)
}

#[cfg(test)]
mod snapshot_tests {
    use super::verify_window_snapshot;
    use std::fs::{self, OpenOptions};
    use std::io::{Seek, SeekFrom, Write};

    #[test]
    fn append_is_accepted_but_selected_text_header_and_replacement_are_not() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("session.jsonl");
        let header = b"session-a\n";
        let bytes = b"selected original\n";
        fs::write(&source, [header.as_slice(), bytes.as_slice()].concat()).unwrap();
        let before = source.metadata().unwrap();
        OpenOptions::new()
            .append(true)
            .open(&source)
            .unwrap()
            .write_all(b"new tail\n")
            .unwrap();
        assert!(
            verify_window_snapshot(&source, &before, header.len() as u64, bytes, header).unwrap()
        );
        let mut changed = OpenOptions::new().write(true).open(&source).unwrap();
        changed.seek(SeekFrom::Start(header.len() as u64)).unwrap();
        changed.write_all(b"X").unwrap();
        assert!(
            verify_window_snapshot(&source, &before, header.len() as u64, bytes, header).is_err()
        );
        changed.seek(SeekFrom::Start(header.len() as u64)).unwrap();
        changed.write_all(bytes).unwrap();
        changed.seek(SeekFrom::Start(0)).unwrap();
        changed.write_all(b"session-b\n").unwrap();
        assert!(
            verify_window_snapshot(&source, &before, header.len() as u64, bytes, header).is_err()
        );
        changed.seek(SeekFrom::Start(0)).unwrap();
        changed.write_all(header).unwrap();
        drop(changed);
        fs::rename(&source, temp.path().join("retained-original.jsonl")).unwrap();
        fs::write(&source, [header.as_slice(), bytes.as_slice()].concat()).unwrap();
        #[cfg(unix)]
        assert!(
            verify_window_snapshot(&source, &before, header.len() as u64, bytes, header).is_err()
        );
    }
}
