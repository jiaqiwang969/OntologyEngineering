//! Progress tracking (`semantica/utils/progress_tracker.py`): a process-wide tracker that
//! records start/update/stop of work items and mirrors them to displays (console, Jupyter,
//! log file), with pipeline grouping.
//!
//! The tracker is shared: obtain it with [`get_progress_tracker`] and call its `&self`
//! methods from any thread.
//!
//! ```
//! use sem_utils::progress_tracker::{ProgressDisplay, ProgressTracker};
//! use std::sync::Arc;
//!
//! let tracker = ProgressTracker::new(true, false, 0.0);
//! tracker.set_displays(Vec::new()); // keep the doctest quiet
//! let id = tracker.start_tracking(Some("doc.pdf"), Some("ingest"), Some("FileIngestor"), "Ingesting", None);
//! assert_eq!(id, "ingest:FileIngestor:doc.pdf");
//! tracker.update_progress(&id, 5, 10, "");
//! tracker.stop_tracking(&id, None, "", None);
//! assert_eq!(tracker.items()[0].status, "completed");
//! ```

use std::any::Any;
use std::fmt;
use std::io::Write;
use std::panic::Location;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use indexmap::{IndexMap, IndexSet};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::logging::{Logger, get_logger};
use crate::pycompat::fmt::{format_fixed, path_name, py_str, strip};

/// Whether an IPython kernel is available. Rust programs never run inside one, so the Jupyter
/// display keeps its items but renders nothing.
pub const IPYTHON_AVAILABLE: bool = false;

fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .map(|v| {
            matches!(
                strip(&v).to_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

/// `SEMANTICA_DISABLE_JUPYTER_PROGRESS`, read once (at first use, as Python reads it at import).
pub static DISABLE_JUPYTER_PROGRESS: Lazy<bool> =
    Lazy::new(|| env_flag("SEMANTICA_DISABLE_JUPYTER_PROGRESS"));

/// Whether `SEMANTICA_DISABLE_PROGRESS` disables progress output for this process.
fn progress_disabled_from_env() -> bool {
    env_flag("SEMANTICA_DISABLE_PROGRESS")
}

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// `ProgressItem` dataclass: one tracked unit of work. (Deserializing fills missing fields
/// with the dataclass defaults.)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProgressItem {
    /// File being processed.
    pub file: Option<String>,
    /// Module name (`ingest`, `parse`, ...).
    pub module: Option<String>,
    /// Submodule (usually a class name).
    pub submodule: Option<String>,
    /// `pending`, `running`, `completed` or `failed`.
    pub status: String,
    /// Start time (seconds since the epoch).
    pub start_time: Option<f64>,
    /// End time (seconds since the epoch).
    pub end_time: Option<f64>,
    /// Progress message.
    pub message: String,
    /// Display emoji.
    pub emoji: String,
    /// Free-form metadata (`extraction_counts`, `core_dependency`, ...).
    pub metadata: Map<String, Value>,
    /// Progress percentage (0-100).
    pub progress_percentage: Option<f64>,
    /// Total items to process.
    pub total_items: Option<i64>,
    /// Items processed so far.
    pub processed_items: Option<i64>,
    /// Estimated remaining time in seconds.
    pub estimated_remaining: Option<f64>,
    /// Pipeline this item belongs to.
    pub pipeline_id: Option<String>,
    /// Order of the item's module in its pipeline.
    pub pipeline_order: Option<i64>,
}

impl Default for ProgressItem {
    fn default() -> Self {
        ProgressItem {
            file: None,
            module: None,
            submodule: None,
            status: "pending".to_string(),
            start_time: None,
            end_time: None,
            message: String::new(),
            emoji: "\u{23f3}".to_string(),
            metadata: Map::new(),
            progress_percentage: None,
            total_items: None,
            processed_items: None,
            estimated_remaining: None,
            pipeline_id: None,
            pipeline_order: None,
        }
    }
}

/// `ProgressDisplay` (abstract base class): where progress is shown.
pub trait ProgressDisplay: Any + Send + Sync {
    /// Update the display with an item's new state.
    fn update(&self, item: &ProgressItem);
    /// Show the final summary of all items.
    fn show_summary(&self, items: &[ProgressItem]);
    /// Clear the display.
    fn clear(&self);
}

const MODULE_EMOJIS: &[(&str, &str)] = &[
    ("ingest", "\u{1f4e5}"),
    ("parse", "\u{1f50d}"),
    ("kg", "\u{1f9e0}"),
    ("embeddings", "\u{1f4be}"),
    ("normalize", "\u{1f527}"),
    ("ontology", "\u{1f4da}"),
    ("semantic_extract", "\u{1f3af}"),
    ("seed", "\u{1f331}"),
    ("split", "\u{2702}\u{fe0f}"),
    ("triplet_store", "\u{1f5c4}\u{fe0f}"),
    ("vector_store", "\u{1f4ca}"),
    ("export", "\u{1f4be}"),
    ("reasoning", "\u{1f914}"),
    ("kg_qa", "\u{2705}"),
    ("visualization", "\u{1f4c8}"),
    ("context", "\u{1f517}"),
    ("conflicts", "\u{26a0}\u{fe0f}"),
    ("deduplication", "\u{1f504}"),
    ("pipeline", "\u{2699}\u{fe0f}"),
];

const ACTIONS: &[(&str, &str)] = &[
    ("ingest", "is ingesting"),
    ("parse", "is parsing"),
    ("kg", "is building"),
    ("embeddings", "is embedding"),
    ("normalize", "is normalizing"),
    ("ontology", "is generating"),
    ("semantic_extract", "is extracting"),
    ("seed", "is seeding"),
    ("split", "is splitting"),
    ("triplet_store", "is storing"),
    ("vector_store", "is indexing"),
    ("export", "is exporting"),
    ("reasoning", "is reasoning"),
    ("kg_qa", "is validating"),
    ("visualization", "is visualizing"),
    ("context", "is processing"),
    ("conflicts", "is resolving"),
    ("deduplication", "is deduplicating"),
    ("pipeline", "is executing"),
    ("core", "is processing"),
];

const HOURGLASS: &str = "\u{23f3}";
const BRAIN: &str = "\u{1f9e0}";
const CHART: &str = "\u{1f4ca}";
const CHECK: &str = "\u{2705}";
const CROSS: &str = "\u{274c}";
const STOPWATCH: &str = "\u{23f1}\u{fe0f}";

/// The emoji whose key the (lowercased) module name contains, else an hourglass.
fn module_emoji(module: &str) -> &'static str {
    let lower = module.to_lowercase();
    MODULE_EMOJIS
        .iter()
        .find(|(key, _)| lower.contains(key))
        .map(|(_, emoji)| *emoji)
        .unwrap_or(HOURGLASS)
}

fn status_emoji(status: &str) -> &'static str {
    match status {
        "running" => "\u{1f504}",
        "completed" => CHECK,
        "failed" => CROSS,
        _ => HOURGLASS,
    }
}

fn action_for(module: Option<&str>) -> &'static str {
    let module = module.unwrap_or("");
    ACTIONS
        .iter()
        .find(|(key, _)| *key == module)
        .map(|(_, action)| *action)
        .unwrap_or("is processing")
}

fn non_empty(value: &Option<String>) -> Option<&str> {
    value.as_deref().filter(|s| !s.is_empty())
}

fn or_na(value: &Option<String>) -> &str {
    non_empty(value).unwrap_or("N/A")
}

fn truthy_time(value: Option<f64>) -> Option<f64> {
    value.filter(|t| *t != 0.0)
}

fn eta_text(remaining: Option<f64>) -> Option<String> {
    let remaining = remaining.filter(|r| *r > 0.0)?;
    Some(if remaining < 60.0 {
        format!("{}s", format_fixed(remaining, 1))
    } else if remaining < 3600.0 {
        format!("{}m", format_fixed(remaining / 60.0, 1))
    } else {
        format!("{}h", format_fixed(remaining / 3600.0, 1))
    })
}

/// `"12 tables, 3 images, 40 pages"` from an item's `extraction_counts`, if any are present.
fn extraction_text(item: &ProgressItem) -> Option<String> {
    let Some(Value::Object(counts)) = item.metadata.get("extraction_counts") else {
        return None;
    };
    let mut parts = Vec::new();
    for (key, label) in [
        ("tables", "tables"),
        ("images", "images"),
        ("pages", "pages"),
    ] {
        if let Some(count) = counts.get(key) {
            parts.push(format!("{} {label}", py_str(count)));
        }
    }
    (!parts.is_empty()).then(|| parts.join(", "))
}

fn has_extraction_counts(item: &ProgressItem) -> bool {
    item.metadata
        .get("extraction_counts")
        .map(crate::pycompat::fmt::truthy)
        .unwrap_or(false)
}

fn is_docling(item: &ProgressItem) -> bool {
    matches!(item.metadata.get("core_dependency"), Some(Value::String(s)) if s == "docling")
}

fn sort_pipeline_items(items: &mut [ProgressItem]) {
    items.sort_by(|a, b| {
        let key = |i: &ProgressItem| {
            (
                i.pipeline_order.unwrap_or(999),
                i.module.clone().unwrap_or_default(),
            )
        };
        key(a).cmp(&key(b))
    });
}

fn group_by_module(items: &[ProgressItem]) -> IndexMap<String, Vec<&ProgressItem>> {
    let mut by_module: IndexMap<String, Vec<&ProgressItem>> = IndexMap::new();
    for item in items {
        let module = non_empty(&item.module).unwrap_or("unknown").to_string();
        by_module.entry(module).or_default().push(item);
    }
    by_module
}

enum ConsoleOutput {
    Stdout,
    Writer(Box<dyn Write + Send>),
}

struct ConsoleState {
    current_lines: IndexMap<String, String>,
    pipeline_header_shown: bool,
    output: ConsoleOutput,
}

impl ConsoleState {
    fn write(&mut self, text: &str) {
        match &mut self.output {
            ConsoleOutput::Stdout => print!("{text}"),
            ConsoleOutput::Writer(writer) => {
                let _ = writer.write_all(text.as_bytes());
            }
        }
    }

    fn flush(&mut self) {
        match &mut self.output {
            ConsoleOutput::Stdout => {
                let _ = std::io::stdout().flush();
            }
            ConsoleOutput::Writer(writer) => {
                let _ = writer.flush();
            }
        }
    }
}

/// `ConsoleProgressDisplay(use_emoji=True, update_interval=0.1)`: real-time lines on stdout.
pub struct ConsoleProgressDisplay {
    use_emoji: bool,
    update_interval: f64,
    last_update: Mutex<f64>,
    state: Mutex<ConsoleState>,
}

impl fmt::Debug for ConsoleProgressDisplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConsoleProgressDisplay")
            .field("use_emoji", &self.use_emoji)
            .field("update_interval", &self.update_interval)
            .finish()
    }
}

impl Default for ConsoleProgressDisplay {
    fn default() -> Self {
        ConsoleProgressDisplay::new(true, 0.1)
    }
}

impl ConsoleProgressDisplay {
    /// A display writing to stdout. (Stdout is UTF-8, so `use_emoji` is kept as given.)
    pub fn new(use_emoji: bool, update_interval: f64) -> Self {
        Self::with_output(use_emoji, update_interval, ConsoleOutput::Stdout)
    }

    /// A display writing to `writer` instead of stdout.
    pub fn with_writer(
        use_emoji: bool,
        update_interval: f64,
        writer: Box<dyn Write + Send>,
    ) -> Self {
        Self::with_output(use_emoji, update_interval, ConsoleOutput::Writer(writer))
    }

    fn with_output(use_emoji: bool, update_interval: f64, output: ConsoleOutput) -> Self {
        ConsoleProgressDisplay {
            use_emoji,
            update_interval,
            last_update: Mutex::new(0.0),
            state: Mutex::new(ConsoleState {
                current_lines: IndexMap::new(),
                pipeline_header_shown: false,
                output,
            }),
        }
    }

    /// `use_emoji`.
    pub fn use_emoji(&self) -> bool {
        self.use_emoji
    }

    /// `update_interval` (seconds between updates).
    pub fn update_interval(&self) -> f64 {
        self.update_interval
    }

    /// `last_update` (seconds since the epoch; 0 forces the next update).
    pub fn last_update(&self) -> f64 {
        *lock(&self.last_update)
    }

    /// Set `last_update`.
    pub fn set_last_update(&self, value: f64) {
        *lock(&self.last_update) = value;
    }

    /// `current_lines`: the last line shown per `file:module:submodule` key.
    pub fn current_lines(&self) -> IndexMap<String, String> {
        lock(&self.state).current_lines.clone()
    }

    fn should_update(&self) -> bool {
        let now = now();
        let mut last = lock(&self.last_update);
        if *last <= 0.0 || (now - *last) >= self.update_interval {
            *last = now;
            return true;
        }
        false
    }

    /// Update bypassing the time throttle without disturbing it (what the tracker does for
    /// `update_progress`).
    fn update_forced(&self, item: &ProgressItem) {
        let saved = self.last_update();
        self.set_last_update(0.0);
        self.update(item);
        self.set_last_update(saved);
    }

    fn emoji_for_module(&self, module: &str) -> &'static str {
        if self.use_emoji {
            module_emoji(module)
        } else {
            ""
        }
    }

    fn status_emoji(&self, status: &str) -> &'static str {
        if self.use_emoji {
            status_emoji(status)
        } else {
            ""
        }
    }

    fn action_message(module: Option<&str>, message: &str) -> String {
        let base = format!("Semantica {}", action_for(module));
        if message.is_empty() {
            return base;
        }
        if let Some((_, tail)) = message.rsplit_once("Semantica:") {
            return strip(tail).to_string();
        }
        format!("{base}: {message}")
    }

    /// The single-line rendering of an item (`_display_item_line` without the write).
    pub fn item_line(&self, item: &ProgressItem) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.use_emoji {
            parts.push(self.status_emoji(&item.status).to_string());
        }
        parts.push(Self::action_message(item.module.as_deref(), &item.message));
        if self.use_emoji {
            let emoji = self.emoji_for_module(item.module.as_deref().unwrap_or(""));
            parts.push(format!("{emoji} {}", or_na(&item.module)));
        } else {
            parts.push(or_na(&item.module).to_string());
        }
        parts.push(or_na(&item.submodule).to_string());

        match item.progress_percentage {
            Some(pct) => {
                let width = 15i64;
                let filled = (width as f64 * pct / 100.0).trunc() as i64;
                let bar = format!(
                    "{}{}",
                    "\u{2588}".repeat(filled.max(0) as usize),
                    "\u{2591}".repeat((width - filled).max(0) as usize)
                );
                parts.push(format!("|{bar}| {}%", format_fixed(pct, 1)));
            }
            None => parts.push(format!("|{}| 0.0%", "\u{2591}".repeat(15))),
        }

        parts.push(match eta_text(item.estimated_remaining) {
            Some(eta) => format!("ETA: {eta}"),
            None => "ETA: -".to_string(),
        });

        match (truthy_time(item.start_time), item.processed_items) {
            (Some(start), Some(processed)) if processed > 0 => {
                let elapsed = now() - start;
                if elapsed > 0.0 {
                    parts.push(format!(
                        "Rate: {}/s",
                        format_fixed(processed as f64 / elapsed, 1)
                    ));
                }
            }
            _ => parts.push("Rate: -".to_string()),
        }

        match truthy_time(item.start_time) {
            Some(start) => {
                let elapsed = match truthy_time(item.end_time) {
                    Some(end) => end - start,
                    None => now() - start,
                };
                parts.push(format!("Time: {}s", format_fixed(elapsed, 2)));
            }
            None => parts.push("Time: -".to_string()),
        }

        if has_extraction_counts(item) {
            if let Some(extracted) = extraction_text(item) {
                if is_docling(item) {
                    parts.push(format!("Extracted (Docling): {extracted}"));
                } else {
                    parts.push(format!("Extracted: {extracted}"));
                }
            }
        } else {
            parts.push("Extracted: -".to_string());
        }
        parts.join(" ")
    }

    fn display_item_line(&self, state: &mut ConsoleState, item: &ProgressItem) {
        let mut key = format!(
            "{}:{}",
            item.module.as_deref().unwrap_or("None"),
            item.submodule.as_deref().unwrap_or("None")
        );
        if let Some(file) = non_empty(&item.file) {
            key = format!("{file}:{key}");
        }
        let line = self.item_line(item);
        state.current_lines.insert(key, line.clone());
        state.write(&line);
    }
}

impl ProgressDisplay for ConsoleProgressDisplay {
    /// Write the item's line (or, for pipeline items, every item of the pipeline under a
    /// header), at most once per `update_interval`.
    fn update(&self, item: &ProgressItem) {
        if !self.should_update() {
            return;
        }
        let mut state = lock(&self.state);
        let pipeline_items = match non_empty(&item.pipeline_id) {
            Some(pipeline_id) => ProgressTracker::get_instance().get_pipeline_items(pipeline_id),
            None => Vec::new(),
        };
        if !pipeline_items.is_empty() {
            state.write(&format!("\r{}\r", " ".repeat(150)));
            if !state.pipeline_header_shown {
                if self.use_emoji {
                    state.write(&format!("{BRAIN} Semantica - {CHART} Current Progress\n"));
                } else {
                    state.write("Semantica - Current Progress\n");
                }
                state.write(&format!("{}\n", "=".repeat(150)));
                state.pipeline_header_shown = true;
            }
            for pipeline_item in &pipeline_items {
                self.display_item_line(&mut state, pipeline_item);
                state.write("\n");
            }
            state.flush();
        } else {
            self.display_item_line(&mut state, item);
            state.flush();
        }
    }

    /// Write the summary grouped by module, with completed/failed counts and total time.
    fn show_summary(&self, items: &[ProgressItem]) {
        let mut state = lock(&self.state);
        let rule = "=".repeat(80);
        state.write(&format!("\n{rule}\n"));
        if self.use_emoji {
            state.write(&format!("{BRAIN} Semantica - {CHART} Progress Summary\n"));
        } else {
            state.write("Semantica - Progress Summary\n");
        }
        state.write(&format!("{rule}\n"));

        let mut total_time = 0.0;
        let mut completed = 0;
        let mut failed = 0;
        for (module, module_items) in group_by_module(items) {
            if self.use_emoji {
                let emoji = self.emoji_for_module(&module);
                state.write(&format!("\n{emoji} {}\n", module.to_uppercase()));
            } else {
                state.write(&format!("\n{}\n", module.to_uppercase()));
            }
            state.write(&format!("{}\n", "-".repeat(80)));
            for item in module_items {
                let emoji = self.status_emoji(&item.status);
                let duration = match (truthy_time(item.start_time), truthy_time(item.end_time)) {
                    (Some(start), Some(end)) => format!(" ({}s)", format_fixed(end - start, 2)),
                    (Some(_), None) => " (running...)".to_string(),
                    _ => String::new(),
                };
                let mut line = format!("  {emoji} {}", or_na(&item.submodule));
                if let Some(file) = non_empty(&item.file) {
                    line.push_str(&format!(" - {}", path_name(file)));
                }
                line.push_str(&duration);
                state.write(&format!("{line}\n"));
                if item.status == "completed" {
                    completed += 1;
                } else if item.status == "failed" {
                    failed += 1;
                }
                if let (Some(start), Some(end)) =
                    (truthy_time(item.start_time), truthy_time(item.end_time))
                {
                    total_time += end - start;
                }
            }
        }
        state.write(&format!("\n{rule}\n"));
        let total = format_fixed(total_time, 2);
        if self.use_emoji {
            state.write(&format!(
                "{CHECK} Completed: {completed} | {CROSS} Failed: {failed} | {STOPWATCH}  Total Time: {total}s\n"
            ));
        } else {
            state.write(&format!(
                "Completed: {completed} | Failed: {failed} | Total Time: {total}s\n"
            ));
        }
        state.write(&format!("{rule}\n"));
        state.flush();
    }

    /// Blank the current console line and forget the shown lines.
    fn clear(&self) {
        let mut state = lock(&self.state);
        state.write(&format!("\r{}\r", " ".repeat(100)));
        state.flush();
        state.current_lines.clear();
    }
}

/// Add or replace an item matched by `(module, submodule, file)`.
fn upsert(items: &mut Vec<ProgressItem>, item: &ProgressItem) {
    match items.iter().position(|existing| {
        existing.module == item.module
            && existing.submodule == item.submodule
            && existing.file == item.file
    }) {
        Some(index) => items[index] = item.clone(),
        None => items.push(item.clone()),
    }
}

/// `JupyterProgressDisplay(use_emoji=True)`: an HTML table in notebooks. Without an IPython
/// kernel ([`IPYTHON_AVAILABLE`] is false) it only keeps its items; [`Self::build_html`] and
/// [`Self::summary_html`] produce the HTML the original would display.
#[derive(Debug)]
pub struct JupyterProgressDisplay {
    use_emoji: bool,
    items: Mutex<Vec<ProgressItem>>,
}

impl Default for JupyterProgressDisplay {
    fn default() -> Self {
        JupyterProgressDisplay::new(true)
    }
}

impl JupyterProgressDisplay {
    /// A new display.
    pub fn new(use_emoji: bool) -> Self {
        JupyterProgressDisplay {
            use_emoji,
            items: Mutex::new(Vec::new()),
        }
    }

    /// `use_emoji`.
    pub fn use_emoji(&self) -> bool {
        self.use_emoji
    }

    /// The items shown so far.
    pub fn items(&self) -> Vec<ProgressItem> {
        lock(&self.items).clone()
    }

    fn emoji_for_module(&self, module: &str) -> &'static str {
        if self.use_emoji {
            module_emoji(module)
        } else {
            ""
        }
    }

    fn status_emoji(&self, status: &str) -> &'static str {
        if self.use_emoji {
            status_emoji(status)
        } else {
            ""
        }
    }

    fn action_message(module: Option<&str>, message: &str) -> String {
        if !message.is_empty()
            && let Some((_, tail)) = message.rsplit_once("Semantica:")
        {
            return strip(tail).to_string();
        }
        format!("Semantica {}", action_for(module))
    }

    /// `_build_html(items)`: the progress table (pipeline items with pending modules when the
    /// items belong to a registered pipeline, else the last ten items).
    pub fn build_html(&self, items: &[ProgressItem]) -> String {
        let mut html = vec![
            "<div style='font-family: monospace;'>".to_string(),
            format!("<h4>{BRAIN} Semantica - {CHART} Current Progress</h4>"),
            "<table style='width: 100%; border-collapse: collapse;'>".to_string(),
        ];
        let pipeline_id = items.iter().find_map(|i| non_empty(&i.pipeline_id));
        let mut pipeline_items = Vec::new();
        if let Some(pipeline_id) = pipeline_id {
            let tracker = ProgressTracker::get_instance();
            pipeline_items = tracker.get_pipeline_items(pipeline_id);
            if let Some(expected) = tracker.pipeline_contexts().get(pipeline_id) {
                let order = tracker
                    .pipeline_module_order()
                    .get(pipeline_id)
                    .cloned()
                    .unwrap_or_default();
                let existing: IndexSet<String> = pipeline_items
                    .iter()
                    .filter_map(|i| non_empty(&i.module).map(str::to_string))
                    .collect();
                for module in expected {
                    if !existing.contains(module) {
                        pipeline_items.push(ProgressItem {
                            module: Some(module.clone()),
                            submodule: Some("Pending".to_string()),
                            status: "pending".to_string(),
                            progress_percentage: Some(0.0),
                            pipeline_id: Some(pipeline_id.to_string()),
                            pipeline_order: Some(order.get(module).copied().unwrap_or(999)),
                            ..ProgressItem::default()
                        });
                    }
                }
                sort_pipeline_items(&mut pipeline_items);
            }
        }
        let display_items: &[ProgressItem] = if pipeline_items.is_empty() {
            &items[items.len().saturating_sub(10)..]
        } else {
            &pipeline_items
        };
        let extracted_header = if display_items.iter().any(is_docling) {
            "Extracted (Docling)"
        } else {
            "Extracted"
        };
        html.push(format!(
            "<tr><th>Status</th><th>Action</th><th>Module</th><th>Submodule</th><th>Progress</th><th>ETA</th><th>Rate</th><th>Time</th><th>{extracted_header}</th></tr>"
        ));
        for item in display_items {
            let status = self.status_emoji(&item.status);
            let emoji = self.emoji_for_module(item.module.as_deref().unwrap_or(""));
            let action = Self::action_message(item.module.as_deref(), &item.message);
            let mut progress = "-".to_string();
            if let Some(pct) = item.progress_percentage {
                progress = format!("{}%", format_fixed(pct, 1));
                if let (Some(done), Some(total)) = (item.processed_items, item.total_items) {
                    progress.push_str(&format!(" ({done}/{total})"));
                }
            }
            let eta = eta_text(item.estimated_remaining).unwrap_or_else(|| "-".to_string());
            let mut rate = "-".to_string();
            if let (Some(start), Some(processed)) =
                (truthy_time(item.start_time), item.processed_items)
                && processed > 0
            {
                let elapsed = now() - start;
                if elapsed > 0.0 {
                    rate = format!("{}/s", format_fixed(processed as f64 / elapsed, 1));
                }
            }
            let elapsed = match truthy_time(item.start_time) {
                Some(start) => match truthy_time(item.end_time) {
                    Some(end) => format!("{}s", format_fixed(end - start, 2)),
                    None => format!("{}s", format_fixed(now() - start, 2)),
                },
                None => "-".to_string(),
            };
            let mut extracted = "-".to_string();
            if has_extraction_counts(item) {
                if let Some(text) = extraction_text(item) {
                    extracted = text;
                }
            } else if item.status == "running"
                && item.module.as_deref() == Some("parse")
                && (item.message.contains("Docling") || is_docling(item))
            {
                extracted = "Converting with Docling...".to_string();
            }
            html.push(format!(
                "<tr><td>{status}</td><td>{action}</td><td>{emoji} {}</td><td>{}</td><td>{progress}</td><td>{eta}</td><td>{rate}</td><td>{elapsed}</td><td>{extracted}</td></tr>",
                or_na(&item.module),
                or_na(&item.submodule)
            ));
        }
        html.push("</table>".to_string());
        html.push("</div>".to_string());
        html.concat()
    }

    /// The summary table `show_summary` displays in a notebook.
    pub fn summary_html(&self, items: &[ProgressItem]) -> String {
        let mut html = vec![
            "<div style='font-family: monospace;'>".to_string(),
            format!("<h3>{BRAIN} Semantica - {CHART} Progress Summary</h3>"),
            "<table style='width: 100%; border-collapse: collapse; border: 1px solid #ddd;'>"
                .to_string(),
            "<tr style='background-color: #f2f2f2;'><th>Module</th><th>Submodule</th><th>File</th><th>Status</th><th>Time</th></tr>".to_string(),
        ];
        let (mut completed, mut failed, mut total_time) = (0, 0, 0.0);
        for (module, module_items) in group_by_module(items) {
            let emoji = self.emoji_for_module(&module);
            for item in module_items {
                let status = self.status_emoji(&item.status);
                let duration = match (truthy_time(item.start_time), truthy_time(item.end_time)) {
                    (Some(start), Some(end)) => {
                        total_time += end - start;
                        format!("{}s", format_fixed(end - start, 2))
                    }
                    (Some(_), None) => "running...".to_string(),
                    _ => String::new(),
                };
                let file = non_empty(&item.file)
                    .map(path_name)
                    .unwrap_or_else(|| "-".to_string());
                html.push(format!(
                    "<tr><td>{emoji} {module}</td><td>{}</td><td>{file}</td><td>{status}</td><td>{duration}</td></tr>",
                    or_na(&item.submodule)
                ));
                if item.status == "completed" {
                    completed += 1;
                } else if item.status == "failed" {
                    failed += 1;
                }
            }
        }
        html.push("</table>".to_string());
        html.push(format!(
            "<p><strong>{CHECK} Completed:</strong> {completed} | <strong>{CROSS} Failed:</strong> {failed} | <strong>{STOPWATCH} Total Time:</strong> {}s</p>",
            format_fixed(total_time, 2)
        ));
        html.push("</div>".to_string());
        html.concat()
    }
}

impl ProgressDisplay for JupyterProgressDisplay {
    /// Add or replace the item (matched by module, submodule and file). Rendering needs an
    /// IPython kernel, so nothing is displayed.
    fn update(&self, item: &ProgressItem) {
        upsert(&mut lock(&self.items), item);
    }

    /// Display the summary table (nothing without an IPython kernel).
    fn show_summary(&self, _items: &[ProgressItem]) {}

    /// Forget the items.
    fn clear(&self) {
        lock(&self.items).clear();
    }
}

/// `FileProgressDisplay(logger=None)`: progress lines on a logger (`semantica.progress` by
/// default) at `INFO` level.
#[derive(Debug)]
pub struct FileProgressDisplay {
    logger: Logger,
    items: Mutex<Vec<ProgressItem>>,
}

impl Default for FileProgressDisplay {
    fn default() -> Self {
        FileProgressDisplay::new(None)
    }
}

impl FileProgressDisplay {
    /// A display logging to `logger`, or to `get_logger("progress")`.
    pub fn new(logger: Option<Logger>) -> Self {
        FileProgressDisplay {
            logger: logger.unwrap_or_else(|| get_logger("progress")),
            items: Mutex::new(Vec::new()),
        }
    }

    /// The logger progress is written to.
    pub fn logger(&self) -> &Logger {
        &self.logger
    }

    /// The items logged so far.
    pub fn items(&self) -> Vec<ProgressItem> {
        lock(&self.items).clone()
    }
}

impl ProgressDisplay for FileProgressDisplay {
    /// Log `[STATUS] | Module: m | Submodule: s | File: f | Message: text`.
    fn update(&self, item: &ProgressItem) {
        upsert(&mut lock(&self.items), item);
        let mut parts = vec![format!("[{}]", item.status.to_uppercase())];
        if let Some(module) = non_empty(&item.module) {
            parts.push(format!("Module: {module}"));
        }
        if let Some(submodule) = non_empty(&item.submodule) {
            parts.push(format!("Submodule: {submodule}"));
        }
        if let Some(file) = non_empty(&item.file) {
            parts.push(format!("File: {}", path_name(file)));
        }
        if !item.message.is_empty() {
            parts.push(format!("Message: {}", item.message));
        }
        self.logger.info(parts.join(" | "));
    }

    /// Log the summary grouped by module.
    fn show_summary(&self, items: &[ProgressItem]) {
        let rule = "=".repeat(80);
        self.logger.info(&rule);
        self.logger.info("SEMANTICA - PROGRESS SUMMARY");
        self.logger.info(&rule);
        let (mut completed, mut failed, mut total_time) = (0, 0, 0.0);
        for (module, module_items) in group_by_module(items) {
            self.logger.info(format!("\n{}:", module.to_uppercase()));
            for item in module_items {
                let mut duration = String::new();
                if let (Some(start), Some(end)) =
                    (truthy_time(item.start_time), truthy_time(item.end_time))
                {
                    duration = format!(" ({}s)", format_fixed(end - start, 2));
                    total_time += end - start;
                }
                let file = non_empty(&item.file)
                    .map(path_name)
                    .unwrap_or_else(|| "N/A".to_string());
                self.logger.info(format!(
                    "  [{}] {} - {file}{duration}",
                    item.status.to_uppercase(),
                    or_na(&item.submodule)
                ));
                if item.status == "completed" {
                    completed += 1;
                } else if item.status == "failed" {
                    failed += 1;
                }
            }
        }
        self.logger.info(&rule);
        self.logger.info(format!(
            "Completed: {completed} | Failed: {failed} | Total Time: {}s",
            format_fixed(total_time, 2)
        ));
        self.logger.info(&rule);
    }

    /// No-op: log files are not cleared.
    fn clear(&self) {}
}

/// What `ModuleDetector.detect_from_frame` reads from a Python frame: the module's
/// `__name__`, the class of a `self` local, a `cls` local that is a class, and the code's
/// qualified name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FrameInfo {
    /// `frame.f_globals["__name__"]`.
    pub module_name: Option<String>,
    /// `type(frame.f_locals["self"]).__name__`.
    pub self_class: Option<String>,
    /// `frame.f_locals["cls"].__name__` when it is a class.
    pub cls_name: Option<String>,
    /// `frame.f_code.co_qualname`.
    pub qualname: Option<String>,
}

impl FrameInfo {
    /// The frame of a Rust call site: the source file maps to a Python-style module name
    /// (`crates/sem-ingest/src/file_ingestor.rs` -> `semantica.ingest.file_ingestor`). Rust
    /// keeps no `self`/`cls` information at run time.
    pub fn from_location(location: &Location<'_>) -> FrameInfo {
        FrameInfo {
            module_name: Some(module_name_for_path(location.file())),
            ..FrameInfo::default()
        }
    }
}

fn module_name_for_path(file: &str) -> String {
    let normalized = file.replace('\\', "/");
    let without_ext = normalized.strip_suffix(".rs").unwrap_or(&normalized);
    let parts: Vec<&str> = without_ext
        .split('/')
        .filter(|p| !p.is_empty() && *p != ".")
        .collect();
    if let Some(pos) = parts.iter().position(|p| *p == "crates")
        && parts.get(pos + 2) == Some(&"src")
    {
        let krate = parts[pos + 1];
        let base = match krate.strip_prefix("sem-") {
            Some(rest) => format!("semantica.{}", rest.replace('-', "_")),
            None if krate == "semantica" => "semantica".to_string(),
            None => krate.replace('-', "_"),
        };
        let mut module: Vec<&str> = parts[pos + 3..].to_vec();
        if matches!(module.last(), Some(&"lib") | Some(&"mod") | Some(&"main")) {
            module.pop();
        }
        if module.is_empty() {
            return base;
        }
        return format!("{base}.{}", module.join("."));
    }
    parts.join(".")
}

/// `ModuleDetector`: infer `(module, submodule)` for a tracking call.
#[derive(Debug, Clone, Copy, Default)]
pub struct ModuleDetector;

impl ModuleDetector {
    /// `detect_from_frame(frame)`: the module is the second component of a `semantica.*`
    /// module name; the submodule is the class of `self`/`cls`, or the class part of a dotted
    /// qualified name.
    pub fn detect_from_frame(frame: &FrameInfo) -> (Option<String>, Option<String>) {
        let module_name = frame.module_name.clone().unwrap_or_default();
        let module = if module_name.contains("semantica.") {
            module_name.split('.').nth(1).map(str::to_string)
        } else {
            None
        };
        let mut submodule = match (&frame.self_class, &frame.cls_name) {
            (Some(class), _) => Some(class.clone()),
            (None, Some(class)) => Some(class.clone()),
            _ => None,
        };
        if submodule.as_deref().is_none_or(str::is_empty)
            && let Some(qualname) = &frame.qualname
            && let Some((class, _)) = qualname.split_once('.')
        {
            submodule = Some(class.to_string());
        }
        (module, submodule)
    }

    /// `detect_from_call_stack(depth=2)`. Rust has no frame introspection: depth 0 is this
    /// function itself (`("utils", "ModuleDetector")`), and any other depth resolves to the
    /// nearest call site outside `#[track_caller]` functions, whose source path gives the
    /// module (the submodule is unknown).
    #[track_caller]
    pub fn detect_from_call_stack(depth: usize) -> (Option<String>, Option<String>) {
        if depth == 0 {
            return Self::detect_from_frame(&FrameInfo {
                module_name: Some("semantica.utils.progress_tracker".to_string()),
                qualname: Some("ModuleDetector.detect_from_call_stack".to_string()),
                ..FrameInfo::default()
            });
        }
        Self::detect_from_frame(&FrameInfo::from_location(Location::caller()))
    }
}

type ItemRef = Arc<Mutex<ProgressItem>>;

fn snapshot(item: &ItemRef) -> ProgressItem {
    lock(item).clone()
}

#[derive(Default)]
struct TrackerState {
    displays: Vec<Arc<dyn ProgressDisplay>>,
    items: Vec<ItemRef>,
    active_items: IndexMap<String, ItemRef>,
    pipeline_contexts: IndexMap<String, Vec<String>>,
    pipeline_items: IndexMap<String, IndexMap<String, ItemRef>>,
    pipeline_module_order: IndexMap<String, IndexMap<String, i64>>,
}

/// `ProgressTracker(enabled=True, use_emoji=True, update_interval=0.1)`: the tracking
/// coordinator.
pub struct ProgressTracker {
    progress_forced_disabled: AtomicBool,
    enabled: AtomicBool,
    use_emoji: bool,
    update_interval: f64,
    is_jupyter: AtomicBool,
    disable_jupyter_progress: bool,
    state: Mutex<TrackerState>,
}

impl fmt::Debug for ProgressTracker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProgressTracker")
            .field("enabled", &self.enabled())
            .field("use_emoji", &self.use_emoji)
            .field("update_interval", &self.update_interval)
            .finish()
    }
}

impl Default for ProgressTracker {
    fn default() -> Self {
        ProgressTracker::new(true, true, 0.1)
    }
}

static INSTANCE: Lazy<Mutex<Option<Arc<ProgressTracker>>>> = Lazy::new(|| Mutex::new(None));
static GLOBAL_TRACKER: Lazy<Mutex<Option<Arc<ProgressTracker>>>> = Lazy::new(|| Mutex::new(None));

impl ProgressTracker {
    /// A tracker with a console display (stdout) and a file display (`semantica.progress`
    /// logger). `SEMANTICA_DISABLE_PROGRESS` keeps it disabled.
    pub fn new(enabled: bool, use_emoji: bool, update_interval: f64) -> Self {
        let tracker = ProgressTracker {
            progress_forced_disabled: AtomicBool::new(progress_disabled_from_env()),
            enabled: AtomicBool::new(false),
            use_emoji,
            update_interval,
            is_jupyter: AtomicBool::new(Self::detect_jupyter()),
            disable_jupyter_progress: *DISABLE_JUPYTER_PROGRESS,
            state: Mutex::new(TrackerState::default()),
        };
        tracker.set_enabled(enabled);
        {
            let mut state = lock(&tracker.state);
            state.displays.push(Arc::new(ConsoleProgressDisplay::new(
                use_emoji,
                update_interval,
            )));
            state
                .displays
                .push(Arc::new(FileProgressDisplay::new(None)));
        }
        tracker
    }

    /// `tracker.enabled`.
    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    /// `tracker.enabled = value`; enabling is refused for good once `SEMANTICA_DISABLE_PROGRESS`
    /// is set.
    pub fn set_enabled(&self, value: bool) {
        if value
            && (self.progress_forced_disabled.load(Ordering::SeqCst)
                || progress_disabled_from_env())
        {
            self.progress_forced_disabled.store(true, Ordering::SeqCst);
            self.enabled.store(false, Ordering::SeqCst);
            return;
        }
        self.enabled.store(value, Ordering::SeqCst);
    }

    /// `use_emoji`.
    pub fn use_emoji(&self) -> bool {
        self.use_emoji
    }

    /// `update_interval`.
    pub fn update_interval(&self) -> f64 {
        self.update_interval
    }

    /// `is_jupyter` (always false in Rust).
    pub fn is_jupyter(&self) -> bool {
        self.is_jupyter.load(Ordering::SeqCst)
    }

    /// `disable_jupyter_progress` (from `SEMANTICA_DISABLE_JUPYTER_PROGRESS`).
    pub fn disable_jupyter_progress(&self) -> bool {
        self.disable_jupyter_progress
    }

    fn detect_jupyter() -> bool {
        // `_detect_jupyter`: needs IPython.
        IPYTHON_AVAILABLE
    }

    fn ensure_jupyter_display(&self) {
        // `_ensure_jupyter_display`: nothing to add without IPython.
        if !self.enabled() || !IPYTHON_AVAILABLE || self.is_jupyter() {
            return;
        }
        if Self::detect_jupyter() {
            self.is_jupyter.store(true, Ordering::SeqCst);
            let mut state = lock(&self.state);
            if !self.disable_jupyter_progress
                && !state
                    .displays
                    .iter()
                    .any(|d| (d.as_ref() as &dyn Any).is::<JupyterProgressDisplay>())
            {
                state
                    .displays
                    .insert(0, Arc::new(JupyterProgressDisplay::new(self.use_emoji)));
            }
        }
    }

    /// `tracker.displays`.
    pub fn displays(&self) -> Vec<Arc<dyn ProgressDisplay>> {
        lock(&self.state).displays.clone()
    }

    /// `tracker.displays = displays`.
    pub fn set_displays(&self, displays: Vec<Arc<dyn ProgressDisplay>>) {
        lock(&self.state).displays = displays;
    }

    /// `tracker.items`: finished items that were not kept in a pipeline.
    pub fn items(&self) -> Vec<ProgressItem> {
        lock(&self.state).items.iter().map(snapshot).collect()
    }

    /// `tracker.active_items` by tracking id.
    pub fn active_items(&self) -> IndexMap<String, ProgressItem> {
        lock(&self.state)
            .active_items
            .iter()
            .map(|(k, v)| (k.clone(), snapshot(v)))
            .collect()
    }

    /// `tracker.pipeline_contexts`: pipeline id -> module names.
    pub fn pipeline_contexts(&self) -> IndexMap<String, Vec<String>> {
        lock(&self.state).pipeline_contexts.clone()
    }

    /// `tracker.pipeline_module_order`: pipeline id -> module -> order.
    pub fn pipeline_module_order(&self) -> IndexMap<String, IndexMap<String, i64>> {
        lock(&self.state).pipeline_module_order.clone()
    }

    /// `tracker.pipeline_items`: pipeline id -> tracking id -> item.
    pub fn pipeline_items_by_id(&self) -> IndexMap<String, IndexMap<String, ProgressItem>> {
        lock(&self.state)
            .pipeline_items
            .iter()
            .map(|(pid, items)| {
                (
                    pid.clone(),
                    items
                        .iter()
                        .map(|(k, v)| (k.clone(), snapshot(v)))
                        .collect(),
                )
            })
            .collect()
    }

    /// `ProgressTracker.get_instance()`: the process-wide tracker (created on first use).
    pub fn get_instance() -> Arc<ProgressTracker> {
        let mut instance = lock(&INSTANCE);
        instance
            .get_or_insert_with(|| Arc::new(ProgressTracker::default()))
            .clone()
    }

    /// Replace the process-wide tracker (`ProgressTracker._instance = tracker`); `None` makes
    /// the next [`Self::get_instance`] create a fresh one.
    pub fn set_instance(tracker: Option<Arc<ProgressTracker>>) {
        *lock(&INSTANCE) = tracker;
    }

    fn update_displays(
        displays: &[Arc<dyn ProgressDisplay>],
        item: &ProgressItem,
        force_console: bool,
    ) {
        for display in displays {
            if force_console
                && let Some(console) =
                    (display.as_ref() as &dyn Any).downcast_ref::<ConsoleProgressDisplay>()
            {
                console.update_forced(item);
                continue;
            }
            display.update(item);
        }
    }

    /// `register_pipeline_modules(pipeline_id, module_list, module_order=None)`; without an
    /// order, modules are numbered in list order.
    pub fn register_pipeline_modules<S: AsRef<str>>(
        &self,
        pipeline_id: &str,
        module_list: &[S],
        module_order: Option<&IndexMap<String, i64>>,
    ) {
        if !self.enabled() {
            return;
        }
        let modules: Vec<String> = module_list.iter().map(|m| m.as_ref().to_string()).collect();
        let mut state = lock(&self.state);
        let order = match module_order.filter(|o| !o.is_empty()) {
            Some(order) => order.clone(),
            None => {
                let mut generated = IndexMap::new();
                for (idx, module) in modules.iter().enumerate() {
                    generated.insert(module.clone(), idx as i64);
                }
                generated
            }
        };
        state
            .pipeline_contexts
            .insert(pipeline_id.to_string(), modules);
        state
            .pipeline_module_order
            .insert(pipeline_id.to_string(), order);
        state
            .pipeline_items
            .entry(pipeline_id.to_string())
            .or_default();
    }

    /// `get_pipeline_items(pipeline_id)`: the pipeline's kept items followed by its active
    /// items (an active item kept in the pipeline appears twice, as in the original), sorted
    /// by pipeline order then module.
    pub fn get_pipeline_items(&self, pipeline_id: &str) -> Vec<ProgressItem> {
        if !self.enabled() {
            return Vec::new();
        }
        let state = lock(&self.state);
        if !state.pipeline_contexts.contains_key(pipeline_id) {
            return Vec::new();
        }
        let mut items: Vec<ProgressItem> = state
            .pipeline_items
            .get(pipeline_id)
            .map(|m| m.values().map(snapshot).collect())
            .unwrap_or_default();
        for item in state.active_items.values() {
            let item = snapshot(item);
            if item.pipeline_id.as_deref() == Some(pipeline_id) {
                items.push(item);
            }
        }
        drop(state);
        sort_pipeline_items(&mut items);
        items
    }

    /// `clear_pipeline_context(pipeline_id)`.
    pub fn clear_pipeline_context(&self, pipeline_id: &str) {
        if !self.enabled() {
            return;
        }
        let mut state = lock(&self.state);
        state.pipeline_contexts.shift_remove(pipeline_id);
        state.pipeline_items.shift_remove(pipeline_id);
        state.pipeline_module_order.shift_remove(pipeline_id);
    }

    /// `start_tracking(file=None, module=None, submodule=None, message="", pipeline_id=None)`:
    /// start a running item and return its tracking id (`"module:submodule:file"`, `""` when
    /// disabled). A missing module/submodule is detected from the caller (see
    /// [`ModuleDetector::detect_from_call_stack`]); an item whose module belongs to a
    /// registered pipeline joins it.
    #[track_caller]
    pub fn start_tracking(
        &self,
        file: Option<&str>,
        module: Option<&str>,
        submodule: Option<&str>,
        message: &str,
        pipeline_id: Option<&str>,
    ) -> String {
        if !self.enabled() {
            return String::new();
        }
        self.ensure_jupyter_display();
        let mut module = module.filter(|m| !m.is_empty()).map(str::to_string);
        let mut submodule = submodule.filter(|s| !s.is_empty()).map(str::to_string);
        if module.is_none() || submodule.is_none() {
            let (detected_module, detected_submodule) = ModuleDetector::detect_from_call_stack(3);
            module = module.or(detected_module);
            submodule = submodule.or(detected_submodule);
        }
        let tracking_id = format!(
            "{}:{}:{}",
            module.as_deref().unwrap_or("None"),
            submodule.as_deref().unwrap_or("None"),
            file.unwrap_or("")
        );

        let (item, displays) = {
            let mut state = lock(&self.state);
            let mut pipeline_id = pipeline_id.map(str::to_string);
            let mut pipeline_order = None;
            if pipeline_id.is_none()
                && let Some(m) = module.as_deref().filter(|m| !m.is_empty())
            {
                for (pid, modules) in &state.pipeline_contexts {
                    if modules.iter().any(|x| x == m) {
                        pipeline_id = Some(pid.clone());
                        if let Some(order) = state.pipeline_module_order.get(pid) {
                            pipeline_order = order.get(m).copied();
                        }
                        break;
                    }
                }
            }
            let item = ProgressItem {
                file: file.map(str::to_string),
                module: module.clone(),
                submodule: submodule.clone(),
                status: "running".to_string(),
                start_time: Some(now()),
                message: message.to_string(),
                emoji: self
                    .get_emoji_for_module(module.as_deref().unwrap_or(""))
                    .to_string(),
                pipeline_id: pipeline_id.clone(),
                pipeline_order,
                ..ProgressItem::default()
            };
            let item_ref: ItemRef = Arc::new(Mutex::new(item.clone()));
            state
                .active_items
                .insert(tracking_id.clone(), item_ref.clone());
            if let Some(pid) = pipeline_id.filter(|p| !p.is_empty()) {
                state
                    .pipeline_items
                    .entry(pid)
                    .or_default()
                    .insert(tracking_id.clone(), item_ref);
            }
            (item, state.displays.clone())
        };
        Self::update_displays(&displays, &item, false);
        tracking_id
    }

    /// `update_tracking(tracking_id, status="running", message="")`: set the status and
    /// message; `completed`/`failed` finish the item (kept in its pipeline while the pipeline
    /// is registered, else moved to [`Self::items`]).
    pub fn update_tracking(&self, tracking_id: &str, status: Option<&str>, message: &str) {
        if !self.enabled() || tracking_id.is_empty() {
            return;
        }
        let status = status.unwrap_or("running");
        let (item, displays) = {
            let mut state = lock(&self.state);
            let Some(item_ref) = state.active_items.get(tracking_id).cloned() else {
                return;
            };
            let snapshot = {
                let mut item = lock(&item_ref);
                item.status = status.to_string();
                item.message = message.to_string();
                if let (Some(processed), Some(total)) = (item.processed_items, item.total_items) {
                    item.progress_percentage = Some(if total > 0 {
                        processed as f64 / total as f64 * 100.0
                    } else {
                        0.0
                    });
                    item.estimated_remaining = Self::calculate_eta(&item);
                }
                if status == "completed" || status == "failed" {
                    item.end_time = Some(now());
                    let completed = status == "completed";
                    item.progress_percentage = completed.then_some(100.0);
                    item.estimated_remaining = completed.then_some(0.0);
                }
                item.clone()
            };
            if status == "completed" || status == "failed" {
                match snapshot.pipeline_id.clone().filter(|p| !p.is_empty()) {
                    Some(pid) if state.pipeline_contexts.contains_key(&pid) => {
                        state
                            .pipeline_items
                            .entry(pid)
                            .or_default()
                            .insert(tracking_id.to_string(), item_ref.clone());
                    }
                    _ => state.items.push(item_ref.clone()),
                }
                state.active_items.shift_remove(tracking_id);
            }
            (snapshot, state.displays.clone())
        };
        Self::update_displays(&displays, &item, false);
    }

    /// `update_progress(tracking_id, processed, total, message="")`: record counts, the
    /// percentage and an ETA; the console display is refreshed regardless of its throttle.
    pub fn update_progress(&self, tracking_id: &str, processed: i64, total: i64, message: &str) {
        if !self.enabled() || tracking_id.is_empty() {
            return;
        }
        self.ensure_jupyter_display();
        let (item, displays) = {
            let state = lock(&self.state);
            let Some(item_ref) = state.active_items.get(tracking_id) else {
                return;
            };
            let mut item = lock(item_ref);
            item.processed_items = Some(processed);
            item.total_items = Some(total);
            item.progress_percentage = Some(if total > 0 {
                processed as f64 / total as f64 * 100.0
            } else {
                0.0
            });
            item.estimated_remaining = Self::calculate_eta(&item);
            if !message.is_empty() {
                item.message = message.to_string();
            }
            (item.clone(), state.displays.clone())
        };
        Self::update_displays(&displays, &item, true);
    }

    fn calculate_eta(item: &ProgressItem) -> Option<f64> {
        let start = item.start_time?;
        let processed = item.processed_items?;
        let total = item.total_items?;
        if processed <= 0 {
            return None;
        }
        let elapsed = now() - start;
        if elapsed <= 0.0 {
            return None;
        }
        let rate = processed as f64 / elapsed;
        if rate <= 0.0 {
            return None;
        }
        let remaining = total - processed;
        if remaining <= 0 {
            return Some(0.0);
        }
        Some((remaining as f64 / rate).max(0.0))
    }

    /// `stop_tracking(tracking_id, status="completed", message="", metadata=None)`: merge
    /// `metadata` into the item, then finish it with [`Self::update_tracking`].
    pub fn stop_tracking(
        &self,
        tracking_id: &str,
        status: Option<&str>,
        message: &str,
        metadata: Option<&Map<String, Value>>,
    ) {
        {
            let state = lock(&self.state);
            let metadata = metadata.filter(|m| !m.is_empty());
            if let (Some(item), Some(metadata)) = (state.active_items.get(tracking_id), metadata) {
                let mut item = lock(item);
                for (k, v) in metadata {
                    item.metadata.insert(k.clone(), v.clone());
                }
            }
            for items in state.pipeline_items.values() {
                if let Some(item) = items.get(tracking_id) {
                    if let Some(metadata) = metadata {
                        let mut item = lock(item);
                        for (k, v) in metadata {
                            item.metadata.insert(k.clone(), v.clone());
                        }
                    }
                    break;
                }
            }
        }
        self.update_tracking(tracking_id, Some(status.unwrap_or("completed")), message);
    }

    fn get_emoji_for_module(&self, module: &str) -> &'static str {
        if !self.use_emoji || module.is_empty() {
            return "";
        }
        module_emoji(module)
    }

    /// `show_summary()`: every display shows the finished and active items.
    pub fn show_summary(&self) {
        if !self.enabled() {
            return;
        }
        let (items, displays) = {
            let state = lock(&self.state);
            let items: Vec<ProgressItem> = state
                .items
                .iter()
                .chain(state.active_items.values())
                .map(snapshot)
                .collect();
            (items, state.displays.clone())
        };
        for display in displays {
            display.show_summary(&items);
        }
    }

    /// `with tracker.track(file, module, submodule, message) as tracking_id:`: run `body`
    /// with the tracking id; the item completes when it returns `Ok` and fails (with the
    /// error text as message) when it returns `Err`.
    #[track_caller]
    pub fn track<T, E: fmt::Display>(
        &self,
        file: Option<&str>,
        module: Option<&str>,
        submodule: Option<&str>,
        message: &str,
        body: impl FnOnce(&str) -> Result<T, E>,
    ) -> Result<T, E> {
        let tracking_id = self.start_tracking(file, module, submodule, message, None);
        match body(&tracking_id) {
            Ok(value) => {
                self.stop_tracking(&tracking_id, Some("completed"), "", None);
                Ok(value)
            }
            Err(err) => {
                self.stop_tracking(&tracking_id, Some("failed"), &err.to_string(), None);
                Err(err)
            }
        }
    }
}

/// `get_progress_tracker()`: the global tracker (the [`ProgressTracker::get_instance`]
/// singleton, cached on first use).
pub fn get_progress_tracker() -> Arc<ProgressTracker> {
    let tracker = {
        let mut global = lock(&GLOBAL_TRACKER);
        global
            .get_or_insert_with(ProgressTracker::get_instance)
            .clone()
    };
    tracker.ensure_jupyter_display();
    tracker
}

/// Replace the tracker [`get_progress_tracker`] returns (the module's `_global_tracker`).
pub fn set_global_progress_tracker(tracker: Option<Arc<ProgressTracker>>) {
    *lock(&GLOBAL_TRACKER) = tracker;
}

/// The decorator returned by [`track_progress`]: [`TrackProgress::run`] tracks one call.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrackProgress {
    /// `file` context.
    pub file: Option<String>,
    /// `module` (detected from the caller when absent).
    pub module: Option<String>,
    /// `submodule` (detected, else the function name).
    pub submodule: Option<String>,
}

/// `@track_progress` / `@track_progress(file=..., module=..., submodule=...)`.
pub fn track_progress(
    file: Option<&str>,
    module: Option<&str>,
    submodule: Option<&str>,
) -> TrackProgress {
    TrackProgress {
        file: file.map(str::to_string),
        module: module.map(str::to_string),
        submodule: submodule.map(str::to_string),
    }
}

impl TrackProgress {
    /// Call `func` (named `func_name`) under tracking: `Running f`, then `Completed f` or
    /// `Failed f: <error>`. Python also read a file name from the call's arguments; Rust
    /// callers pass it as `file`.
    #[track_caller]
    pub fn run<T, E: fmt::Display>(
        &self,
        func_name: &str,
        func: impl FnOnce() -> Result<T, E>,
    ) -> Result<T, E> {
        let tracker = get_progress_tracker();
        let mut module = self.module.clone().filter(|m| !m.is_empty());
        let mut submodule = self.submodule.clone().filter(|s| !s.is_empty());
        if module.is_none() || submodule.is_none() {
            let (detected_module, detected_submodule) = ModuleDetector::detect_from_call_stack(2);
            module = module.or(detected_module);
            submodule = submodule.or(detected_submodule);
        }
        let submodule = submodule.unwrap_or_else(|| func_name.to_string());
        let tracking_id = tracker.start_tracking(
            self.file.as_deref(),
            module.as_deref(),
            Some(&submodule),
            &format!("Running {func_name}"),
            None,
        );
        match func() {
            Ok(value) => {
                tracker.stop_tracking(
                    &tracking_id,
                    Some("completed"),
                    &format!("Completed {func_name}"),
                    None,
                );
                Ok(value)
            }
            Err(err) => {
                tracker.stop_tracking(
                    &tracking_id,
                    Some("failed"),
                    &format!("Failed {func_name}: {err}"),
                    None,
                );
                Err(err)
            }
        }
    }
}
