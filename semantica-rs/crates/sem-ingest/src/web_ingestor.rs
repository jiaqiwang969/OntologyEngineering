//! Web ingestion (`semantica/ingest/web_ingestor.py`): URL fetching with retries, rate
//! limiting and robots.txt compliance, BeautifulSoup-style content extraction, sitemap and
//! domain crawling. Every fetch goes through the SSRF guard.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use fancy_regex::Regex as PyRegex;
use indexmap::IndexMap;
use regex::Regex;
use sem_utils::logging::Logger;
use sem_utils::progress_tracker::ProgressTracker;
use sem_utils::pycompat::time::PyDateTime;
use sem_utils::{SemanticaError, get_logger, get_progress_tracker};
use serde_json::{Map, Value};

use crate::config::processing_error;
use crate::ssrf::{GuardOptions, parse_bool, request_with_ssrf_guard, validate_url_for_request};
use crate::support::etree;
use crate::support::hooks::Hook;
use crate::support::html::{AttrFilter, Document, TagFilter};
use crate::support::http::{HttpAdapter, RequestOptions, Retry, Session, is_request_exception};
use crate::support::kw::{self, Kwargs};
use crate::support::pyurl;
use crate::support::pytime;
use crate::support::robotparser::RobotFileParser;

/// Replacement for `WebIngestor.ingest_url` (`patch.object(WebIngestor, "ingest_url")`).
pub static INGEST_URL: Hook<Box<dyn Fn(&str) -> Result<WebContent, SemanticaError>>> =
    Hook::new("semantica.ingest.web_ingestor.WebIngestor.ingest_url");

/// Replacement for `SitemapCrawler.parse_sitemap`.
pub static PARSE_SITEMAP: Hook<Box<dyn Fn(&str) -> Result<Vec<String>, SemanticaError>>> =
    Hook::new("semantica.ingest.web_ingestor.SitemapCrawler.parse_sitemap");

/// Replacement for `RobotsChecker.can_fetch`.
pub static ROBOTS_CAN_FETCH: Hook<Box<dyn Fn(&str) -> bool>> =
    Hook::new("semantica.ingest.web_ingestor.RobotsChecker.can_fetch");

/// Replacement for the `RobotFileParser` class the robots checker instantiates.
pub static ROBOT_FILE_PARSER: Hook<Box<dyn Fn() -> Box<dyn RobotsParser>>> =
    Hook::new("semantica.ingest.web_ingestor.RobotFileParser");

/// Replacement for `urllib.robotparser.RobotFileParser.can_fetch` (`(useragent, url)`).
pub static ROBOT_FILE_PARSER_CAN_FETCH: Hook<Box<dyn Fn(&str, &str) -> bool>> =
    Hook::new("urllib.robotparser.RobotFileParser.can_fetch");

/// `WebContent`: web content representation.
#[derive(Debug, Clone, PartialEq)]
pub struct WebContent {
    /// Source URL.
    pub url: String,
    /// Page title.
    pub title: String,
    /// Cleaned text.
    pub text: String,
    /// Original HTML.
    pub html: String,
    /// Extracted metadata.
    pub metadata: Map<String, Value>,
    /// Extracted absolute links.
    pub links: Vec<String>,
    /// Fetch time.
    pub fetched_at: PyDateTime,
    /// HTTP status code.
    pub status_code: Option<u16>,
}

impl WebContent {
    /// `WebContent(url)` with the dataclass defaults.
    pub fn new(url: &str) -> WebContent {
        WebContent {
            url: url.to_string(),
            title: String::new(),
            text: String::new(),
            html: String::new(),
            metadata: Map::new(),
            links: Vec::new(),
            fetched_at: PyDateTime::now(false),
            status_code: None,
        }
    }

    /// `WebContent(url, links=[...])`.
    pub fn with_links(url: &str, links: &[&str]) -> WebContent {
        let mut c = WebContent::new(url);
        c.links = links.iter().map(|s| s.to_string()).collect();
        c
    }
}

/// `RateLimiter`: enforces a minimum delay between requests.
#[derive(Debug, Clone, PartialEq)]
pub struct RateLimiter {
    /// Minimum delay in seconds.
    pub delay: f64,
    /// `last_request_time`.
    pub last_request_time: f64,
}

impl RateLimiter {
    /// `RateLimiter(delay=1.0)`.
    pub fn new(delay: f64) -> RateLimiter {
        RateLimiter {
            delay,
            last_request_time: 0.0,
        }
    }

    /// `wait_if_needed()`.
    pub fn wait_if_needed(&mut self) {
        let elapsed = pytime::time() - self.last_request_time;
        if elapsed < self.delay {
            pytime::sleep(self.delay - elapsed);
        }
        self.last_request_time = pytime::time();
    }
}

/// The parser interface `RobotsChecker` uses (`urllib.robotparser.RobotFileParser`).
pub trait RobotsParser: Send {
    /// `set_url(url)`.
    fn set_url(&mut self, url: &str);
    /// `read()`.
    fn read(&mut self) -> Result<(), SemanticaError>;
    /// `can_fetch(useragent, url)`.
    fn can_fetch(&self, useragent: &str, url: &str) -> bool;
}

impl RobotsParser for RobotFileParser {
    fn set_url(&mut self, url: &str) {
        RobotFileParser::set_url(self, url);
    }

    fn read(&mut self) -> Result<(), SemanticaError> {
        RobotFileParser::read(self)
    }

    fn can_fetch(&self, useragent: &str, url: &str) -> bool {
        if let Some(f) = ROBOT_FILE_PARSER_CAN_FETCH.get() {
            return f(useragent, url);
        }
        RobotFileParser::can_fetch(self, useragent, url)
    }
}

/// `RobotsChecker`: robots.txt compliance with a per-domain parser cache.
pub struct RobotsChecker {
    /// User agent checked against the rules.
    pub user_agent: String,
    parsers: HashMap<String, Box<dyn RobotsParser>>,
}

impl std::fmt::Debug for RobotsChecker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RobotsChecker")
            .field("user_agent", &self.user_agent)
            .field("domains", &self.parsers.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl RobotsChecker {
    /// `RobotsChecker(user_agent="SemanticaBot/1.0")`.
    pub fn new(user_agent: &str) -> RobotsChecker {
        RobotsChecker {
            user_agent: user_agent.to_string(),
            parsers: HashMap::new(),
        }
    }

    /// Domains with a cached parser.
    pub fn cached_domains(&self) -> Vec<String> {
        self.parsers.keys().cloned().collect()
    }

    /// `can_fetch(url)`: reads `<scheme>://<netloc>/robots.txt` once per domain; a file that
    /// cannot be read leaves the parser unchecked (which denies every URL).
    pub fn can_fetch(&mut self, url: &str) -> bool {
        if let Some(f) = ROBOTS_CAN_FETCH.get() {
            return f(url);
        }
        let (scheme, netloc) = match pyurl::urlparse(url) {
            Ok(p) => (p.scheme, p.netloc),
            Err(_) => (String::new(), String::new()),
        };
        let domain = format!("{scheme}://{netloc}");
        if !self.parsers.contains_key(&domain) {
            let robots_url = pyurl::urljoin(&domain, "/robots.txt").unwrap_or_default();
            let mut parser: Box<dyn RobotsParser> = match ROBOT_FILE_PARSER.get() {
                Some(factory) => factory(),
                None => Box::new(RobotFileParser::new("")),
            };
            parser.set_url(&robots_url);
            let _ = parser.read();
            self.parsers.insert(domain.clone(), parser);
        }
        self.parsers[&domain].can_fetch(&self.user_agent, url)
    }
}

/// `ContentExtractor`: HTML text, metadata and link extraction.
#[derive(Debug, Clone, Default)]
pub struct ContentExtractor {
    /// Configuration (unused, as in the original).
    pub config: Kwargs,
}

impl ContentExtractor {
    /// `ContentExtractor(**config)`.
    pub fn new(config: Kwargs) -> ContentExtractor {
        ContentExtractor { config }
    }

    /// `extract_text(html_content)`: text without scripts and styles, whitespace cleaned.
    pub fn extract_text(&self, html_content: &str) -> String {
        let mut soup = Document::parse(html_content);
        for element in soup.find_all(soup.root(), &TagFilter::names(&["script", "style"])) {
            soup.decompose(element);
        }
        let text = soup.get_text(soup.root());
        let mut chunks: Vec<String> = Vec::new();
        for line in pytime::splitlines(&text) {
            let line = sem_utils::pycompat::fmt::strip(line);
            for phrase in line.split("  ") {
                let phrase = sem_utils::pycompat::fmt::strip(phrase);
                if !phrase.is_empty() {
                    chunks.push(phrase.to_string());
                }
            }
        }
        chunks.join(" ")
    }

    /// `extract_metadata(html_content, url=None)`: title, meta tags and Open Graph data.
    pub fn extract_metadata(&self, html_content: &str, url: Option<&str>) -> Map<String, Value> {
        let mut metadata = Map::new();
        if let Some(u) = url
            && !u.is_empty()
        {
            metadata.insert("url".to_string(), Value::String(u.to_string()));
        }
        let soup = Document::parse(html_content);
        if let Some(title) = soup.find(soup.root(), &TagFilter::name("title")) {
            metadata.insert(
                "title".to_string(),
                Value::String(sem_utils::pycompat::fmt::strip(&soup.get_text(title)).to_string()),
            );
        }
        for meta in soup.find_all(soup.root(), &TagFilter::name("meta")) {
            let name = soup
                .attr(meta, "name")
                .filter(|s| !s.is_empty())
                .or_else(|| soup.attr(meta, "property"));
            let content = soup.attr(meta, "content");
            if let (Some(name), Some(content)) = (name, content)
                && !name.is_empty()
                && !content.is_empty()
            {
                metadata.insert(name.to_lowercase(), Value::String(content.to_string()));
            }
        }
        let mut og = Map::new();
        let filter = TagFilter::name("meta").attr(
            "property",
            AttrFilter::Regex(Regex::new("^og:").expect("valid regex")),
        );
        for meta in soup.find_all(soup.root(), &filter) {
            let property = soup.attr(meta, "property").unwrap_or("").replace("og:", "");
            let content = soup.attr(meta, "content");
            if !property.is_empty()
                && let Some(content) = content
                && !content.is_empty()
            {
                og.insert(property, Value::String(content.to_string()));
            }
        }
        if !og.is_empty() {
            metadata.insert("og".to_string(), Value::Object(og));
        }
        metadata
    }

    /// `extract_links(html_content, base_url=None)`: absolute HTTP(S) links.
    pub fn extract_links(
        &self,
        html_content: &str,
        base_url: Option<&str>,
    ) -> Result<Vec<String>, SemanticaError> {
        let soup = Document::parse(html_content);
        let mut links = Vec::new();
        for a in soup.find_all(soup.root(), &TagFilter::name("a").attr("href", AttrFilter::Present)) {
            let href = soup.attr(a, "href").unwrap_or("");
            let link = match base_url {
                Some(base) if !base.is_empty() => pyurl::urljoin(base, href)?,
                _ => href.to_string(),
            };
            let parsed = pyurl::urlparse(&link)?;
            if parsed.scheme == "http" || parsed.scheme == "https" {
                links.push(link);
            }
        }
        Ok(links)
    }
}

const SITEMAP_NS: &str = "http://www.sitemaps.org/schemas/sitemap/0.9";

/// `SitemapCrawler`: XML sitemap parsing.
#[derive(Debug, Clone)]
pub struct SitemapCrawler {
    /// Configuration.
    pub config: Kwargs,
    /// `allow_private_ips`.
    pub allow_private_ips: bool,
    logger: Logger,
}

impl SitemapCrawler {
    /// `SitemapCrawler(**config)` (`allow_private_ips` parsed with [`parse_bool`]).
    pub fn new(config: Kwargs) -> Result<SitemapCrawler, SemanticaError> {
        let allow_private_ips = parse_bool(
            config.get("allow_private_ips").unwrap_or(&Value::Null),
            false,
        )?;
        Ok(SitemapCrawler {
            config,
            allow_private_ips,
            logger: get_logger("sitemap_crawler"),
        })
    }

    fn fetch_root(&self, url: &str) -> Result<etree::Element, SemanticaError> {
        let mut options = RequestOptions::default();
        options.timeout = Some(30.0);
        let response = request_with_ssrf_guard(
            "GET",
            url,
            None,
            &GuardOptions::allow_private_ips(self.allow_private_ips),
            options,
        )?;
        response.raise_for_status()?;
        etree::fromstring_bytes(&response.content)
    }

    /// `parse_sitemap(sitemap_url)`: the `<loc>` URLs of a sitemap.
    pub fn parse_sitemap(&self, sitemap_url: &str) -> Result<Vec<String>, SemanticaError> {
        if let Some(f) = PARSE_SITEMAP.get() {
            return f(sitemap_url);
        }
        let result = (|| -> Result<Vec<String>, SemanticaError> {
            let root = self.fetch_root(sitemap_url)?;
            let mut ns = IndexMap::new();
            ns.insert("sitemap".to_string(), SITEMAP_NS.to_string());
            let mut urls = Vec::new();
            for url_elem in root.findall_ns(".//sitemap:url", &ns)? {
                if let Some(loc) = url_elem.find_ns("sitemap:loc", &ns)?
                    && let Some(text) = loc.text.as_deref().filter(|t| !t.is_empty())
                {
                    urls.push(sem_utils::pycompat::fmt::strip(text).to_string());
                }
            }
            if urls.is_empty() {
                for url_elem in root.findall(".//url") {
                    if let Some(loc) = url_elem.find("loc")
                        && let Some(text) = loc.text.as_deref().filter(|t| !t.is_empty())
                    {
                        urls.push(sem_utils::pycompat::fmt::strip(text).to_string());
                    }
                }
            }
            self.logger.debug(format!(
                "Extracted {} URL(s) from sitemap: {sitemap_url}",
                urls.len()
            ));
            Ok(urls)
        })();
        match result {
            Ok(urls) => Ok(urls),
            Err(e) if e.is_validation_error() => Err(e),
            Err(e) => {
                self.logger
                    .error(format!("Failed to parse sitemap {sitemap_url}: {e}"));
                Err(processing_error(format!("Failed to parse sitemap: {e}")))
            }
        }
    }

    /// `crawl_sitemap_index(index_url)`: URLs of every sitemap a sitemap index references.
    pub fn crawl_sitemap_index(&self, index_url: &str) -> Result<Vec<String>, SemanticaError> {
        let result = (|| -> Result<Vec<String>, SemanticaError> {
            let root = self.fetch_root(index_url)?;
            let mut ns = IndexMap::new();
            ns.insert("sitemap".to_string(), SITEMAP_NS.to_string());
            let mut all_urls = Vec::new();
            for elem in root.findall_ns(".//sitemap:sitemap", &ns)? {
                if let Some(loc) = elem.find_ns("sitemap:loc", &ns)?
                    && let Some(text) = loc.text.as_deref().filter(|t| !t.is_empty())
                {
                    let url = sem_utils::pycompat::fmt::strip(text).to_string();
                    all_urls.extend(self.parse_sitemap(&url)?);
                }
            }
            if all_urls.is_empty() {
                for elem in root.findall(".//sitemap") {
                    if let Some(loc) = elem.find("loc")
                        && let Some(text) = loc.text.as_deref().filter(|t| !t.is_empty())
                    {
                        let url = sem_utils::pycompat::fmt::strip(text).to_string();
                        all_urls.extend(self.parse_sitemap(&url)?);
                    }
                }
            }
            self.logger.debug(format!(
                "Extracted {} URL(s) from sitemap index: {index_url}",
                all_urls.len()
            ));
            Ok(all_urls)
        })();
        match result {
            Ok(urls) => Ok(urls),
            Err(e) if e.is_validation_error() => Err(e),
            Err(e) => {
                self.logger
                    .error(format!("Failed to crawl sitemap index {index_url}: {e}"));
                Err(processing_error(format!("Failed to crawl sitemap index: {e}")))
            }
        }
    }
}

/// The named parameters of `WebIngestor.__init__` (everything else is `**kwargs`).
#[derive(Debug, Clone, PartialEq)]
pub struct WebIngestorOptions {
    /// `user_agent`.
    pub user_agent: Option<String>,
    /// `delay`.
    pub delay: f64,
    /// `respect_robots`.
    pub respect_robots: bool,
    /// `max_retries`.
    pub max_retries: i64,
    /// `backoff_factor`.
    pub backoff_factor: f64,
    /// `timeout` (accepted but, as in the original, unused: requests use `config["timeout"]`).
    pub timeout: i64,
    /// `allow_private_ips` (a bool or a boolean-like string).
    pub allow_private_ips: Value,
    /// `config`.
    pub config: Option<Kwargs>,
    /// `**kwargs`.
    pub kwargs: Kwargs,
}

impl Default for WebIngestorOptions {
    fn default() -> Self {
        WebIngestorOptions {
            user_agent: Some("SemanticaBot/1.0".to_string()),
            delay: 1.0,
            respect_robots: true,
            max_retries: 3,
            backoff_factor: 1.0,
            timeout: 30,
            allow_private_ips: Value::Bool(false),
            config: None,
            kwargs: Kwargs::new(),
        }
    }
}

impl WebIngestorOptions {
    /// Binds a `**kwargs` call (`WebIngestor(**config)`) to the named parameters.
    pub fn from_kwargs(kwargs: &Kwargs) -> WebIngestorOptions {
        let mut options = WebIngestorOptions::default();
        for (k, v) in kwargs {
            match k.as_str() {
                "user_agent" => options.user_agent = v.as_str().map(str::to_string),
                "delay" => options.delay = v.as_f64().unwrap_or(options.delay),
                "respect_robots" => options.respect_robots = kw::truthy(v),
                "max_retries" => options.max_retries = v.as_i64().unwrap_or(options.max_retries),
                "backoff_factor" => {
                    options.backoff_factor = v.as_f64().unwrap_or(options.backoff_factor)
                }
                "timeout" => options.timeout = v.as_i64().unwrap_or(options.timeout),
                "allow_private_ips" => options.allow_private_ips = v.clone(),
                "config" => options.config = v.as_object().cloned(),
                _ => {
                    options.kwargs.insert(k.clone(), v.clone());
                }
            }
        }
        options
    }
}

/// `WebIngestor`: single URL fetching, sitemap crawling and domain crawling.
pub struct WebIngestor {
    /// Merged configuration (`allow_private_ips` normalised to a bool).
    pub config: Kwargs,
    /// `allow_private_ips`.
    pub allow_private_ips: bool,
    /// HTTP session with the retry adapter mounted.
    pub session: Session,
    /// Rate limiter.
    pub rate_limiter: RateLimiter,
    /// Content extractor.
    pub content_extractor: ContentExtractor,
    /// Robots checker (`None` when `respect_robots=False`).
    pub robots_checker: Option<RobotsChecker>,
    /// Progress tracker.
    pub progress_tracker: Arc<ProgressTracker>,
    logger: Logger,
}

impl std::fmt::Debug for WebIngestor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebIngestor")
            .field("config", &self.config)
            .field("allow_private_ips", &self.allow_private_ips)
            .field("rate_limiter", &self.rate_limiter)
            .finish()
    }
}

impl WebIngestor {
    /// `WebIngestor(...)`.
    pub fn new(options: WebIngestorOptions) -> Result<WebIngestor, SemanticaError> {
        let logger = get_logger("web_ingestor");
        let mut config = kw::merged(options.config.as_ref(), &options.kwargs);
        let raw_allow = config
            .get("allow_private_ips")
            .cloned()
            .unwrap_or(options.allow_private_ips.clone());
        let allow_private_ips = parse_bool(&raw_allow, false)?;
        config.insert("allow_private_ips".to_string(), Value::Bool(allow_private_ips));

        let mut session = Session::new();
        let user_agent = match &options.user_agent {
            Some(ua) if !ua.is_empty() => ua.clone(),
            _ => kw::get_str(&config, "user_agent").unwrap_or_else(|| "SemanticaBot/1.0".to_string()),
        };
        session.headers.insert("User-Agent", user_agent.clone());
        let retry = Retry {
            total: Some(options.max_retries),
            backoff_factor: options.backoff_factor,
            status_forcelist: vec![429, 500, 502, 503, 504],
        };
        session.mount("http://", HttpAdapter::with_retries(retry.clone()));
        session.mount("https://", HttpAdapter::with_retries(retry));

        let ingestor = WebIngestor {
            rate_limiter: RateLimiter::new(options.delay),
            content_extractor: ContentExtractor::new(config.clone()),
            robots_checker: if options.respect_robots {
                Some(RobotsChecker::new(&user_agent))
            } else {
                None
            },
            config,
            allow_private_ips,
            session,
            progress_tracker: get_progress_tracker(),
            logger,
        };
        ingestor.logger.debug(format!(
            "Web ingestor initialized: user_agent={user_agent}, delay={}, respect_robots={}, allow_private_ips={}",
            sem_utils::pycompat::fmt::float_repr(options.delay),
            if options.respect_robots { "True" } else { "False" },
            if allow_private_ips { "True" } else { "False" }
        ));
        Ok(ingestor)
    }

    /// `WebIngestor(**kwargs)`.
    pub fn from_kwargs(kwargs: &Kwargs) -> Result<WebIngestor, SemanticaError> {
        WebIngestor::new(WebIngestorOptions::from_kwargs(kwargs))
    }

    /// `ingest_url(url, timeout=None, **options)`.
    pub fn ingest_url(&mut self, url: &str, timeout: Option<f64>) -> Result<WebContent, SemanticaError> {
        if let Some(f) = INGEST_URL.get() {
            return f(url);
        }
        let tracking_id = self.progress_tracker.start_tracking(
            Some(url),
            Some("ingest"),
            Some("WebIngestor"),
            &format!("URL: {url}"),
            None,
        );
        let result = self.ingest_url_inner(url, timeout, &tracking_id);
        if let Err(e) = &result {
            self.progress_tracker
                .stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
        }
        result
    }

    fn ingest_url_inner(
        &mut self,
        url: &str,
        timeout: Option<f64>,
        tracking_id: &str,
    ) -> Result<WebContent, SemanticaError> {
        validate_url_for_request(url, self.allow_private_ips)?;
        if let Some(checker) = self.robots_checker.as_mut()
            && !checker.can_fetch(url)
        {
            self.logger.warning(format!("URL {url} blocked by robots.txt"));
            return Err(processing_error(format!("URL blocked by robots.txt: {url}")));
        }
        self.rate_limiter.wait_if_needed();

        let request_timeout = match timeout {
            Some(t) if t != 0.0 => t,
            _ => kw::get_f64(&self.config, "timeout", 30.0),
        };
        let mut options = RequestOptions::default();
        options.timeout = Some(request_timeout);
        let guard = GuardOptions::allow_private_ips(self.allow_private_ips);
        let response = match request_with_ssrf_guard("GET", url, Some(&mut self.session), &guard, options)
            .and_then(|r| {
                r.raise_for_status()?;
                Ok(r)
            }) {
            Ok(r) => r,
            Err(e) if e.is_validation_error() => return Err(e),
            Err(e) if is_request_exception(&e) => {
                self.progress_tracker
                    .stop_tracking(tracking_id, Some("failed"), &e.to_string(), None);
                self.logger.error(format!("Failed to fetch URL {url}: {e}"));
                return Err(processing_error(format!("Failed to fetch URL: {e}")));
            }
            Err(e) => return Err(e),
        };

        let html_content = response.text();
        let mut web_content = self.extract_content(&html_content, Some(url))?;
        web_content.status_code = Some(response.status_code);
        self.progress_tracker.stop_tracking(
            tracking_id,
            Some("completed"),
            &format!("Ingested {url} ({})", response.status_code),
            None,
        );
        self.logger.debug(format!(
            "Ingested URL: {url}, status={}, text_length={}, links={}",
            response.status_code,
            web_content.text.chars().count(),
            web_content.links.len()
        ));
        Ok(web_content)
    }

    /// `crawl_sitemap(sitemap_url, max_urls=None, fail_fast=False, **filters)`.
    pub fn crawl_sitemap(
        &mut self,
        sitemap_url: &str,
        max_urls: Option<i64>,
        fail_fast: bool,
        filters: &Kwargs,
    ) -> Result<Vec<WebContent>, SemanticaError> {
        let tracking_id = self.progress_tracker.start_tracking(
            Some(sitemap_url),
            Some("ingest"),
            Some("WebIngestor"),
            &format!("Sitemap: {sitemap_url}"),
            None,
        );
        let result = self.crawl_sitemap_inner(sitemap_url, max_urls, fail_fast, filters, &tracking_id);
        if let Err(e) = &result {
            self.progress_tracker
                .stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
        }
        result
    }

    fn crawl_sitemap_inner(
        &mut self,
        sitemap_url: &str,
        max_urls: Option<i64>,
        fail_fast: bool,
        filters: &Kwargs,
        tracking_id: &str,
    ) -> Result<Vec<WebContent>, SemanticaError> {
        let crawler = SitemapCrawler::new(self.config.clone())?;
        let mut urls = match crawler.parse_sitemap(sitemap_url) {
            Ok(u) => u,
            Err(e) => match crawler.crawl_sitemap_index(sitemap_url) {
                Ok(u) => u,
                Err(_) => return Err(processing_error(format!("Failed to parse sitemap: {e}"))),
            },
        };
        self.logger
            .debug(format!("Found {} URL(s) in sitemap: {sitemap_url}", urls.len()));
        if !filters.is_empty() {
            urls = self._apply_url_filters(&urls, filters)?;
            self.logger.debug(format!("After filtering: {} URL(s)", urls.len()));
        }
        let max_urls = max_urls
            .filter(|m| *m != 0)
            .or_else(|| kw::get_i64(filters, "max_urls").filter(|m| *m != 0))
            .or_else(|| kw::get_i64(&self.config, "max_urls").filter(|m| *m != 0));
        self.progress_tracker.update_tracking(
            tracking_id,
            None,
            &format!("Crawling {} URLs", urls.len()),
        );
        let mut contents = Vec::new();
        for (idx, url) in urls.iter().enumerate() {
            if let Some(m) = max_urls
                && idx as i64 >= m
            {
                self.logger
                    .debug(format!("Reached max_urls limit ({m}), stopping crawl"));
                break;
            }
            match self.ingest_url(url, None) {
                Ok(c) => {
                    contents.push(c);
                    self.logger.debug(format!(
                        "Crawled URL {}/{}: {url}",
                        idx + 1,
                        max_urls.map(|m| (m as usize).min(urls.len())).unwrap_or(urls.len())
                    ));
                }
                Err(e) => {
                    self.logger.error(format!("Failed to crawl URL {url}: {e}"));
                    if fail_fast || kw::get_truthy(&self.config, "fail_fast", false) {
                        return Err(processing_error(format!("Failed to crawl URL: {e}")));
                    }
                }
            }
        }
        self.progress_tracker.stop_tracking(
            tracking_id,
            Some("completed"),
            &format!("Crawled {}/{} URLs", contents.len(), urls.len()),
            None,
        );
        self.logger.info(format!(
            "Crawled {}/{} URL(s) from sitemap: {sitemap_url}",
            contents.len(),
            urls.len()
        ));
        Ok(contents)
    }

    /// `crawl_domain(domain, max_pages=100, fail_fast=False, **options)`: breadth-first crawl
    /// of same-domain links.
    pub fn crawl_domain(
        &mut self,
        domain: &str,
        max_pages: usize,
        fail_fast: bool,
    ) -> Result<Vec<WebContent>, SemanticaError> {
        let domain = if domain.starts_with("http://") || domain.starts_with("https://") {
            domain.to_string()
        } else {
            format!("https://{domain}")
        };
        let mut visited: HashSet<String> = HashSet::new();
        let mut to_visit: Vec<String> = vec![domain.clone()];
        let mut contents: Vec<WebContent> = Vec::new();
        self.logger.debug(format!(
            "Starting domain crawl: {domain}, max_pages={max_pages}"
        ));
        while !to_visit.is_empty() && contents.len() < max_pages {
            let url = to_visit.remove(0);
            if visited.contains(&url) {
                continue;
            }
            visited.insert(url.clone());
            let result = self.ingest_url(&url, None).and_then(|content| {
                let links = content.links.clone();
                contents.push(content);
                let domain_netloc = pyurl::urlparse(&domain)?.netloc;
                for link in &links {
                    let netloc = pyurl::urlparse(link)?.netloc;
                    if netloc == domain_netloc && !visited.contains(link) && !to_visit.contains(link) {
                        to_visit.push(link.clone());
                    }
                }
                Ok(())
            });
            match result {
                Ok(()) => {
                    self.logger.debug(format!(
                        "Crawled page {}/{max_pages}: {url}, queued={}",
                        contents.len(),
                        to_visit.len()
                    ));
                }
                Err(e) => {
                    self.logger.error(format!("Failed to crawl URL {url}: {e}"));
                    if fail_fast || kw::get_truthy(&self.config, "fail_fast", false) {
                        return Err(processing_error(format!("Failed to crawl URL: {e}")));
                    }
                }
            }
        }
        self.logger.info(format!(
            "Domain crawl completed: {} page(s) crawled, {} URL(s) visited",
            contents.len(),
            visited.len()
        ));
        Ok(contents)
    }

    /// `extract_content(html_content, url=None)`.
    pub fn extract_content(&self, html_content: &str, url: Option<&str>) -> Result<WebContent, SemanticaError> {
        let text = self.content_extractor.extract_text(html_content);
        let metadata = self.content_extractor.extract_metadata(html_content, url);
        let links = self.content_extractor.extract_links(html_content, url)?;
        let title = match metadata.get("title") {
            Some(Value::String(s)) => s.clone(),
            _ => String::new(),
        };
        let mut content = WebContent::new(url.unwrap_or(""));
        content.title = title;
        content.text = text;
        content.html = html_content.to_string();
        content.metadata = metadata;
        content.links = links;
        Ok(content)
    }

    /// `_apply_url_filters(urls, filters)`: `pattern` (regex search), `domains` (substring),
    /// `exclude_pattern`.
    pub fn _apply_url_filters(&self, urls: &[String], filters: &Kwargs) -> Result<Vec<String>, SemanticaError> {
        let compile = |p: &str| {
            PyRegex::new(p).map_err(|e| SemanticaError::Other {
                module: "re".to_string(),
                type_name: "error".to_string(),
                message: e.to_string(),
            })
        };
        let mut filtered: Vec<String> = urls.to_vec();
        if let Some(pattern) = kw::get_str(filters, "pattern") {
            let re = compile(&pattern)?;
            filtered.retain(|u| re.is_match(u).unwrap_or(false));
        }
        if let Some(domains) = kw::get_str_list(filters, "domains") {
            filtered.retain(|u| domains.iter().any(|d| u.contains(d.as_str())));
        }
        if let Some(pattern) = kw::get_str(filters, "exclude_pattern") {
            let re = compile(&pattern)?;
            filtered.retain(|u| !re.is_match(u).unwrap_or(false));
        }
        Ok(filtered)
    }
}
