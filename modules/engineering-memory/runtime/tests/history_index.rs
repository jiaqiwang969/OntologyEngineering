//! Track D: full-history conversation anchor catalog.
use engineering_memory_runtime::contracts::{
    HistoryIndexBuildRequest, HistoryIndexFile, HistoryIndexOversizedLine, HistoryIndexRoot,
    HistorySearchRequest, SourceAnchor,
};
use engineering_memory_runtime::evidence::{EvidenceArgs, read_window};
use engineering_memory_runtime::history_index::{build, search, status};
use engineering_memory_runtime::store::{contains_credential, sha};
use fs2::FileExt;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const BIG: usize = 17 * 1024 * 1024;

fn codex_meta(session: &str) -> String {
    json!({"timestamp":"2026-10-01T00:00:00Z","type":"session_meta",
        "payload":{"id":session,"cwd":"/synthetic"}})
    .to_string()
        + "\n"
}
fn codex_message(id: &str, role: &str, text: &str, at: &str) -> String {
    let kind = if role == "user" {
        "input_text"
    } else {
        "output_text"
    };
    json!({"timestamp":at,"type":"response_item","payload":{"type":"message","role":role,"id":id,
        "content":[{"type":kind,"text":text}]}})
    .to_string()
        + "\n"
}
fn codex_noise() -> String {
    [
        json!({"timestamp":"2026-10-01T00:00:01Z","type":"response_item","payload":{"type":"message",
            "role":"developer","content":[{"type":"input_text","text":"system rules"}]}}),
        json!({"timestamp":"2026-10-01T00:00:02Z","type":"event_msg","payload":{"type":"token_count"}}),
        json!({"timestamp":"2026-10-01T00:00:03Z","type":"response_item","payload":{"type":"reasoning",
            "summary":[{"type":"summary_text","text":"hidden"}]}}),
    ]
    .iter()
    .map(|v| v.to_string() + "\n")
    .collect()
}
fn claude_message(session: &str, id: &str, role: &str, content: Value, at: &str) -> String {
    json!({"type":role,"sessionId":session,"uuid":id,"parentUuid":null,"timestamp":at,
        "message":{"role":role,"content":content}})
    .to_string()
        + "\n"
}

struct Fixture {
    temp: TempDir,
    home: PathBuf,
    codex_root: PathBuf,
    claude_root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = TempDir::new().unwrap();
        let base = temp.path().canonicalize().unwrap();
        let codex_root = base.join("codex/sessions");
        let claude_root = base.join("claude/projects");
        fs::create_dir_all(codex_root.join("2026/10/01")).unwrap();
        fs::create_dir_all(claude_root.join("project-a")).unwrap();
        Self {
            home: base.join("memory"),
            temp,
            codex_root,
            claude_root,
        }
    }
    fn codex(&self, name: &str) -> PathBuf {
        self.codex_root
            .join("2026/10/01")
            .join(format!("rollout-2026-10-01T00-00-00-{name}.jsonl"))
    }
    fn claude(&self, name: &str) -> PathBuf {
        self.claude_root
            .join("project-a")
            .join(format!("{name}.jsonl"))
    }
    fn request(&self, max_files: u64) -> HistoryIndexBuildRequest {
        HistoryIndexBuildRequest {
            schema: "engineering-memory.history-index-build-request/1".into(),
            roots: vec![
                HistoryIndexRoot {
                    path: self.codex_root.to_str().unwrap().into(),
                    provider: "codex".into(),
                    profile: ".codex".into(),
                },
                HistoryIndexRoot {
                    path: self.claude_root.to_str().unwrap().into(),
                    provider: "claude".into(),
                    profile: "profile-a".into(),
                },
            ],
            max_files,
            max_seconds: 600,
            min_free_bytes: 0,
            verify: "changed".into(),
        }
    }
    fn build(&self) -> Value {
        build(&self.home, &self.request(1000)).unwrap()
    }
    fn index(&self) -> PathBuf {
        self.home.join("history-index")
    }
    fn manifest(&self, path: &Path) -> HistoryIndexFile {
        let key = sha(path.to_str().unwrap().as_bytes());
        serde_json::from_slice(
            &fs::read(self.index().join("files").join(format!("{key}.json"))).unwrap(),
        )
        .unwrap()
    }
    /// Committed catalog entries for one source.
    fn shard(&self, path: &Path) -> Vec<SourceAnchor> {
        let manifest = self.manifest(path);
        let key = sha(path.to_str().unwrap().as_bytes());
        let bytes =
            fs::read(self.index().join("shards").join(format!("{key}.jsonl"))).unwrap_or_default();
        bytes[..manifest.shard_bytes as usize]
            .split(|b| *b == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_slice(line).unwrap())
            .collect()
    }
}

fn append(path: &Path, text: &str) {
    OpenOptions::new()
        .append(true)
        .open(path)
        .unwrap()
        .write_all(text.as_bytes())
        .unwrap();
}

/// All anchors the evidence reader yields for complete lines, minus credential-bearing ones.
fn expected(path: &Path, provider: &str, profile: &str) -> Vec<SourceAnchor> {
    let bytes = fs::read(path).unwrap();
    let end = bytes.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1) as u64;
    let mut start = 0;
    let mut result = Vec::new();
    while start < end {
        let window = read_window(&EvidenceArgs {
            source: path.into(),
            provider: provider.into(),
            profile: Some(profile.into()),
            start_byte: start,
            end_byte: Some(end.min(start + 16 * 1024 * 1024)),
            max_record_bytes: 16 * 1024 * 1024,
            json: true,
        })
        .unwrap();
        result.extend(window.records);
        match window.next_byte {
            Some(next) if next > start => start = next,
            _ => break,
        }
    }
    result
        .into_iter()
        .filter(|anchor| !contains_credential(&anchor.text))
        .collect()
}

fn write_codex(f: &Fixture, name: &str, session: &str, lines: &[String]) -> PathBuf {
    let path = f.codex(name);
    fs::write(&path, codex_meta(session) + &lines.concat()).unwrap();
    path
}

#[test]
fn history_index_builds_codex_and_claude_anchors_equal_to_read_window() {
    let f = Fixture::new();
    let codex = write_codex(
        &f,
        "a",
        "codex-session-a",
        &[
            codex_noise(),
            codex_message(
                "u1",
                "user",
                "first question about the caliper reading",
                "2026-10-01T01:00:00Z",
            ),
            codex_message(
                "a1",
                "assistant",
                "游标卡尺的读数是 6.86 mm",
                "2026-10-01T01:00:05Z",
            ),
            codex_message(
                "u2",
                "user",
                "token sk-ABCDEFGHIJKLMNOPQRST leaked",
                "2026-10-01T01:01:00Z",
            ),
            codex_noise(),
        ],
    );
    let claude = f.claude("claude-a");
    fs::write(
        &claude,
        [
            claude_message(
                "claude-session-a",
                "c1",
                "user",
                json!("请检查蚁群的记忆回路"),
                "2026-10-02T00:00:00Z",
            ),
            claude_message(
                "claude-session-a",
                "c2",
                "assistant",
                json!([{"type":"thinking","thinking":"hidden"},{"type":"text","text":"日常整理已停转"}]),
                "2026-10-02T00:00:05Z",
            ),
            claude_message(
                "claude-session-a",
                "c3",
                "user",
                json!([{"type":"tool_result","tool_use_id":"t1","content":"tool output"}]),
                "2026-10-02T00:00:06Z",
            ),
        ]
        .concat(),
    )
    .unwrap();
    let receipt = f.build();
    assert_eq!(receipt["stop_reason"], "complete");
    assert_eq!(receipt["states"]["complete"], 2);
    assert_eq!(receipt["anchors_added"], 4);
    assert_eq!(receipt["credential_screened"], 1);
    assert_eq!(receipt["provider_calls"], 0);
    let codex_anchors = f.shard(&codex);
    assert_eq!(codex_anchors, expected(&codex, "codex", ".codex"));
    assert_eq!(codex_anchors.len(), 2);
    assert!(
        codex_anchors
            .iter()
            .all(|a| a.session_id == "codex-session-a")
    );
    let claude_anchors = f.shard(&claude);
    assert_eq!(claude_anchors, expected(&claude, "claude", "profile-a"));
    assert_eq!(claude_anchors.len(), 2);
    let state = status(&f.home).unwrap();
    assert_eq!(state["files"], 2);
    assert_eq!(state["complete_files"], 2);
    assert_eq!(state["anchors"], 4);
    assert_eq!(state["credential_screened"], 1);
    // A second build sees nothing new and reads no source bytes.
    let again = f.build();
    assert_eq!(again["unchanged_skipped"], 2);
    assert_eq!(again["processed_files"], 0);
    assert_eq!(again["bytes_indexed"], 0);
    // verify=all re-hashes every committed prefix without adding anchors.
    let mut audit = f.request(1000);
    audit.verify = "all".into();
    let audited = build(&f.home, &audit).unwrap();
    assert_eq!(audited["processed_files"], 2);
    assert_eq!(audited["anchors_added"], 0);
    assert_eq!(audited["superseded_total"], 0);
    assert_eq!(
        audited["prefix_verified_bytes"],
        fs::metadata(&codex).unwrap().len() + fs::metadata(&claude).unwrap().len()
    );
    assert_eq!(f.shard(&codex), expected(&codex, "codex", ".codex"));
}

#[test]
fn history_index_appends_scan_only_tail_and_rewrites_supersede() {
    let f = Fixture::new();
    let path = write_codex(
        &f,
        "grow",
        "codex-session-grow",
        &[codex_message(
            "u1",
            "user",
            "original question",
            "2026-10-01T01:00:00Z",
        )],
    );
    f.build();
    let first_size = fs::metadata(&path).unwrap().len();
    let addition = codex_message("a1", "assistant", "appended answer", "2026-10-01T01:00:05Z")
        + &codex_message("u2", "user", "appended follow up", "2026-10-01T01:00:09Z");
    append(&path, &addition);
    let receipt = f.build();
    assert_eq!(receipt["anchors_added"], 2);
    assert_eq!(receipt["bytes_indexed"], addition.len() as u64);
    assert_eq!(receipt["prefix_verified_bytes"], first_size);
    assert_eq!(f.shard(&path), expected(&path, "codex", ".codex"));

    // A line still being written waits until its newline arrives.
    let partial = codex_message("a2", "assistant", "late answer", "2026-10-01T01:01:00Z");
    let (head, tail) = partial.split_at(partial.len() / 2);
    append(&path, head);
    assert_eq!(f.build()["anchors_added"], 0);
    assert!(f.manifest(&path).complete);
    append(&path, tail);
    assert_eq!(f.build()["anchors_added"], 1);
    assert_eq!(f.shard(&path), expected(&path, "codex", ".codex"));

    // An interrupted commit leaves an uncommitted shard tail; it is cut, never mixed in.
    let key = sha(path.to_str().unwrap().as_bytes());
    append(
        &f.index().join("shards").join(format!("{key}.jsonl")),
        "{\"partial\":\n",
    );
    append(
        &path,
        &codex_message("u3", "user", "after interruption", "2026-10-01T01:02:00Z"),
    );
    let resumed = f.build();
    assert!(resumed["uncommitted_tail_discarded"].as_u64().unwrap() > 0);
    assert_eq!(f.shard(&path), expected(&path, "codex", ".codex"));

    // Rewriting earlier bytes supersedes the old shard and rebuilds from zero.
    let mut bytes = fs::read(&path).unwrap();
    let at = bytes.windows(8).position(|w| w == b"original").unwrap();
    bytes[at] = b'O';
    fs::write(&path, &bytes).unwrap();
    let rebuilt = f.build();
    assert_eq!(rebuilt["superseded_total"], 1);
    assert!(
        rebuilt["superseded"][0]["superseded"]
            .as_str()
            .unwrap()
            .contains("prefix_changed")
    );
    assert_eq!(f.shard(&path), expected(&path, "codex", ".codex"));
    assert!(f.shard(&path)[0].text.starts_with("Original"));
    let superseded: Vec<_> = fs::read_dir(f.index().join("superseded"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(superseded.len(), 1);
    assert!(superseded[0].join("shard.jsonl").is_file());
    assert!(superseded[0].join("file.json").is_file());

    // Truncation and replacement are detected the same way.
    let lines: Vec<_> = fs::read_to_string(&path)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    fs::write(&path, lines[..2].join("\n") + "\n").unwrap();
    let truncated = f.build();
    assert!(
        truncated["superseded"][0]["superseded"]
            .as_str()
            .unwrap()
            .contains("truncated")
    );
    assert_eq!(f.shard(&path), expected(&path, "codex", ".codex"));
    let replacement = path.with_extension("tmp");
    fs::write(&replacement, fs::read(&path).unwrap()).unwrap();
    fs::rename(&replacement, &path).unwrap();
    let replaced = f.build();
    assert!(
        replaced["superseded"][0]["superseded"]
            .as_str()
            .unwrap()
            .contains("replaced")
    );
    assert_eq!(f.shard(&path), expected(&path, "codex", ".codex"));
    assert_eq!(
        fs::read_dir(f.index().join("superseded")).unwrap().count(),
        3
    );
}

#[test]
fn history_index_skips_oversized_non_messages_and_blocks_messages() {
    let f = Fixture::new();
    let huge = "x".repeat(BIG);
    let event = json!({"timestamp":"2026-10-01T01:00:01Z","type":"event_msg",
        "payload":{"type":"item_completed","text":huge}})
    .to_string()
        + "\n";
    let skipped = write_codex(
        &f,
        "skip",
        "codex-session-skip",
        &[
            codex_message(
                "u1",
                "user",
                "before the huge event",
                "2026-10-01T01:00:00Z",
            ),
            event.clone(),
            "{not valid json ".to_string() + &huge + "\n",
            codex_message(
                "a1",
                "assistant",
                "after the huge event",
                "2026-10-01T01:00:09Z",
            ),
        ],
    );
    let blocked = write_codex(
        &f,
        "block",
        "codex-session-block",
        &[
            codex_message(
                "u1",
                "user",
                "kept before the block",
                "2026-10-01T02:00:00Z",
            ),
            codex_message("u2", "user", &huge, "2026-10-01T02:00:01Z"),
            codex_message(
                "a2",
                "assistant",
                "never indexed after block",
                "2026-10-01T02:00:02Z",
            ),
        ],
    );
    let claude = f.claude("claude-tool");
    fs::write(
        &claude,
        [
            claude_message(
                "claude-tool",
                "c1",
                "user",
                json!("question before tool output"),
                "2026-10-03T00:00:00Z",
            ),
            claude_message(
                "claude-tool",
                "c2",
                "user",
                json!([{"type":"tool_result","tool_use_id":"t","content":huge}]),
                "2026-10-03T00:00:01Z",
            ),
            claude_message(
                "claude-tool",
                "c3",
                "assistant",
                json!([{"type":"text","text":"answer after tool"}]),
                "2026-10-03T00:00:02Z",
            ),
        ]
        .concat(),
    )
    .unwrap();
    let receipt = f.build();
    assert_eq!(receipt["oversized_skipped"], 3);
    assert_eq!(receipt["states"]["blocked"], 1);
    let texts = |path: &Path| {
        f.shard(path)
            .into_iter()
            .map(|a| a.text)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        texts(&skipped),
        ["before the huge event", "after the huge event"]
    );
    assert_eq!(
        texts(&claude),
        ["question before tool output", "answer after tool"]
    );
    assert_eq!(texts(&blocked), ["kept before the block"]);
    assert!(f.manifest(&skipped).complete);
    let block = f.manifest(&blocked);
    let block_start = (codex_meta("codex-session-block").len()
        + codex_message(
            "u1",
            "user",
            "kept before the block",
            "2026-10-01T02:00:00Z",
        )
        .len()) as u64;
    assert_eq!(block.blocked_at, Some(block_start));
    assert_eq!(
        block.blocked_reason.as_deref(),
        Some("oversized_visible_message")
    );
    assert!(!block.complete);
    let receipts: Vec<HistoryIndexOversizedLine> = fs::read_dir(f.index().join("oversized"))
        .unwrap()
        .map(|e| serde_json::from_slice(&fs::read(e.unwrap().path()).unwrap()).unwrap())
        .collect();
    assert_eq!(receipts.len(), 3);
    let event_receipt = receipts
        .iter()
        .find(|r| r.record_type.as_deref() == Some("event_msg"))
        .unwrap();
    assert_eq!(event_receipt.reason, "not_visible_message");
    assert_eq!(
        event_receipt.byte_end - event_receipt.byte_start,
        event.len() as u64
    );
    assert_eq!(event_receipt.sha256, sha(event.as_bytes()));
    assert!(receipts.iter().any(|r| r.reason == "invalid_json"));
    assert!(receipts.iter().any(
        |r| r.body_role.as_deref() == Some("user") && r.source_path == claude.to_str().unwrap()
    ));
    let state = status(&f.home).unwrap();
    assert_eq!(state["blocked_files"], 1);
    assert_eq!(state["blocked"][0]["reason"], "oversized_visible_message");
    // Unchanged blocked files are not re-read.
    let again = f.build();
    assert_eq!(again["blocked_unchanged"], 1);
    assert_eq!(again["processed_files"], 0);
}

#[test]
fn history_index_build_is_bounded_resumable_and_exclusive() {
    let f = Fixture::new();
    let paths: Vec<_> = ["one", "two", "three"]
        .iter()
        .map(|name| {
            write_codex(
                &f,
                name,
                &format!("codex-session-{name}"),
                &[codex_message(
                    "u",
                    "user",
                    &format!("question {name}"),
                    "2026-10-01T01:00:00Z",
                )],
            )
        })
        .collect();
    let first = build(&f.home, &f.request(1)).unwrap();
    assert_eq!(first["stop_reason"], "max_files");
    assert_eq!(first["processed_files"], 1);
    assert_eq!(first["pending_files"], 2);
    assert_eq!(build(&f.home, &f.request(1)).unwrap()["pending_files"], 1);
    let last = build(&f.home, &f.request(1)).unwrap();
    assert_eq!(last["stop_reason"], "complete");
    assert_eq!(last["pending_files"], 0);
    for path in &paths {
        assert_eq!(f.shard(path), expected(path, "codex", ".codex"));
    }
    assert_eq!(
        status(&f.home).unwrap()["last_build"]["build_id"],
        last["build_id"]
    );

    let lock = fs::File::open(f.index().join("build.lock")).unwrap();
    lock.try_lock_exclusive().unwrap();
    let busy = build(&f.home, &f.request(10)).unwrap_err();
    assert!(format!("{busy:#}").contains("history_index_build_busy"));
    FileExt::unlock(&lock).unwrap();

    // Not enough free space: nothing is indexed and no shard is written.
    let g = Fixture::new();
    write_codex(
        &g,
        "space",
        "codex-space",
        &[codex_message("u", "user", "q", "2026-10-01T01:00:00Z")],
    );
    let mut request = g.request(10);
    // The largest value the contract admits (2^53 - 1) exceeds any real free space.
    request.min_free_bytes = 9_007_199_254_740_991;
    let starved = build(&g.home, &request).unwrap();
    request.min_free_bytes = u64::MAX;
    assert!(build(&g.home, &request).is_err());
    assert_eq!(starved["stop_reason"], "min_free_bytes");
    assert_eq!(starved["anchors_added"], 0);
    assert!(!g.index().join("shards").exists());
    assert!(!g.index().join("files").exists());

    // Links out of a root and credential-like paths are never followed or read.
    let h = Fixture::new();
    let outside = h.temp.path().join("outside-rollout-x.jsonl");
    fs::write(
        &outside,
        codex_meta("outside") + &codex_message("u", "user", "outside", "2026-10-01T01:00:00Z"),
    )
    .unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, h.codex("linked")).unwrap();
    fs::create_dir_all(h.codex_root.join("secrets")).unwrap();
    fs::write(
        h.codex_root.join("secrets").join("rollout-hidden.jsonl"),
        codex_meta("hidden") + &codex_message("u", "user", "hidden", "2026-10-01T01:00:00Z"),
    )
    .unwrap();
    let screened = build(&h.home, &h.request(10)).unwrap();
    assert_eq!(screened["discovered_files"], 0);
    assert!(screened["discovery"]["symlinks_skipped"].as_u64().unwrap() >= 1);
    assert!(
        screened["discovery"]["credential_paths_skipped"]
            .as_u64()
            .unwrap()
            >= 1
    );

    // Requests are validated before any work.
    assert!(build(&f.home, &f.request(0)).is_err());
    let mut relative = f.request(1);
    relative.roots[0].path = "relative/path".into();
    assert!(build(&f.home, &relative).is_err());
    let mut duplicate = f.request(1);
    duplicate.roots[1] = duplicate.roots[0].clone();
    assert!(build(&f.home, &duplicate).is_err());
}

fn query(text: &str, limit: u64) -> HistorySearchRequest {
    HistorySearchRequest {
        schema: "engineering-memory.history-search-request/1".into(),
        query: text.into(),
        limit,
        provider: None,
        profile: None,
        session_id: None,
        since: None,
        until: None,
    }
}

type Row = (PathBuf, u64, std::time::SystemTime, Vec<u8>);

fn tree(root: &Path) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let meta = fs::symlink_metadata(&path).unwrap();
        let bytes = if meta.is_dir() {
            for child in fs::read_dir(&path).unwrap() {
                stack.push(child.unwrap().path());
            }
            Vec::new()
        } else {
            fs::read(&path).unwrap()
        };
        rows.push((path, meta.len(), meta.modified().unwrap(), bytes));
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    rows
}

fn hits(value: &Value) -> &Vec<Value> {
    value["hits"].as_array().unwrap()
}

#[test]
fn history_search_ranks_filters_and_is_read_only() {
    let f = Fixture::new();
    let codex = write_codex(
        &f,
        "search",
        "codex-session-s",
        &[
            codex_message("u1", "user", "请读取游标卡尺的读数", "2026-10-01T01:00:00Z"),
            codex_message(
                "a1",
                "assistant",
                "游标卡尺读数 6.86 mm，主尺 6 mm",
                "2026-10-01T01:00:05Z",
            ),
            codex_message(
                "u2",
                "user",
                "unrelated deployment question",
                "2026-10-05T00:00:00Z",
            ),
        ],
    );
    let claude = f.claude("claude-search");
    fs::write(
        &claude,
        [
            claude_message(
                "claude-s",
                "c1",
                "user",
                json!("游标卡尺 的 读数 怎么看"),
                "2026-10-03T00:00:00Z",
            ),
            claude_message(
                "claude-s",
                "c2",
                "assistant",
                json!([{"type":"text","text":"Caliper reading uses the vernier scale"}]),
                "2026-10-03T00:00:05Z",
            ),
        ]
        .concat(),
    )
    .unwrap();
    f.build();
    let before = tree(&f.home);
    let found = search(&f.home, &query("游标卡尺 读数", 5)).unwrap();
    assert_eq!(hits(&found).len(), 3);
    // Ranked by (coverage, score), both reported for every hit.
    let ranks: Vec<(u64, u64)> = hits(&found)
        .iter()
        .zip(found["hit_notes"].as_array().unwrap())
        .map(|(h, n)| {
            (
                n["coverage"].as_u64().unwrap(),
                h["score"].as_u64().unwrap(),
            )
        })
        .collect();
    assert!(ranks.windows(2).all(|w| w[0] >= w[1]));
    assert_eq!(found["unavailable_shards"], 0);
    assert!(
        hits(&found)
            .iter()
            .all(|h| h["anchor"]["text"].as_str().unwrap().contains("游标卡尺"))
    );
    assert_eq!(found["verification"], "call read_window before citing");
    // Every hit is exactly what the evidence reader returns for its range.
    for hit in hits(&found) {
        let anchor: SourceAnchor = serde_json::from_value(hit["anchor"].clone()).unwrap();
        let window = read_window(&EvidenceArgs {
            source: anchor.source_path.clone().into(),
            provider: anchor.provider.clone(),
            profile: anchor.profile.clone(),
            start_byte: anchor.byte_start,
            end_byte: Some(anchor.byte_end),
            max_record_bytes: 16 * 1024 * 1024,
            json: true,
        })
        .unwrap();
        assert_eq!(window.records, vec![anchor]);
    }
    assert_eq!(
        hits(&search(&f.home, &query("游标卡尺 读数", 1)).unwrap()).len(),
        1
    );
    assert_eq!(
        search(&f.home, &query("vernier", 5)).unwrap()["hits"][0]["anchor"]["source_path"],
        claude.to_str().unwrap()
    );
    let mut by_provider = query("游标卡尺", 10);
    by_provider.provider = Some("claude".into());
    let claude_hits = search(&f.home, &by_provider).unwrap();
    assert!(
        hits(&claude_hits)
            .iter()
            .all(|h| h["anchor"]["provider"] == "claude")
    );
    assert_eq!(hits(&claude_hits).len(), 1);
    let mut by_session = query("游标卡尺", 10);
    by_session.session_id = Some("codex-session-s".into());
    assert_eq!(hits(&search(&f.home, &by_session).unwrap()).len(), 2);
    let mut by_profile = query("游标卡尺", 10);
    by_profile.profile = Some("profile-a".into());
    assert_eq!(hits(&search(&f.home, &by_profile).unwrap()).len(), 1);
    let mut by_time = query("游标卡尺 deployment", 10);
    by_time.since = Some("2026-10-02T00:00:00Z".into());
    by_time.until = Some("2026-10-04T00:00:00Z".into());
    let timed = search(&f.home, &by_time).unwrap();
    assert_eq!(hits(&timed).len(), 1);
    assert_eq!(timed["hits"][0]["anchor"]["session_id"], "claude-s");
    assert_eq!(f.shard(&codex).len(), 3);

    for bad in [query("   ", 5), query("x", 0), query("x", 51)] {
        assert!(search(&f.home, &bad).is_err());
    }
    let mut bad_provider = query("x", 5);
    bad_provider.provider = Some("other".into());
    assert!(search(&f.home, &bad_provider).is_err());
    let mut reversed = query("x", 5);
    reversed.since = Some("2026-10-05T00:00:00Z".into());
    reversed.until = Some("2026-10-01T00:00:00Z".into());
    assert!(search(&f.home, &reversed).is_err());
    let mut unparsable = query("x", 5);
    unparsable.since = Some("yesterday".into());
    assert!(search(&f.home, &unparsable).is_err());
    status(&f.home).unwrap();
    assert_eq!(before, tree(&f.home), "status and search must not write");

    // A home without a catalog stays untouched.
    let empty = Fixture::new();
    assert_eq!(status(&empty.home).unwrap()["exists"], false);
    assert_eq!(
        search(&empty.home, &query("anything", 5)).unwrap()["hits"],
        json!([])
    );
    assert!(!empty.home.exists());
}

#[test]
fn tick_refreshes_catalog_only_when_requested_and_never_fails_for_it() {
    use engineering_memory_runtime::contracts::{SyncBudget, TickPolicy};
    use engineering_memory_runtime::store::Store;
    let f = Fixture::new();
    let path = write_codex(
        &f,
        "tick",
        "session-tick",
        &[codex_message(
            "m1",
            "user",
            "蚁群 召回 第一条",
            "2026-10-01T00:00:05Z",
        )],
    );
    let store = Store::new(f.home.clone());
    let policy = TickPolicy {
        schema: "engineering-memory.tick-policy/1".into(),
        sync: SyncBudget {
            max_bytes: 2 * 1024 * 1024,
            max_verify_bytes: 16 * 1024 * 1024,
            max_windows: 8,
            max_seconds: 10,
        },
        max_hook_sources: 8,
        max_registration_bytes: 16 * 1024 * 1024,
        max_pending_packets: 1,
        max_curation_packets: 2,
    };
    let first = store.tick(&policy).unwrap();
    assert_eq!(first["status"], "ran");
    assert_eq!(first["result"]["history_index"]["status"], "not_configured");
    assert!(!f.index().join("files").exists());

    // The initial build is explicit; tick then only refreshes appended tails.
    f.build();
    assert_eq!(f.shard(&path).len(), 1);
    append(
        &path,
        &codex_message(
            "m2",
            "assistant",
            "蚁群 召回 第二条",
            "2026-10-01T00:00:06Z",
        ),
    );
    let mut request = f.request(100_000);
    request.max_seconds = 86_400;
    fs::write(
        f.index().join("tick-request.json"),
        serde_json::to_vec(&request).unwrap(),
    )
    .unwrap();
    let second = store.tick(&policy).unwrap();
    assert_eq!(second["status"], "ran");
    let history = &second["result"]["history_index"];
    assert_eq!(history["status"], "ran", "{history}");
    assert_eq!(history["stop_reason"], "complete");
    assert_eq!(history["processed_files"], 1);
    assert_eq!(f.shard(&path).len(), 2);

    // A broken request is reported inside the tick result; the tick still runs.
    let mut broken = f.request(10);
    broken.roots[0].path = f.temp.path().join("missing").to_str().unwrap().into();
    fs::write(
        f.index().join("tick-request.json"),
        serde_json::to_vec(&broken).unwrap(),
    )
    .unwrap();
    let third = store.tick(&policy).unwrap();
    assert_eq!(third["status"], "ran");
    assert_eq!(third["result"]["history_index"]["status"], "failed");
    assert_eq!(f.shard(&path).len(), 2);
}

#[test]
fn history_search_prefers_coverage_collapses_copies_and_labels_host_rows() {
    let f = Fixture::new();
    // A long answer holding every query term, against a short message holding
    // two: density alone would rank the short one first.
    let filler: String = (0..150).map(|i| format!(" 词{i}")).collect();
    let long = format!("卡尺 图5 采样带：两侧采样带从 35 扩到 50 像素后六组 ready。{filler}");
    let agents = "# AGENTS.md instructions for /work\n\n<INSTRUCTIONS>\n卡尺 图5 采样带 的说明\n</INSTRUCTIONS>";
    let original = write_codex(
        &f,
        "search",
        "codex-original",
        &[
            codex_message("u1", "user", agents, "2026-10-01T01:00:00Z"),
            codex_message("u2", "user", "5 分钟采样。", "2026-10-01T01:00:01Z"),
            codex_message("a1", "assistant", &long, "2026-10-01T01:00:02Z"),
        ],
    );
    // A resumed session replays the same answer.
    let copy = write_codex(
        &f,
        "search-copy",
        "codex-copy",
        &[codex_message(
            "a1",
            "assistant",
            &long,
            "2026-10-02T01:00:02Z",
        )],
    );
    // A session without visible messages has no shard.
    let empty = write_codex(&f, "empty", "codex-empty", &[]);
    let claude = f.claude("claude-notice");
    let mut notice = serde_json::from_str::<Value>(&claude_message(
        "claude-n",
        "n1",
        "user",
        json!(
            "<task-notification>\n<summary>卡尺 图5 采样带 已完成</summary>\n</task-notification>"
        ),
        "2026-10-03T00:00:00Z",
    ))
    .unwrap();
    notice["origin"] = json!({"kind":"task-notification"});
    notice["promptSource"] = json!("system");
    fs::write(&claude, format!("{notice}\n")).unwrap();
    f.build();
    assert_eq!(f.manifest(&empty).shard_bytes, 0);
    let before = tree(&f.home);

    let found = search(&f.home, &query("卡尺 图5 采样带", 10)).unwrap();
    let rows: Vec<(&str, u64, u64, u64, Value)> = hits(&found)
        .iter()
        .zip(found["hit_notes"].as_array().unwrap())
        .map(|(h, n)| {
            (
                h["anchor"]["text"].as_str().unwrap(),
                n["coverage"].as_u64().unwrap(),
                h["score"].as_u64().unwrap(),
                n["same_text_anchors"].as_u64().unwrap(),
                n["host_injected"].clone(),
            )
        })
        .collect();
    assert_eq!(found["query_terms"], 6);
    assert_eq!(rows.len(), 4, "{rows:#?}");
    // Every text appears once; the replayed answer is one hit with two anchors.
    let texts: BTreeSet<&str> = rows.iter().map(|r| r.0).collect();
    assert_eq!(texts.len(), rows.len());
    let answer = rows.iter().position(|r| r.0 == long).unwrap();
    assert_eq!(rows[answer].1, 6);
    assert_eq!(rows[answer].3, 2);
    assert_eq!(found["collapsed_copies"], 1);
    let earliest = original.to_str().unwrap().min(copy.to_str().unwrap());
    assert_eq!(hits(&found)[answer]["anchor"]["source_path"], earliest);
    // Coverage outranks density.
    let short = rows.iter().position(|r| r.0 == "5 分钟采样。").unwrap();
    assert!(answer < short);
    assert!(rows[answer].2 < rows[short].2);
    assert_eq!(rows.last().unwrap().0, "5 分钟采样。");
    // Host-made rows are labeled, not hidden.
    let label = |needle: &str| {
        rows.iter()
            .find(|r| r.0.contains(needle))
            .map(|r| r.4.clone())
            .unwrap()
    };
    assert_eq!(label("AGENTS.md"), json!("codex_agents_md_instructions"));
    assert_eq!(
        label("task-notification"),
        json!("claude_task_notification")
    );
    assert_eq!(rows[answer].4, Value::Null);
    assert!(
        found["hit_notes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|n| n["origin_checked"] == true)
    );
    assert_eq!(found["empty_files"], 1);
    assert_eq!(found["unavailable_shards"], 0);

    // Deterministic, and the top hit survives a limit of one.
    let mut again = search(&f.home, &query("卡尺 图5 采样带", 10)).unwrap();
    let mut first = found.clone();
    first["elapsed_ms"] = json!(0);
    again["elapsed_ms"] = json!(0);
    assert_eq!(first, again);
    let one = search(&f.home, &query("卡尺 图5 采样带", 1)).unwrap();
    assert_eq!(hits(&one).len(), 1);
    assert_eq!(hits(&one)[0], hits(&found)[0]);
    assert_eq!(one["hit_notes"][0], found["hit_notes"][0]);
    assert_eq!(before, tree(&f.home), "search must not write");
}
