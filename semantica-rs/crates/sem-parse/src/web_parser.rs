//! Web content parsing (`semantica/parse/web_parser.py`).
//!
//! - [`WebParser`] routes HTML to [`HTMLParser`] and XML to [`XMLParser`], extracts text and
//!   links, and renders JavaScript through a [`JavaScriptRenderer`].
//! - [`HTMLContentParser`] is the original's `HTMLParser` subclass ([`Deref`]s to
//!   [`HTMLParser`]): `clean_html` and `extract_structured_content` (which, as in the
//!   original, always fails with `AttributeError` because `HTMLData` has no `get`).
//! - [`JavaScriptRenderer`] keeps the original's Selenium control flow. Selenium is not
//!   available to the port (nor in the original's reference environment); with
//!   `use_selenium` the renderer drives a [`WebDriver`], by default the built-in
//!   [`StaticWebDriver`], which loads `file://` pages from disk and `http(s)://` pages over
//!   HTTP and returns the document as an HTML5 browser parses and serialises it (html5ever),
//!   without executing scripts.

mod css;
mod html5dom;

use std::io::Write as _;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};

use sem_utils::logging::{Logger, get_logger};
use sem_utils::pycompat::fmt::{is_space, truthy};
use sem_utils::{ProcessingError, SemanticaError, ValidationError};

use crate::common::{
    DecodeErrors, Options, PResult, Source, bind_kwarg, merge_config, opt_truthy, py_path_str, py_read_text,
    sub_config,
};
use crate::html_parser::HTMLParser;
use crate::methods::ParseOutput;
use crate::soup::{NodeData, Soup, StringKind};
use crate::xml_parser::XMLParser;

pub(crate) use html5dom::{document_element, outer_html, parse_html};

/// `WebParser(config=None, **kwargs)`: web content parsing handler.
#[derive(Debug)]
pub struct WebParser {
    /// `self.logger` (`semantica.web_parser`).
    pub logger: Logger,
    /// `self.config` (`config` updated with the keyword arguments).
    pub config: Options,
    /// `self.html_parser` (`HTMLParser(**config.get("html", {}))`).
    pub html_parser: HTMLParser,
    /// `self.xml_parser` (`XMLParser(**config.get("xml", {}))`).
    pub xml_parser: XMLParser,
    /// `self.js_renderer` (`JavaScriptRenderer(**config.get("js", {}))`).
    pub js_renderer: JavaScriptRenderer,
}

impl Default for WebParser {
    fn default() -> Self {
        WebParser::new(None, Options::new())
    }
}

impl WebParser {
    /// `WebParser(config=None, **kwargs)`.
    pub fn new(config: Option<Options>, kwargs: Options) -> Self {
        let config = merge_config(config, kwargs);
        WebParser {
            logger: get_logger("web_parser"),
            html_parser: HTMLParser::new(sub_config(&config, "html")),
            xml_parser: XMLParser::new(sub_config(&config, "xml")),
            js_renderer: JavaScriptRenderer::new(sub_config(&config, "js")),
            config,
        }
    }

    /// `parse_web_content(content, content_type="html", base_url=None, **options)`:
    /// [`ParseOutput::Html`] for `"html"`, [`ParseOutput::Xml`] for `"xml"`, otherwise
    /// `ValidationError("Unsupported content type: ...")`.
    pub fn parse_web_content(
        &self,
        content: impl Into<Source>,
        content_type: Option<&str>,
        base_url: Option<&str>,
        options: &Options,
    ) -> PResult<ParseOutput> {
        match content_type.unwrap_or("html") {
            "html" => Ok(ParseOutput::Html(self.html_parser.parse(content, base_url, options)?)),
            "xml" => Ok(ParseOutput::Xml(self.xml_parser.parse(content, options)?)),
            other => Err(ValidationError::new(format!("Unsupported content type: {other}")).into()),
        }
    }

    /// `extract_text(web_content, **options)`: clean text (`clean`, default true), rendering
    /// JavaScript first when `render_javascript` is set.
    pub fn extract_text(&self, web_content: impl Into<Source>, options: &Options) -> PResult<String> {
        let clean = opt_truthy(options, "clean", true);
        if opt_truthy(options, "render_javascript", false) {
            let html_content = self.js_renderer.render_page(web_content, options)?;
            self.html_parser.extract_text(html_content, Some(clean))
        } else {
            self.html_parser.extract_text(web_content, Some(clean))
        }
    }

    /// `extract_links(web_content, base_url=None)`.
    pub fn extract_links(
        &self,
        web_content: impl Into<Source>,
        base_url: Option<&str>,
    ) -> PResult<Vec<Map<String, Value>>> {
        self.html_parser.extract_links(web_content, base_url)
    }

    /// `render_javascript(html_content, **options)`: [`JavaScriptRenderer::render_page`].
    pub fn render_javascript(&self, html_content: impl Into<Source>, options: &Options) -> PResult<String> {
        self.js_renderer.render_page(html_content, options)
    }
}

/// `HTMLContentParser(**config)`: the original's `HTMLParser` subclass for structured
/// content and HTML cleaning.
#[derive(Debug, Clone, Default)]
pub struct HTMLContentParser {
    /// The `HTMLParser` part of the object (every `HTMLParser` method is available through
    /// `Deref`).
    pub parser: HTMLParser,
}

impl Deref for HTMLContentParser {
    type Target = HTMLParser;

    fn deref(&self) -> &HTMLParser {
        &self.parser
    }
}

/// `open(path, "r", encoding="utf-8", errors="ignore").read()` for a file argument, the
/// string itself otherwise.
fn load_html(source: &Source) -> PResult<String> {
    match source.file_path()? {
        Some(path) => py_read_text(&path, "utf-8", DecodeErrors::Ignore),
        None => Ok(source.text().unwrap_or_default().to_string()),
    }
}

impl HTMLContentParser {
    /// `HTMLContentParser(**config)`.
    pub fn new(config: Options) -> Self {
        HTMLContentParser {
            parser: HTMLParser::new(config),
        }
    }

    /// `extract_structured_content(html_content, **options)`.
    ///
    /// The original calls `parsed.get("tables", [])` on the `HTMLData` returned by `parse`,
    /// which has no `get`: after parsing, this always raises
    /// `AttributeError: 'HTMLData' object has no attribute 'get'`, as in the original.
    pub fn extract_structured_content(
        &self,
        html_content: impl Into<Source>,
        options: &Options,
    ) -> PResult<Map<String, Value>> {
        let source: Source = html_content.into();
        let (base_url, rest) = bind_kwarg(options, "base_url");
        let base_url = base_url.and_then(|v| v.as_str().map(str::to_string));
        let parsed = ParseOutput::Html(self.parse(source.clone(), base_url.as_deref(), &rest)?);
        let tables = parsed.get("tables")?.cloned().unwrap_or_else(|| json!([]));
        let forms = parsed.get("forms")?.cloned().unwrap_or_else(|| json!([]));
        let mut structure = Map::new();
        structure.insert("headings".to_string(), json!([]));
        structure.insert("paragraphs".to_string(), json!([]));
        structure.insert("lists".to_string(), json!([]));
        structure.insert("tables".to_string(), tables);
        structure.insert("forms".to_string(), forms);
        let soup = Soup::parse(&load_html(&source)?);
        let root = soup.root();
        let mut headings = Vec::new();
        for level in 1..=6 {
            let name = format!("h{level}");
            for heading in soup.find_all_named(root, &[name.as_str()]) {
                headings.push(json!({
                    "level": level,
                    "text": sem_utils::pycompat::fmt::strip(&soup.get_text(heading)),
                    "id": crate::html_parser::attr_json(soup.get(heading, "id")),
                }));
            }
        }
        structure.insert("headings".to_string(), Value::Array(headings));
        let paragraphs: Vec<Value> = soup
            .find_all_named(root, &["p"])
            .into_iter()
            .map(|p| sem_utils::pycompat::fmt::strip(&soup.get_text(p)).to_string())
            .filter(|t| !t.is_empty())
            .map(Value::String)
            .collect();
        structure.insert("paragraphs".to_string(), Value::Array(paragraphs));
        let mut lists = Vec::new();
        for list in soup.find_all_named(root, &["ul", "ol"]) {
            let items: Vec<Value> = soup
                .find_all_named(list, &["li"])
                .into_iter()
                .map(|li| Value::String(sem_utils::pycompat::fmt::strip(&soup.get_text(li)).to_string()))
                .collect();
            if !items.is_empty() {
                lists.push(json!({"type": soup.name(list), "items": items}));
            }
        }
        structure.insert("lists".to_string(), Value::Array(lists));
        Ok(structure)
    }

    /// `clean_html(html_content, **options)`: remove scripts/styles (`remove_scripts`,
    /// default true) and comments (`remove_comments`, true), normalise whitespace outside
    /// `pre`/`code` (`normalize_whitespace`, true), and return `str(soup)`.
    pub fn clean_html(&self, html_content: impl Into<Source>, options: &Options) -> PResult<String> {
        let source: Source = html_content.into();
        let html_string = load_html(&source)?;
        let mut soup = Soup::parse(&html_string);
        let root = soup.root();
        if opt_truthy(options, "remove_scripts", true) {
            for element in soup.find_all_named(root, &["script", "style", "noscript"]) {
                soup.decompose(element);
            }
        }
        if opt_truthy(options, "remove_comments", true) {
            let comments = soup.find_all(root, |s, n| matches!(s.string(n), Some((_, StringKind::Comment))));
            for comment in comments {
                soup.extract(comment);
            }
        }
        if opt_truthy(options, "normalize_whitespace", true) {
            let strings = soup.find_all(root, |s, n| matches!(s.node(n).data, NodeData::Text { .. }));
            for elem in strings {
                let parent_name = soup.parent(elem).and_then(|p| soup.name(p)).unwrap_or_default();
                if parent_name == "pre" || parent_name == "code" {
                    continue;
                }
                let text = soup.string(elem).map(|(t, _)| t.to_string()).unwrap_or_default();
                let normalized: Vec<&str> = text.split(is_space).filter(|w| !w.is_empty()).collect();
                soup.replace_with_text(elem, &normalized.join(" "));
            }
        }
        Ok(soup.decode(root))
    }
}

// ----------------------------------------------------------------------------- WebDriver

/// The WebDriver operations the renderer uses (Selenium's `webdriver.Chrome` in the
/// original).
pub trait WebDriver: Send {
    /// `driver.get(url)`.
    fn get(&mut self, url: &str) -> PResult<()>;
    /// `driver.page_source`.
    fn page_source(&mut self) -> PResult<String>;
    /// `driver.execute_script(script)`.
    fn execute_script(&mut self, script: &str) -> PResult<Value>;
    /// `driver.find_element(By.CSS_SELECTOR, selector)` succeeds (`Ok(false)` when nothing
    /// matches; invalid selectors are errors).
    fn has_css_element(&mut self, selector: &str) -> PResult<bool>;
    /// `driver.quit()`.
    fn quit(&mut self) {}
}

/// Fetches `http(s)://` pages for the [`StaticWebDriver`].
pub trait HttpFetcher: Send + Sync {
    /// The page body for `url`.
    fn fetch(&self, url: &str) -> PResult<String>;
}

/// [`HttpFetcher`] over `reqwest` (blocking).
#[derive(Debug, Clone, Default)]
pub struct ReqwestFetcher;

impl HttpFetcher for ReqwestFetcher {
    fn fetch(&self, url: &str) -> PResult<String> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| webdriver_error(&format!("unknown error: {e}")))?;
        let response = client
            .get(url)
            .send()
            .map_err(|e| webdriver_error(&format!("unknown error: net::ERR_FAILED ({e})")))?;
        response
            .text()
            .map_err(|e| webdriver_error(&format!("unknown error: {e}")))
    }
}

/// `selenium.common.exceptions.WebDriverException` (`str(e)` is `"Message: <msg>\n"`).
fn webdriver_error(message: &str) -> SemanticaError {
    SemanticaError::Other {
        module: "selenium.common.exceptions".to_string(),
        type_name: "WebDriverException".to_string(),
        message: format!("Message: {message}\n"),
    }
}

/// A WebDriver without a browser: pages are loaded from `file://` paths or fetched over
/// HTTP, parsed with the HTML5 algorithm (html5ever) and serialised as `page_source`.
/// Scripts are not executed, so the DOM is the page as delivered.
pub struct StaticWebDriver {
    fetcher: Arc<dyn HttpFetcher>,
    page: Option<String>,
}

impl std::fmt::Debug for StaticWebDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StaticWebDriver").field("page", &self.page.is_some()).finish()
    }
}

impl StaticWebDriver {
    /// A driver fetching `http(s)://` pages with `fetcher`.
    pub fn new(fetcher: Arc<dyn HttpFetcher>) -> Self {
        StaticWebDriver { fetcher, page: None }
    }

    fn document(&self) -> PResult<html5dom::Dom> {
        let html = self.page.clone().unwrap_or_default();
        Ok(parse_html(&html))
    }
}

/// Decode a file loaded as a page: UTF-8 (a BOM is skipped), invalid bytes replaced.
fn decode_page(bytes: &[u8]) -> String {
    let body = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    String::from_utf8_lossy(body).into_owned()
}

/// Percent-decode a `file://` URL path.
fn percent_decode(path: &str) -> Vec<u8> {
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len() + 0
            && let (Some(h), Some(l)) = (
                (bytes[i + 1] as char).to_digit(16),
                bytes.get(i + 2).and_then(|b| (*b as char).to_digit(16)),
            )
        {
            out.push((h * 16 + l) as u8);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

impl WebDriver for StaticWebDriver {
    fn get(&mut self, url: &str) -> PResult<()> {
        let lower = url.to_ascii_lowercase();
        let html = if let Some(path) = url.strip_prefix("file://") {
            let path = String::from_utf8_lossy(&percent_decode(path)).into_owned();
            let bytes = std::fs::read(&path).map_err(|_| webdriver_error("unknown error: net::ERR_FILE_NOT_FOUND"))?;
            decode_page(&bytes)
        } else if lower.starts_with("http://") || lower.starts_with("https://") {
            self.fetcher.fetch(url)?
        } else if lower == "about:blank" {
            String::new()
        } else {
            return Err(webdriver_error(&format!("invalid argument: unsupported URL {url}")));
        };
        self.page = Some(html);
        Ok(())
    }

    fn page_source(&mut self) -> PResult<String> {
        let dom = self.document()?;
        Ok(document_element(&dom).map(|e| outer_html(&e)).unwrap_or_default())
    }

    fn execute_script(&mut self, script: &str) -> PResult<Value> {
        let normalized: String = script.split_whitespace().collect::<Vec<_>>().join(" ");
        let normalized = normalized.trim_end_matches(';');
        match normalized {
            "return document.readyState" => Ok(Value::String("complete".to_string())),
            _ => Err(SemanticaError::Other {
                module: "selenium.common.exceptions".to_string(),
                type_name: "JavascriptException".to_string(),
                message: "Message: javascript error: script execution is not supported by the static WebDriver\n"
                    .to_string(),
            }),
        }
    }

    fn has_css_element(&mut self, selector: &str) -> PResult<bool> {
        let list = css::SelectorList::parse(selector).ok_or_else(|| SemanticaError::Other {
            module: "selenium.common.exceptions".to_string(),
            type_name: "InvalidSelectorException".to_string(),
            message: "Message: invalid selector: An invalid or illegal selector was specified\n".to_string(),
        })?;
        let dom = self.document()?;
        Ok(list.matches_any(&html5dom::elements(&dom)))
    }
}

/// Creates the WebDriver on first use (`webdriver.Chrome(options=self.driver_options)`); it
/// receives the browser arguments.
pub type DriverFactory = Arc<dyn Fn(&[String]) -> PResult<Box<dyn WebDriver>> + Send + Sync>;

/// The default factory: a [`StaticWebDriver`] fetching over `reqwest`.
pub fn static_driver_factory() -> DriverFactory {
    Arc::new(|_args: &[String]| -> PResult<Box<dyn WebDriver>> {
        Ok(Box::new(StaticWebDriver::new(Arc::new(ReqwestFetcher))))
    })
}

/// `JavaScriptRenderer(**config)`: renders pages for JavaScript-dependent content.
pub struct JavaScriptRenderer {
    /// `self.logger` (`semantica.js_renderer`).
    pub logger: Logger,
    /// `self.config`.
    pub config: Options,
    /// `self.use_selenium` (`config.get("use_selenium", False)`).
    pub use_selenium: bool,
    /// `self.driver_options`: the browser arguments (`--headless`, `--no-sandbox`,
    /// `--disable-dev-shm-usage`), set when `use_selenium` is on.
    pub driver_options: Option<Vec<String>>,
    driver: Mutex<Option<Box<dyn WebDriver>>>,
    driver_factory: DriverFactory,
}

impl std::fmt::Debug for JavaScriptRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JavaScriptRenderer")
            .field("config", &self.config)
            .field("use_selenium", &self.use_selenium)
            .field("driver_options", &self.driver_options)
            .field("has_driver", &self.has_driver())
            .finish()
    }
}

impl Default for JavaScriptRenderer {
    fn default() -> Self {
        JavaScriptRenderer::new(Options::new())
    }
}

impl Drop for JavaScriptRenderer {
    /// `__del__`: quit the driver if one was created.
    fn drop(&mut self) {
        if let Ok(mut guard) = self.driver.lock()
            && let Some(driver) = guard.as_mut()
        {
            driver.quit();
        }
    }
}

impl JavaScriptRenderer {
    /// `JavaScriptRenderer(**config)` with the built-in [`StaticWebDriver`].
    pub fn new(config: Options) -> Self {
        Self::with_driver_factory(config, static_driver_factory())
    }

    /// `JavaScriptRenderer(**config)` in an environment without a WebDriver: the original's
    /// `except (ImportError, OSError)` branch when `selenium` cannot be imported (as in the
    /// reference environment) - a warning, and `use_selenium` is turned off.
    pub fn without_webdriver(config: Options) -> Self {
        let logger = get_logger("js_renderer");
        let use_selenium = config.get("use_selenium").map(truthy).unwrap_or(false);
        if use_selenium {
            logger.warning("Selenium not available, JavaScript rendering disabled");
        }
        JavaScriptRenderer {
            logger,
            config,
            use_selenium: false,
            driver_options: None,
            driver: Mutex::new(None),
            driver_factory: static_driver_factory(),
        }
    }

    /// `JavaScriptRenderer(**config)` creating its WebDriver with `factory`.
    pub fn with_driver_factory(config: Options, factory: DriverFactory) -> Self {
        let logger = get_logger("js_renderer");
        let use_selenium = config.get("use_selenium").map(truthy).unwrap_or(false);
        let mut driver_options = None;
        if use_selenium {
            driver_options = Some(vec![
                "--headless".to_string(),
                "--no-sandbox".to_string(),
                "--disable-dev-shm-usage".to_string(),
            ]);
            logger.info("Selenium WebDriver configured");
        }
        JavaScriptRenderer {
            logger,
            config,
            use_selenium,
            driver_options,
            driver: Mutex::new(None),
            driver_factory: factory,
        }
    }

    /// Whether a WebDriver has been created (`self.driver is not None`).
    pub fn has_driver(&self) -> bool {
        self.driver.lock().map(|d| d.is_some()).unwrap_or(false)
    }

    fn driver_guard(&self) -> std::sync::MutexGuard<'_, Option<Box<dyn WebDriver>>> {
        self.driver.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// `render_page(html_content, **options)`: the rendered page source.
    ///
    /// Without `use_selenium` this logs a warning and returns the content unchanged (a file's
    /// text for file arguments). Otherwise the page is loaded into the WebDriver (a file
    /// argument through its `file://` URL, content through a temporary `.html` file that is
    /// kept, as in the original), the renderer waits for the document (`wait_time`, default
    /// 5 seconds) and for `wait_for_selector`, and returns `page_source`; failures become
    /// `ProcessingError("Failed to render JavaScript: ...")`.
    pub fn render_page(&self, html_content: impl Into<Source>, options: &Options) -> PResult<String> {
        let source: Source = html_content.into();
        if !self.use_selenium {
            self.logger
                .warning("JavaScript rendering not available (Selenium not configured)");
            return match source.file_path()? {
                Some(path) => py_read_text(&path, "utf-8", DecodeErrors::Ignore),
                None => Ok(source.text().unwrap_or_default().to_string()),
            };
        }
        let result = (|| -> PResult<String> {
            {
                let mut guard = self.driver_guard();
                if guard.is_none() {
                    let args = self.driver_options.clone().unwrap_or_default();
                    *guard = Some((self.driver_factory)(&args)?);
                }
            }
            let url = match source.file_path()? {
                Some(path) => format!("file://{}", absolute_path_str(&path)),
                None => {
                    let mut file = tempfile::Builder::new()
                        .prefix("tmp")
                        .suffix(".html")
                        .tempfile()
                        .map_err(SemanticaError::from)?;
                    file.write_all(source.text().unwrap_or_default().as_bytes())
                        .map_err(SemanticaError::from)?;
                    let (_, path) = file.keep().map_err(|e| SemanticaError::from(e.error))?;
                    format!("file://{}", py_path_str(&path))
                }
            };
            self.driver_guard().as_mut().expect("driver created").get(&url)?;
            let wait_time = options.get("wait_time").and_then(Value::as_f64).unwrap_or(5.0);
            self.wait_for_content_secs(None, wait_time);
            if let Some(selector) = options.get("wait_for_selector").filter(|v| truthy(v)) {
                let selector = match selector {
                    Value::String(s) => s.clone(),
                    other => sem_utils::pycompat::fmt::py_str(other),
                };
                self.wait_for_selector(&selector, wait_time)?;
            }
            self.driver_guard().as_mut().expect("driver created").page_source()
        })();
        result.map_err(|e| {
            self.logger.error(format!("Failed to render JavaScript: {e}"));
            ProcessingError::new(format!("Failed to render JavaScript: {e}")).into()
        })
    }

    /// `WebDriverWait(driver, wait_time).until(presence_of_element_located(...))`: polls every
    /// 0.5 s and raises `TimeoutException` (`"Message: \n"`) when the time runs out.
    fn wait_for_selector(&self, selector: &str, timeout: f64) -> PResult<()> {
        let end = Instant::now() + Duration::from_secs_f64(timeout.max(0.0));
        loop {
            if self.driver_guard().as_mut().expect("driver created").has_css_element(selector)? {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(500).min(end.saturating_duration_since(Instant::now())));
            if Instant::now() >= end {
                return Err(SemanticaError::Other {
                    module: "selenium.common.exceptions".to_string(),
                    type_name: "TimeoutException".to_string(),
                    message: "Message: \n".to_string(),
                });
            }
        }
    }

    /// `execute_script(script, **context)`: run JavaScript in the loaded page. Requires a
    /// WebDriver that has rendered a page (`ProcessingError("JavaScript execution requires
    /// Selenium WebDriver")` otherwise); the static driver only answers
    /// `return document.readyState`.
    pub fn execute_script(&self, script: &str, _context: &Options) -> PResult<Value> {
        let mut guard = self.driver_guard();
        let Some(driver) = guard.as_mut().filter(|_| self.use_selenium) else {
            return Err(ProcessingError::new("JavaScript execution requires Selenium WebDriver").into());
        };
        driver.execute_script(script).map_err(|e| {
            self.logger.error(format!("Failed to execute script: {e}"));
            ProcessingError::new(format!("Failed to execute script: {e}")).into()
        })
    }

    /// `wait_for_content(condition=None, timeout=30)`: wait until `document.readyState` is
    /// `"complete"`; `false` without a WebDriver or on failure (logged as `Wait timeout`).
    pub fn wait_for_content(&self, condition: Option<&str>, timeout: Option<i64>) -> bool {
        self.wait_for_content_secs(condition, timeout.unwrap_or(30) as f64)
    }

    fn wait_for_content_secs(&self, _condition: Option<&str>, timeout: f64) -> bool {
        if !self.use_selenium || !self.has_driver() {
            return false;
        }
        let end = Instant::now() + Duration::from_secs_f64(timeout.max(0.0));
        loop {
            let state = self
                .driver_guard()
                .as_mut()
                .expect("driver created")
                .execute_script("return document.readyState");
            match state {
                Ok(Value::String(s)) if s == "complete" => return true,
                Ok(_) => {}
                Err(e) => {
                    self.logger.warning(format!("Wait timeout: {e}"));
                    return false;
                }
            }
            std::thread::sleep(Duration::from_millis(500).min(end.saturating_duration_since(Instant::now())));
            if Instant::now() >= end {
                self.logger.warning("Wait timeout: Message: \n");
                return false;
            }
        }
    }
}

/// `str(Path(p).absolute())`.
fn absolute_path_str(path: &std::path::Path) -> String {
    let text = py_path_str(path);
    if text.starts_with('/') {
        return text;
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    if text == "." {
        return py_path_str(&cwd);
    }
    py_path_str(&cwd.join(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_page_source_is_the_html5_serialisation() {
        let dom = parse_html("<!DOCTYPE html><title>T</title><p>one<p>two<table><td>x</table>");
        let html = outer_html(&document_element(&dom).unwrap());
        assert_eq!(
            html,
            "<html><head><title>T</title></head><body><p>one</p><p>two</p><table><tbody><tr><td>x</td></tr></tbody></table></body></html>"
        );
    }

    #[test]
    fn css_subset_matches() {
        let dom = parse_html("<div id=main class='a b'><ul><li data-x='1'>i</li></ul></div><p class=a>");
        let els = html5dom::elements(&dom);
        for (sel, want) in [
            ("#main", true),
            ("div.a.b > ul li", true),
            ("div > li", false),
            ("li[data-x='1']", true),
            ("li[data-x^='2']", false),
            ("p.a, span", true),
            ("span", false),
        ] {
            assert_eq!(css::SelectorList::parse(sel).unwrap().matches_any(&els), want, "{sel}");
        }
        assert!(css::SelectorList::parse("p:hover").is_none());
    }
}
