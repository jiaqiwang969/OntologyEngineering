//! Native transcript origin classification; no model dispatch.
use crate::contracts::SourceAnchor;
use crate::store::sha;
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::Value;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
/// Blocks that Codex injects into user-role messages and that are not the
/// person's words: (opening text, closing tag, kind). The goal re-send
/// (`<codex_internal_context source="goal">`, whose objective the person wrote),
/// shell-command echoes, question replies and images stay conversation.
const CODEX_BLOCKS: [(&str, &str, &str); 6] = [
    (
        "# AGENTS.md instructions",
        "</INSTRUCTIONS>",
        "codex_agents_md_instructions",
    ),
    (
        "<environment_context>",
        "</environment_context>",
        "codex_environment_context",
    ),
    (
        "<recommended_plugins>",
        "</recommended_plugins>",
        "codex_recommended_plugins",
    ),
    ("<turn_aborted>", "</turn_aborted>", "codex_turn_aborted"),
    (
        "<subagent_notification>",
        "</subagent_notification>",
        "codex_subagent_notification",
    ),
    ("<skill>", "</skill>", "codex_skill"),
];

/// One leading Codex host block and the text after it. Only the first closing
/// tag counts, so person-written text can never sit inside a matched block.
fn codex_block(text: &str) -> Option<(&'static str, &str)> {
    let (open, close, kind) = CODEX_BLOCKS
        .iter()
        .find(|(open, _, _)| text.starts_with(open))?;
    let body = &text[open.len()..];
    if *kind == "codex_agents_md_instructions" {
        // A one-line header such as "for /path", then the instructions block.
        let (header, _) = body.split_once("<INSTRUCTIONS>")?;
        if header.trim().contains('\n') {
            return None;
        }
    }
    let end = body.find(close)? + close.len();
    Some((kind, body[end..].trim_start()))
}

/// Whole user-role messages that Codex injects as host material: AGENTS.md
/// instructions, environment context, plugin lists, aborted-turn notices,
/// subagent notifications and skill bodies, alone or one after another. They
/// are not conversation. Any other text in the message keeps it conversation.
/// Returns the kind of the first block.
pub fn host_configuration(source: &SourceAnchor) -> Option<&'static str> {
    if source.provider != "codex" || source.role != "user" {
        return None;
    }
    let mut text = source.text.trim();
    let mut first = None;
    while !text.is_empty() {
        let (kind, rest) = codex_block(text)?;
        first.get_or_insert(kind);
        text = rest;
    }
    first
}

/// Claude rows with a user role that the person did not write, recognized by
/// their leading text when the row carries no deciding flag.
/// Slash commands the person typed (`<command-name>`) stay conversation.
const CLAUDE_WRAPPERS: [(&str, &str); 10] = [
    (
        "This session is being continued from a previous conversation",
        "claude_continuation_summary",
    ),
    ("<task-notification>", "claude_task_notification"),
    ("<local-command-stdout>", "claude_local_command_output"),
    ("<local-command-stderr>", "claude_local_command_output"),
    ("<local-command-caveat>", "claude_local_command_caveat"),
    ("<system-reminder>", "claude_system_reminder"),
    ("<fork-boilerplate>", "claude_fork_directive"),
    ("[Request interrupted by user", "claude_interruption_marker"),
    ("Stop hook feedback:", "claude_hook_feedback"),
    ("Base directory for this skill:", "claude_skill_body"),
];

/// How a user-role message came to be in its transcript.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MessageOrigin {
    /// Host material rather than conversation, with its kind. Automatic
    /// drafts leave it out as an ignored source.
    pub injected: Option<&'static str>,
    /// Positively known to be typed by the person, starting with their own
    /// words. Authorization anchors require this.
    pub typed: bool,
}

/// Starts with the person's own words, not a host wrapper, marker or heading.
fn plain(text: &str) -> bool {
    let text = text.trim_start();
    !text.is_empty() && !text.starts_with(['<', '[', '#'])
}

/// The exact anchored transcript line, re-hashed against the anchor.
fn anchored_line(source: &SourceAnchor) -> Result<Vec<u8>> {
    let length = source
        .byte_end
        .checked_sub(source.byte_start)
        .filter(|n| (1..=crate::store::MAX_BYTES).contains(n))
        .context("source_anchor_range")?;
    let mut file = File::open(&source.source_path).context("source_anchor_unavailable")?;
    file.seek(SeekFrom::Start(source.byte_start))?;
    let mut line = vec![0; usize::try_from(length)?];
    file.read_exact(&mut line)
        .context("source_anchor_changed")?;
    ensure!(sha(&line) == source.sha256, "source_anchor_changed");
    Ok(line)
}

/// Claude row fields that say who produced a user-role row. The message body
/// is skipped; unexpected value types read as absent.
#[derive(Deserialize)]
struct ClaudeRowHead {
    #[serde(rename = "isMeta", default)]
    is_meta: Option<Value>,
    #[serde(rename = "isCompactSummary", default)]
    compact_summary: Option<Value>,
    #[serde(rename = "isSidechain", default)]
    sidechain: Option<Value>,
    #[serde(default)]
    origin: Option<Value>,
    #[serde(rename = "promptSource", default)]
    prompt_source: Option<Value>,
}

/// Claude marks typed prompts with `origin.kind = "human"` and
/// `promptSource = "typed"`; summaries, meta rows (skill bodies, caveats,
/// peer hand-backs), task notifications and sidechain prompts carry their own
/// marks. Older rows without marks fall back to their leading text.
fn claude_origin(source: &SourceAnchor) -> Result<MessageOrigin> {
    let head: ClaudeRowHead =
        serde_json::from_slice(&anchored_line(source)?).context("source_anchor_row_invalid")?;
    let flag = |value: &Option<Value>| value.as_ref().and_then(Value::as_bool) == Some(true);
    let origin = head
        .origin
        .as_ref()
        .and_then(|origin| origin.get("kind"))
        .and_then(Value::as_str);
    let text = source.text.trim_start();
    let injected = if flag(&head.compact_summary) {
        Some("claude_continuation_summary")
    } else {
        match origin {
            Some("task-notification") => Some("claude_task_notification"),
            Some("peer") => Some("claude_peer_message"),
            Some(kind) if kind != "human" => Some("claude_non_human_origin"),
            _ if flag(&head.is_meta) => Some("claude_meta_message"),
            _ if flag(&head.sidechain) => Some("claude_sidechain_prompt"),
            _ => CLAUDE_WRAPPERS
                .iter()
                .find(|(prefix, _)| text.starts_with(prefix))
                .map(|(_, kind)| *kind),
        }
    };
    let typed = injected.is_none()
        && origin == Some("human")
        && head.prompt_source.as_ref().and_then(Value::as_str) == Some("typed")
        && plain(text);
    Ok(MessageOrigin { injected, typed })
}

/// Classify one source anchor. Codex messages are judged by their text;
/// Claude user rows by the anchored row itself, re-hashed first.
pub fn message_origin(source: &SourceAnchor) -> Result<MessageOrigin> {
    if source.role != "user" {
        return Ok(MessageOrigin {
            injected: None,
            typed: false,
        });
    }
    match source.provider.as_str() {
        "codex" => {
            let injected = host_configuration(source);
            Ok(MessageOrigin {
                injected,
                typed: injected.is_none() && plain(&source.text),
            })
        }
        "claude" => claude_origin(source),
        _ => Ok(MessageOrigin {
            injected: None,
            typed: false,
        }),
    }
}

pub(crate) fn credential_path(path: &Path) -> bool {
    path.components().any(|part| {
        let name = part.as_os_str().to_string_lossy().to_ascii_lowercase();
        matches!(
            name.as_str(),
            "auth.json"
                | "credentials.json"
                | "credentials"
                | "secrets"
                | ".env"
                | "id_rsa"
                | "id_ed25519"
                | "id_ecdsa"
        ) || name.starts_with(".env.")
            || name.ends_with(".pem")
            || name.ends_with(".key")
            || name.starts_with("credentials.")
            || name.starts_with("secret.")
    })
}
