//! RSS/Atom feed ingestion (`semantica/ingest/feed_ingestor.py`): parsing with format
//! detection, feed discovery, update monitoring in a background thread, content extraction.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use indexmap::IndexMap;
use sem_utils::logging::Logger;
use sem_utils::progress_tracker::ProgressTracker;
use sem_utils::pycompat::time::{PyDateTime, strptime};
use sem_utils::{SemanticaError, get_logger, get_progress_tracker};
use serde_json::{Map, Value, json};

use crate::config::{processing_error, validation_error};
use crate::ssrf::{GuardOptions, parse_bool, request_with_ssrf_guard};
use crate::support::etree::{self, Element};
use crate::support::hooks::Hook;
use crate::support::html::{AttrFilter, Document, TagFilter};
use crate::support::http::{RequestOptions, is_request_exception};
use crate::support::kw::{self, Kwargs};
use crate::support::pytime;
use crate::support::pyurl;

const ATOM: &str = "{http://www.w3.org/2005/Atom}";

/// Replacement for `FeedMonitor.check_updates` (`patch.object(monitor, "check_updates")`).
pub static CHECK_UPDATES: Hook<Box<dyn Fn(&str) -> Result<Vec<FeedItem>, SemanticaError>>> =
    Hook::new("semantica.ingest.feed_ingestor.FeedMonitor.check_updates");

/// Replacement for `threading.Thread(target=...).start()` as the monitor uses it: receives the
/// loop and decides whether/how to run it (`patch("threading.Thread")`).
pub static THREAD_START: Hook<Box<dyn Fn(Box<dyn FnOnce() + Send>) -> Option<JoinHandle<()>>>> =
    Hook::new("threading.Thread");

/// Replacement for `FeedIngestor.ingest_feed` (`patch.object(feed_ingestor, "ingest_feed")`).
pub static INGEST_FEED: Hook<Box<dyn Fn(&str) -> Result<FeedData, SemanticaError>>> =
    Hook::new("semantica.ingest.feed_ingestor.FeedIngestor.ingest_feed");

/// Replacement for `FeedMonitor.add_feed` (records `(url, options)` instead).
pub static ADD_FEED: Hook<Box<dyn Fn(&str, &Kwargs)>> =
    Hook::new("semantica.ingest.feed_ingestor.FeedMonitor.add_feed");

/// Replacement for `FeedMonitor.start_monitoring`.
pub static START_MONITORING: Hook<Box<dyn Fn()>> =
    Hook::new("semantica.ingest.feed_ingestor.FeedMonitor.start_monitoring");

/// `FeedItem`.
#[derive(Debug, Clone, PartialEq)]
pub struct FeedItem {
    /// `title`.
    pub title: String,
    /// `link`.
    pub link: String,
    /// `description`.
    pub description: String,
    /// `content`.
    pub content: String,
    /// `author`.
    pub author: String,
    /// `published`.
    pub published: Option<PyDateTime>,
    /// `updated`.
    pub updated: Option<PyDateTime>,
    /// `guid`.
    pub guid: String,
    /// `categories`.
    pub categories: Vec<String>,
    /// `metadata`.
    pub metadata: Map<String, Value>,
}

impl FeedItem {
    /// `FeedItem(title, link)` with the other defaults.
    pub fn new(title: &str, link: &str) -> FeedItem {
        FeedItem {
            title: title.to_string(),
            link: link.to_string(),
            description: String::new(),
            content: String::new(),
            author: String::new(),
            published: None,
            updated: None,
            guid: String::new(),
            categories: Vec::new(),
            metadata: Map::new(),
        }
    }
}

/// `FeedData`.
#[derive(Debug, Clone, PartialEq)]
pub struct FeedData {
    /// `title`.
    pub title: String,
    /// `link`.
    pub link: String,
    /// `description`.
    pub description: String,
    /// `language`.
    pub language: String,
    /// `updated`.
    pub updated: Option<PyDateTime>,
    /// `items`.
    pub items: Vec<FeedItem>,
    /// `metadata`.
    pub metadata: Map<String, Value>,
}

impl FeedData {
    /// `FeedData(title, link, items=items)` with the other defaults.
    pub fn new(title: &str, link: &str, items: Vec<FeedItem>) -> FeedData {
        FeedData {
            title: title.to_string(),
            link: link.to_string(),
            description: String::new(),
            language: String::new(),
            updated: None,
            items,
            metadata: Map::new(),
        }
    }
}

fn text_of(element: Option<&Element>) -> String {
    element.and_then(|e| e.text.clone()).unwrap_or_default()
}

/// `FeedParser`: RSS and Atom parsing.
#[derive(Debug, Clone)]
pub struct FeedParser {
    /// Configuration (unused).
    pub config: Kwargs,
    logger: Logger,
}

impl Default for FeedParser {
    fn default() -> Self {
        FeedParser::new(Kwargs::new())
    }
}

impl FeedParser {
    /// `FeedParser(**config)`.
    pub fn new(config: Kwargs) -> FeedParser {
        FeedParser {
            config,
            logger: get_logger("feed_parser"),
        }
    }

    /// `parse_feed(feed_content, feed_url=None)`.
    pub fn parse_feed(&self, feed_content: &str, feed_url: Option<&str>) -> Result<FeedData, SemanticaError> {
        let result = match self.detect_format(feed_content).as_str() {
            "rss" => self._parse_rss(feed_content, feed_url),
            "atom" => self._parse_atom(feed_content, feed_url),
            other => Err(processing_error(format!("Unsupported feed format: {other}"))),
        };
        result.map_err(|e| {
            self.logger.error(format!("Failed to parse feed: {e}"));
            processing_error(format!("Failed to parse feed: {e}"))
        })
    }

    /// `detect_format(feed_content)`: `"rss"` or `"atom"`.
    pub fn detect_format(&self, feed_content: &str) -> String {
        match etree::fromstring(feed_content) {
            Ok(root) => {
                if root.tag.ends_with("feed") {
                    "atom".to_string()
                } else {
                    "rss".to_string()
                }
            }
            Err(_) => {
                let lower = feed_content.to_lowercase();
                if lower.contains("<feed") || lower.contains("xmlns:atom") {
                    "atom".to_string()
                } else {
                    "rss".to_string()
                }
            }
        }
    }

    fn _parse_rss(&self, feed_content: &str, feed_url: Option<&str>) -> Result<FeedData, SemanticaError> {
        let root = etree::fromstring(feed_content)?;
        let Some(channel) = root.find("channel") else {
            return Err(processing_error("Invalid RSS feed: no channel element found"));
        };
        let title = text_of(channel.find("title"));
        let link = {
            let l = text_of(channel.find("link"));
            if l.is_empty() { feed_url.unwrap_or("").to_string() } else { l }
        };
        let description = text_of(channel.find("description"));
        let language = text_of(channel.find("language"));
        let mut items = Vec::new();
        for item in channel.findall("item") {
            let item_title = text_of(item.find("title"));
            let item_link = text_of(item.find("link"));
            let item_description = text_of(item.find("description"));
            let guid = {
                let g = text_of(item.find("guid"));
                if g.is_empty() { item_link.clone() } else { g }
            };
            let published = match item.find("pubDate").and_then(|e| e.text.clone()) {
                Some(t) if !t.is_empty() => self._parse_date(&t)?,
                _ => None,
            };
            let categories = item
                .findall("category")
                .into_iter()
                .filter_map(|c| c.text.clone().filter(|t| !t.is_empty()))
                .collect();
            let mut feed_item = FeedItem::new(&item_title, &item_link);
            feed_item.description = item_description;
            feed_item.guid = guid;
            feed_item.published = published;
            feed_item.categories = categories;
            items.push(feed_item);
        }
        let mut data = FeedData::new(&title, &link, items);
        data.description = description;
        data.language = language;
        Ok(data)
    }

    fn _parse_atom(&self, feed_content: &str, feed_url: Option<&str>) -> Result<FeedData, SemanticaError> {
        let root = etree::fromstring(feed_content)?;
        let title = text_of(root.find(&format!("{ATOM}title")));
        let link_elem = root
            .find(&format!("{ATOM}link[@rel='self']"))
            .or_else(|| root.find(&format!("{ATOM}link")));
        let link = match link_elem {
            Some(l) => l.get("href").unwrap_or("").to_string(),
            None => feed_url.unwrap_or("").to_string(),
        };
        let description = text_of(root.find(&format!("{ATOM}subtitle")));
        let updated = match root.find(&format!("{ATOM}updated")).and_then(|e| e.text.clone()) {
            Some(t) if !t.is_empty() => self._parse_date(&t)?,
            _ => None,
        };
        let mut items = Vec::new();
        for entry in root.findall(&format!("{ATOM}entry")) {
            let item_title = text_of(entry.find(&format!("{ATOM}title")));
            let link = entry
                .find(&format!("{ATOM}link[@rel='alternate']"))
                .or_else(|| entry.find(&format!("{ATOM}link")))
                .map(|l| l.get("href").unwrap_or("").to_string())
                .unwrap_or_default();
            let description = text_of(entry.find(&format!("{ATOM}summary")));
            let content = text_of(entry.find(&format!("{ATOM}content")));
            let guid = text_of(entry.find(&format!("{ATOM}id")));
            let published = match entry.find(&format!("{ATOM}published")).and_then(|e| e.text.clone()) {
                Some(t) if !t.is_empty() => self._parse_date(&t)?,
                _ => None,
            };
            let item_updated = match entry.find(&format!("{ATOM}updated")).and_then(|e| e.text.clone()) {
                Some(t) if !t.is_empty() => self._parse_date(&t)?,
                _ => None,
            };
            let categories = entry
                .findall(&format!("{ATOM}category"))
                .into_iter()
                .filter_map(|c| c.get("term").filter(|t| !t.is_empty()).map(str::to_string))
                .collect();
            let mut item = FeedItem::new(&item_title, &link);
            item.description = description;
            item.content = content;
            item.guid = guid;
            item.published = published;
            item.updated = item_updated;
            item.categories = categories;
            items.push(item);
        }
        let mut data = FeedData::new(&title, &link, items);
        data.description = description;
        data.updated = updated;
        Ok(data)
    }

    /// `_parse_date(date_string)`: the RFC 822 / ISO 8601 formats, then the `dateutil`
    /// fallback; `ValueError` when nothing parses.
    pub fn _parse_date(&self, date_string: &str) -> Result<Option<PyDateTime>, SemanticaError> {
        const FORMATS: [&str; 7] = [
            "%a, %d %b %Y %H:%M:%S %z",
            "%a, %d %b %Y %H:%M:%S %Z",
            "%Y-%m-%dT%H:%M:%S%z",
            "%Y-%m-%dT%H:%M:%SZ",
            "%Y-%m-%dT%H:%M:%S",
            "%Y-%m-%d %H:%M:%S",
            "%Y-%m-%d",
        ];
        let trimmed = sem_utils::pycompat::fmt::strip(date_string);
        for fmt in FORMATS {
            if let Ok(dt) = strptime(trimmed, fmt) {
                return Ok(Some(dt));
            }
        }
        match crate::support::dateparse::parse(date_string) {
            Some(dt) => Ok(Some(dt)),
            None => Err(SemanticaError::Value(format!("Invalid date format: {date_string}"))),
        }
    }

    /// `validate_feed(feed_data)`.
    pub fn validate_feed(&self, feed_data: &FeedData) -> bool {
        if feed_data.title.is_empty() || feed_data.items.is_empty() {
            return false;
        }
        feed_data
            .items
            .iter()
            .all(|item| !(item.title.is_empty() && item.description.is_empty()))
    }
}

/// The monitored state shared with the monitoring thread.
#[derive(Debug, Clone, PartialEq)]
pub struct MonitoredFeed {
    /// `url`.
    pub url: String,
    /// `last_check`.
    pub last_check: Option<PyDateTime>,
    /// `last_items` (GUIDs or links).
    pub last_items: Vec<String>,
    /// `options`.
    pub options: Kwargs,
}

type UpdateCallback = Arc<dyn Fn(&str, &[FeedItem]) + Send + Sync>;

/// `FeedMonitor`: polls feeds for new items in a background thread.
pub struct FeedMonitor {
    /// Configuration.
    pub config: Kwargs,
    /// Monitored feeds.
    pub feeds: Arc<Mutex<IndexMap<String, MonitoredFeed>>>,
    /// `monitoring` flag (shared with the thread).
    pub monitoring: Arc<AtomicBool>,
    /// The monitoring thread.
    pub thread: Option<JoinHandle<()>>,
    /// `check_interval` in seconds.
    pub check_interval: f64,
    /// `allow_private_ips`.
    pub allow_private_ips: bool,
    update_callback: Option<UpdateCallback>,
    thread_started: bool,
    logger: Logger,
}

impl std::fmt::Debug for FeedMonitor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FeedMonitor")
            .field("feeds", &self.feeds.lock().map(|f| f.keys().cloned().collect::<Vec<_>>()).unwrap_or_default())
            .field("monitoring", &self.monitoring.load(Ordering::SeqCst))
            .field("check_interval", &self.check_interval)
            .finish()
    }
}

fn check_updates_impl(
    feeds: &Mutex<IndexMap<String, MonitoredFeed>>,
    allow_private_ips: bool,
    logger: &Logger,
    feed_url: &str,
) -> Result<Vec<FeedItem>, SemanticaError> {
    if let Some(f) = CHECK_UPDATES.get() {
        return f(feed_url);
    }
    if !feeds.lock().map(|f| f.contains_key(feed_url)).unwrap_or(false) {
        return Err(validation_error(format!("Feed {feed_url} not in monitoring list")));
    }
    let parser = FeedParser::default();
    let fetched = (|| -> Result<FeedData, SemanticaError> {
        let options = RequestOptions {
            timeout: Some(30.0),
            ..RequestOptions::default()
        };
        let response = request_with_ssrf_guard(
            "GET",
            feed_url,
            None,
            &GuardOptions::allow_private_ips(allow_private_ips),
            options,
        )?;
        response.raise_for_status()?;
        parser.parse_feed(&response.text(), Some(feed_url))
    })();
    let feed_data = match fetched {
        Ok(d) => d,
        Err(e) => {
            logger.error(format!("Failed to check feed {feed_url}: {e}"));
            return Ok(Vec::new());
        }
    };
    let mut feeds = feeds.lock().map_err(|_| SemanticaError::Runtime("feed state poisoned".to_string()))?;
    let Some(info) = feeds.get_mut(feed_url) else {
        return Ok(Vec::new());
    };
    let key = |item: &FeedItem| {
        if item.guid.is_empty() { item.link.clone() } else { item.guid.clone() }
    };
    let current = feed_data.items;
    if info.last_items.is_empty() {
        info.last_items = current.iter().map(key).collect();
        info.last_check = Some(PyDateTime::now(false));
        return Ok(current);
    }
    let new_items: Vec<FeedItem> = current
        .iter()
        .filter(|item| !info.last_items.contains(&key(item)))
        .cloned()
        .collect();
    info.last_items = current.iter().map(key).collect();
    info.last_check = Some(PyDateTime::now(false));
    Ok(new_items)
}

fn monitoring_loop(
    feeds: Arc<Mutex<IndexMap<String, MonitoredFeed>>>,
    monitoring: Arc<AtomicBool>,
    callback: Option<UpdateCallback>,
    check_interval: f64,
    allow_private_ips: bool,
    logger: Logger,
) {
    while monitoring.load(Ordering::SeqCst) {
        let urls: Vec<String> = feeds.lock().map(|f| f.keys().cloned().collect()).unwrap_or_default();
        for url in urls {
            match check_updates_impl(&feeds, allow_private_ips, &logger, &url) {
                Ok(items) => {
                    if !items.is_empty()
                        && let Some(cb) = &callback
                    {
                        cb(&url, &items);
                    }
                }
                Err(e) => logger.error(format!("Error checking feed {url}: {e}")),
            }
        }
        pytime::sleep(check_interval);
    }
}

impl FeedMonitor {
    /// `FeedMonitor(**config)` (`check_interval` default 3600 s).
    pub fn new(config: Kwargs) -> Result<FeedMonitor, SemanticaError> {
        let allow_private_ips = parse_bool(config.get("allow_private_ips").unwrap_or(&Value::Null), false)?;
        Ok(FeedMonitor {
            check_interval: kw::get_f64(&config, "check_interval", 3600.0),
            config,
            feeds: Arc::new(Mutex::new(IndexMap::new())),
            monitoring: Arc::new(AtomicBool::new(false)),
            thread: None,
            allow_private_ips,
            update_callback: None,
            thread_started: false,
            logger: get_logger("feed_monitor"),
        })
    }

    /// `add_feed(feed_url, **options)`.
    pub fn add_feed(&mut self, feed_url: &str, options: &Kwargs) -> Result<(), SemanticaError> {
        if let Some(f) = ADD_FEED.get() {
            f(feed_url, options);
            return Ok(());
        }
        if feed_url.is_empty() {
            return Err(validation_error("Feed URL is required"));
        }
        if let Ok(mut feeds) = self.feeds.lock() {
            feeds.insert(
                feed_url.to_string(),
                MonitoredFeed {
                    url: feed_url.to_string(),
                    last_check: None,
                    last_items: Vec::new(),
                    options: options.clone(),
                },
            );
        }
        self.logger.info(format!("Added feed to monitoring: {feed_url}"));
        Ok(())
    }

    /// `start_monitoring()`.
    pub fn start_monitoring(&mut self) {
        if let Some(f) = START_MONITORING.get() {
            f();
            return;
        }
        if self.monitoring.load(Ordering::SeqCst) {
            self.logger.warning("Monitoring already started");
            return;
        }
        if self.feeds.lock().map(|f| f.is_empty()).unwrap_or(true) {
            self.logger.warning("No feeds to monitor");
            return;
        }
        self.monitoring.store(true, Ordering::SeqCst);
        let feeds = self.feeds.clone();
        let monitoring = self.monitoring.clone();
        let callback = self.update_callback.clone();
        let interval = self.check_interval;
        let allow = self.allow_private_ips;
        let logger = self.logger.clone();
        let run: Box<dyn FnOnce() + Send> =
            Box::new(move || monitoring_loop(feeds, monitoring, callback, interval, allow, logger));
        self.thread = match THREAD_START.get() {
            Some(f) => f(run),
            None => Some(std::thread::spawn(run)),
        };
        self.thread_started = true;
        self.logger.info("Feed monitoring started");
    }

    /// `stop_monitoring()` (joins the thread for at most 5 seconds).
    pub fn stop_monitoring(&mut self) {
        self.monitoring.store(false, Ordering::SeqCst);
        if let Some(handle) = self.thread.take() {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !handle.is_finished() && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            if handle.is_finished() {
                let _ = handle.join();
            }
        }
        self.logger.info("Feed monitoring stopped");
    }

    /// `check_updates(feed_url)`: new items since the previous check (all items the first
    /// time).
    pub fn check_updates(&self, feed_url: &str) -> Result<Vec<FeedItem>, SemanticaError> {
        check_updates_impl(&self.feeds, self.allow_private_ips, &self.logger, feed_url)
    }

    /// `_monitoring_loop()`: runs on the calling thread until `monitoring` is cleared.
    pub fn _monitoring_loop(&self) {
        monitoring_loop(
            self.feeds.clone(),
            self.monitoring.clone(),
            self.update_callback.clone(),
            self.check_interval,
            self.allow_private_ips,
            self.logger.clone(),
        );
    }

    /// `set_update_callback(callback)`.
    pub fn set_update_callback(&mut self, callback: impl Fn(&str, &[FeedItem]) + Send + Sync + 'static) {
        self.update_callback = Some(Arc::new(callback));
    }

    /// Whether `start_monitoring` started a thread at least once.
    pub fn thread_started(&self) -> bool {
        self.thread_started
    }
}

/// `FeedIngestor`: RSS/Atom feed ingestion.
pub struct FeedIngestor {
    /// Merged configuration.
    pub config: Kwargs,
    /// `allow_private_ips`.
    pub allow_private_ips: bool,
    /// Parser.
    pub parser: FeedParser,
    /// Monitor.
    pub monitor: FeedMonitor,
    /// `update_intervals`.
    pub update_intervals: Value,
    /// Progress tracker.
    pub progress_tracker: Arc<ProgressTracker>,
    logger: Logger,
}

impl std::fmt::Debug for FeedIngestor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FeedIngestor").field("config", &self.config).finish()
    }
}

impl FeedIngestor {
    /// `FeedIngestor(config=None, **kwargs)`.
    pub fn new(config: Kwargs) -> Result<FeedIngestor, SemanticaError> {
        let allow_private_ips = parse_bool(config.get("allow_private_ips").unwrap_or(&Value::Null), false)?;
        let ingestor = FeedIngestor {
            parser: FeedParser::new(config.clone()),
            monitor: FeedMonitor::new(config.clone())?,
            update_intervals: config.get("update_intervals").cloned().unwrap_or(json!({})),
            config,
            allow_private_ips,
            progress_tracker: get_progress_tracker(),
            logger: get_logger("feed_ingestor"),
        };
        ingestor.logger.debug("Feed ingestor initialized");
        Ok(ingestor)
    }

    /// `ingest_feed(feed_url, timeout=None, validate=True, **options)`.
    pub fn ingest_feed(&mut self, feed_url: &str, timeout: Option<f64>, validate: bool, options: &Kwargs) -> Result<FeedData, SemanticaError> {
        if let Some(f) = INGEST_FEED.get() {
            return f(feed_url);
        }
        let tracking_id = self.progress_tracker.start_tracking(
            Some(feed_url),
            Some("ingest"),
            Some("FeedIngestor"),
            &format!("Feed: {feed_url}"),
            None,
        );
        let result = (|| -> Result<FeedData, SemanticaError> {
            let valid = match pyurl::urlparse(feed_url) {
                Ok(p) => !p.scheme.is_empty() && !p.netloc.is_empty(),
                Err(_) => false,
            };
            if !valid {
                return Err(validation_error(format!("Invalid feed URL: {feed_url}")));
            }
            let request_timeout = match timeout {
                Some(t) if t != 0.0 => t,
                _ => match options.get("timeout").and_then(Value::as_f64) {
                    Some(t) if t != 0.0 => t,
                    Some(_) => kw::get_f64(&self.config, "timeout", 30.0),
                    None => kw::get_f64(&self.config, "timeout", 30.0),
                },
            };
            let request = RequestOptions {
                timeout: Some(request_timeout),
                ..RequestOptions::default()
            };
            let response = match request_with_ssrf_guard(
                "GET",
                feed_url,
                None,
                &GuardOptions::allow_private_ips(self.allow_private_ips),
                request,
            )
            .and_then(|r| {
                r.raise_for_status()?;
                Ok(r)
            }) {
                Ok(r) => r,
                Err(e) if is_request_exception(&e) => {
                    self.progress_tracker
                        .stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
                    self.logger.error(format!("Failed to fetch feed {feed_url}: {e}"));
                    return Err(processing_error(format!("Failed to fetch feed: {e}")));
                }
                Err(e) => return Err(e),
            };
            let text = response.text();
            self.logger.debug(format!(
                "Fetched feed from {feed_url}: {} bytes",
                text.chars().count()
            ));
            let feed_data = self.parser.parse_feed(&text, Some(feed_url))?;
            let validate_option = options.get("validate").map(kw::truthy).unwrap_or(true);
            if (validate || validate_option) && !self.parser.validate_feed(&feed_data) {
                self.logger.warning(format!("Feed {feed_url} validation failed"));
            }
            self.progress_tracker.stop_tracking(
                &tracking_id,
                Some("completed"),
                &format!("Ingested {} items", feed_data.items.len()),
                None,
            );
            self.logger.info(format!(
                "Ingested feed {feed_url}: {} item(s)",
                feed_data.items.len()
            ));
            Ok(feed_data)
        })();
        if let Err(e) = &result {
            self.progress_tracker
                .stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
        }
        result
    }

    /// `discover_feeds(website_url)`: feed links in the page plus common feed paths, kept
    /// when they answer 200; `[]` on any failure.
    pub fn discover_feeds(&self, website_url: &str) -> Vec<String> {
        let guard = GuardOptions::allow_private_ips(self.allow_private_ips);
        let result = (|| -> Result<Vec<String>, SemanticaError> {
            let response = request_with_ssrf_guard(
                "GET",
                website_url,
                None,
                &guard,
                RequestOptions {
                    timeout: Some(30.0),
                    ..RequestOptions::default()
                },
            )?;
            response.raise_for_status()?;
            let soup = Document::parse(&response.text());
            let filter = TagFilter::name("link").attr(
                "type",
                AttrFilter::Func(Box::new(|t: Option<&str>| {
                    t.is_some_and(|t| !t.is_empty() && (t.to_lowercase().contains("rss") || t.to_lowercase().contains("atom")))
                })),
            );
            let mut feed_urls: Vec<String> = Vec::new();
            for link in soup.find_all(soup.root(), &filter) {
                if let Some(href) = soup.attr(link, "href").filter(|h| !h.is_empty()) {
                    feed_urls.push(pyurl::urljoin(website_url, href)?);
                }
            }
            for path in ["/feed", "/rss", "/atom", "/feed.xml", "/rss.xml", "/atom.xml"] {
                let probe = (|| -> Result<Option<String>, SemanticaError> {
                    let feed_url = pyurl::urljoin(website_url, path)?;
                    let test = request_with_ssrf_guard(
                        "HEAD",
                        &feed_url,
                        None,
                        &guard,
                        RequestOptions {
                            timeout: Some(10.0),
                            ..RequestOptions::default()
                        },
                    )?;
                    if test.status_code == 200 {
                        let ct = test.headers.get("Content-Type").unwrap_or("").to_lowercase();
                        if ct.contains("rss") || ct.contains("atom") || ct.contains("xml") {
                            return Ok(Some(feed_url));
                        }
                    }
                    Ok(None)
                })();
                if let Ok(Some(url)) = probe
                    && !feed_urls.contains(&url)
                {
                    feed_urls.push(url);
                }
            }
            let mut validated = Vec::new();
            for url in feed_urls {
                let ok = request_with_ssrf_guard(
                    "GET",
                    &url,
                    None,
                    &guard,
                    RequestOptions {
                        timeout: Some(10.0),
                        ..RequestOptions::default()
                    },
                )
                .map(|r| r.status_code == 200)
                .unwrap_or(false);
                if ok {
                    validated.push(url);
                }
            }
            Ok(validated)
        })();
        match result {
            Ok(v) => v,
            Err(e) => {
                self.logger
                    .error(format!("Failed to discover feeds from {website_url}: {e}"));
                Vec::new()
            }
        }
    }

    /// `monitor_feeds(feed_urls, **options)` (`update_callback` is given separately;
    /// `start` defaults to true).
    pub fn monitor_feeds(
        &mut self,
        feed_urls: &[&str],
        options: &Kwargs,
        update_callback: Option<Box<dyn Fn(&str, &[FeedItem]) + Send + Sync>>,
    ) -> Result<&mut FeedMonitor, SemanticaError> {
        for url in feed_urls {
            self.monitor.add_feed(url, options)?;
        }
        if let Some(cb) = update_callback {
            self.monitor.set_update_callback(cb);
        }
        if options.get("start").map(kw::truthy).unwrap_or(true) {
            self.monitor.start_monitoring();
        }
        Ok(&mut self.monitor)
    }

    /// `extract_content(feed_item)`.
    pub fn extract_content(&self, feed_item: &FeedItem) -> Map<String, Value> {
        let iso = |d: &Option<PyDateTime>| match d {
            Some(d) => Value::String(d.isoformat('T')),
            None => Value::Null,
        };
        kw::obj([
            ("title", Value::String(feed_item.title.clone())),
            ("description", Value::String(feed_item.description.clone())),
            (
                "content",
                Value::String(if feed_item.content.is_empty() {
                    feed_item.description.clone()
                } else {
                    feed_item.content.clone()
                }),
            ),
            ("link", Value::String(feed_item.link.clone())),
            ("author", Value::String(feed_item.author.clone())),
            ("published", iso(&feed_item.published)),
            ("updated", iso(&feed_item.updated)),
            ("categories", json!(feed_item.categories)),
            ("guid", Value::String(feed_item.guid.clone())),
            ("metadata", Value::Object(feed_item.metadata.clone())),
        ])
    }
}
