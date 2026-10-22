//! HTML document parsing (`semantica/parse/html_parser.py`).
//!
//! [`HTMLParser`] parses HTML (a string, or a file path) with the BeautifulSoup
//! `html.parser` port in [`crate::soup`] and extracts metadata (title, description,
//! keywords, author, Open Graph, Twitter card, canonical URL), clean text, links (resolved
//! against a base URL with Python's `urljoin`), images, forms, tables and the heading
//! structure, returning an [`HTMLData`].
//!
//! As in the original, a `str` argument naming an existing file is read as a file (an
//! over-long string makes the `Path(...).exists()` check raise `OSError`), and every step
//! shares one parse tree: removing `script`/`style`/`noscript` for the clean text also
//! removes their links and images from the later extractions.

mod urlparse;

use std::sync::Arc;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use sem_utils::logging::{Logger, get_logger};
use sem_utils::progress_tracker::{ProgressTracker, get_progress_tracker};
use sem_utils::pycompat::fmt::strip as py_strip;
use sem_utils::{ProcessingError, SemanticaError};

use crate::common::{
    DecodeErrors, Options, PResult, Source, opt_truthy, py_name, py_path_str, py_read_text,
};
use crate::soup::{AttrValue, NodeId, Soup};

pub(crate) use urlparse::urljoin;

/// `HTMLMetadata` dataclass.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct HTMLMetadata {
    pub title: Option<String>,
    pub description: Option<String>,
    pub keywords: Option<String>,
    pub author: Option<String>,
    pub og_title: Option<String>,
    pub og_description: Option<String>,
    pub og_image: Option<String>,
    pub og_url: Option<String>,
    pub twitter_card: Option<String>,
    pub canonical_url: Option<String>,
}

impl HTMLMetadata {
    /// `metadata.__dict__`.
    pub fn to_dict(&self) -> Map<String, Value> {
        match serde_json::to_value(self) {
            Ok(Value::Object(m)) => m,
            _ => Map::new(),
        }
    }
}

/// `HTMLElement` dataclass: a tag, its text, attributes and child elements.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct HTMLElement {
    pub tag: String,
    pub text: String,
    #[serde(default)]
    pub attributes: IndexMap<String, String>,
    #[serde(default)]
    pub children: Vec<HTMLElement>,
}

impl HTMLElement {
    /// `HTMLElement(tag, text)` with no attributes or children.
    pub fn new(tag: impl Into<String>, text: impl Into<String>) -> Self {
        HTMLElement {
            tag: tag.into(),
            text: text.into(),
            attributes: IndexMap::new(),
            children: Vec::new(),
        }
    }
}

/// `HTMLData` dataclass: the parsed document. `structure` is declared as a list (and
/// defaults to `[]`) but `HTMLParser.parse` stores the `{"headings": ..., "sections": ...}`
/// dictionary there, so it is a JSON value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HTMLData {
    pub metadata: Map<String, Value>,
    pub text: String,
    pub html: String,
    #[serde(default)]
    pub links: Vec<Map<String, Value>>,
    #[serde(default)]
    pub images: Vec<Map<String, Value>>,
    #[serde(default)]
    pub forms: Vec<Map<String, Value>>,
    #[serde(default)]
    pub tables: Vec<Map<String, Value>>,
    #[serde(default = "empty_list")]
    pub structure: Value,
}

fn empty_list() -> Value {
    Value::Array(Vec::new())
}

impl Default for HTMLData {
    fn default() -> Self {
        HTMLData::new(Map::new(), String::new(), String::new())
    }
}

impl HTMLData {
    /// `HTMLData(metadata, text, html)` with the dataclass defaults for the other fields.
    pub fn new(metadata: Map<String, Value>, text: impl Into<String>, html: impl Into<String>) -> Self {
        HTMLData {
            metadata,
            text: text.into(),
            html: html.into(),
            links: Vec::new(),
            images: Vec::new(),
            forms: Vec::new(),
            tables: Vec::new(),
            structure: empty_list(),
        }
    }
}

/// The Python value of an attribute (`tag.get(key)`): a string, the list of a multi-valued
/// attribute, or `None`.
pub(crate) fn attr_json(value: Option<&AttrValue>) -> Value {
    match value {
        None => Value::Null,
        Some(AttrValue::List(items)) => json!(items),
        Some(other) => Value::String(other.as_str().unwrap_or_default().to_string()),
    }
}

/// `tag.get(key, default)` as a string (`str(...)` of a list is never needed by the callers).
fn attr_string(soup: &Soup, id: NodeId, key: &str) -> Option<String> {
    soup.get_str(id, key)
}

/// `str.splitlines()` (Python's line boundaries).
pub(crate) fn py_splitlines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut iter = text.char_indices().peekable();
    while let Some((i, c)) = iter.next() {
        let is_break = matches!(
            c,
            '\n' | '\r' | '\x0b' | '\x0c' | '\x1c' | '\x1d' | '\x1e' | '\u{85}' | '\u{2028}' | '\u{2029}'
        );
        if is_break {
            lines.push(&text[start..i]);
            let mut next = i + c.len_utf8();
            if c == '\r'
                && let Some(&(_, '\n')) = iter.peek()
            {
                iter.next();
                next += 1;
            }
            start = next;
        }
    }
    if start < text.len() {
        lines.push(&text[start..]);
    }
    lines
}

/// `HTMLParser._extract_clean_text`'s whitespace cleaning of `soup.get_text()`.
pub(crate) fn clean_whitespace(text: &str) -> String {
    let mut chunks: Vec<&str> = Vec::new();
    for line in py_splitlines(text) {
        let line = py_strip(line);
        for phrase in line.split("  ") {
            let chunk = py_strip(phrase);
            if !chunk.is_empty() {
                chunks.push(chunk);
            }
        }
    }
    chunks.join(" ")
}

/// `HTMLParser(**config)`: HTML document parser.
#[derive(Debug, Clone)]
pub struct HTMLParser {
    /// `self.logger` (`semantica.html_parser`).
    pub logger: Logger,
    /// `self.config`.
    pub config: Options,
    /// `self.progress_tracker`.
    pub progress_tracker: Arc<ProgressTracker>,
}

impl Default for HTMLParser {
    fn default() -> Self {
        HTMLParser::new(Options::new())
    }
}

impl HTMLParser {
    /// `HTMLParser(**config)`.
    pub fn new(config: Options) -> Self {
        HTMLParser {
            logger: get_logger("html_parser"),
            config,
            progress_tracker: get_progress_tracker(),
        }
    }

    /// `parse(html_content, base_url=None, **options)`: parse HTML content (or an HTML
    /// file) into [`HTMLData`].
    ///
    /// Options: `extract_links` (default true), `extract_images` (true), `extract_forms`
    /// (false), `extract_tables` (true), `clean_text` (true).
    pub fn parse(
        &self,
        html_content: impl Into<Source>,
        base_url: Option<&str>,
        options: &Options,
    ) -> PResult<HTMLData> {
        let source: Source = html_content.into();
        let file_path = source.file_path()?;
        let file_str = file_path.as_deref().map(py_path_str);
        let label = file_path.as_deref().map(py_name).unwrap_or_else(|| "content".to_string());
        let tracking_id = self.progress_tracker.start_tracking(
            file_str.as_deref(),
            Some("parse"),
            Some("HTMLParser"),
            &format!("HTML: {label}"),
            None,
        );
        let outer = (|| -> PResult<HTMLData> {
            let html_string = match &file_path {
                Some(path) => py_read_text(path, "utf-8", DecodeErrors::Ignore)?,
                None => source.text().unwrap_or_default().to_string(),
            };
            match self.parse_html_string(&html_string, base_url, options) {
                Ok(data) => {
                    self.progress_tracker.stop_tracking(
                        &tracking_id,
                        Some("completed"),
                        &format!(
                            "Parsed HTML: {} links, {} images",
                            data.links.len(),
                            data.images.len()
                        ),
                        None,
                    );
                    Ok(data)
                }
                Err(e) => {
                    self.progress_tracker
                        .stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
                    self.logger.error(format!("Failed to parse HTML: {e}"));
                    Err(ProcessingError::new(format!("Failed to parse HTML: {e}")).into())
                }
            }
        })();
        outer.inspect_err(|e| {
            self.progress_tracker
                .stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
        })
    }

    /// The `try` body of `parse` (BeautifulSoup and the extractions).
    fn parse_html_string(&self, html_string: &str, base_url: Option<&str>, options: &Options) -> PResult<HTMLData> {
        let mut soup = Soup::parse(html_string);
        let root = soup.root();
        let metadata = extract_metadata_from(&soup, root);
        let text = if opt_truthy(options, "clean_text", true) {
            extract_clean_text(&mut soup)
        } else {
            soup.get_text(root)
        };
        let links = if opt_truthy(options, "extract_links", true) {
            extract_links_from(&soup, base_url)?
        } else {
            Vec::new()
        };
        let images = if opt_truthy(options, "extract_images", true) {
            extract_images_from(&soup, base_url)?
        } else {
            Vec::new()
        };
        let forms = if opt_truthy(options, "extract_forms", false) {
            extract_forms_from(&soup)
        } else {
            Vec::new()
        };
        let tables = if opt_truthy(options, "extract_tables", true) {
            extract_tables_from(&soup)
        } else {
            Vec::new()
        };
        let structure = extract_structure_from(&soup);
        Ok(HTMLData {
            metadata: metadata.to_dict(),
            text,
            html: html_string.to_string(),
            links,
            images,
            forms,
            tables,
            structure: Value::Object(structure),
        })
    }

    /// `extract_metadata(html_content)`: the metadata dictionary.
    pub fn extract_metadata(&self, html_content: impl Into<Source>) -> PResult<Map<String, Value>> {
        let options = flags(&[
            ("extract_links", false),
            ("extract_images", false),
            ("extract_forms", false),
            ("extract_tables", false),
            ("clean_text", false),
        ]);
        Ok(self.parse(html_content, None, &options)?.metadata)
    }

    /// `extract_text(html_content, clean=True)`: the document text, cleaned by default.
    pub fn extract_text(&self, html_content: impl Into<Source>, clean: Option<bool>) -> PResult<String> {
        let options = flags(&[
            ("extract_links", false),
            ("extract_images", false),
            ("extract_forms", false),
            ("extract_tables", false),
            ("clean_text", clean.unwrap_or(true)),
        ]);
        Ok(self.parse(html_content, None, &options)?.text)
    }

    /// `extract_links(html_content, base_url=None)`: the links, resolved against `base_url`.
    pub fn extract_links(
        &self,
        html_content: impl Into<Source>,
        base_url: Option<&str>,
    ) -> PResult<Vec<Map<String, Value>>> {
        let options = flags(&[
            ("extract_images", false),
            ("extract_forms", false),
            ("extract_tables", false),
        ]);
        Ok(self.parse(html_content, base_url, &options)?.links)
    }
}

fn flags(items: &[(&str, bool)]) -> Options {
    items
        .iter()
        .map(|(k, v)| (k.to_string(), Value::Bool(*v)))
        .collect()
}

/// `_extract_metadata(soup)`.
pub(crate) fn extract_metadata_from(soup: &Soup, root: NodeId) -> HTMLMetadata {
    let mut metadata = HTMLMetadata::default();
    if let Some(title) = soup.find_named(root, "title") {
        metadata.title = Some(py_strip(&soup.get_text(title)).to_string());
    }
    for meta in soup.find_all_named(root, &["meta"]) {
        let name = match attr_string(soup, meta, "name").filter(|n| !n.is_empty()) {
            Some(n) => n,
            None => attr_string(soup, meta, "property").unwrap_or_default(),
        };
        let content = attr_string(soup, meta, "content").unwrap_or_default();
        let lower = name.to_lowercase();
        if lower == "description" {
            metadata.description = Some(content);
        } else if lower == "keywords" {
            metadata.keywords = Some(content);
        } else if lower == "author" {
            metadata.author = Some(content);
        } else if name == "og:title" {
            metadata.og_title = Some(content);
        } else if name == "og:description" {
            metadata.og_description = Some(content);
        } else if name == "og:image" {
            metadata.og_image = Some(content);
        } else if name == "og:url" {
            metadata.og_url = Some(content);
        } else if name == "twitter:card" {
            metadata.twitter_card = Some(content);
        }
    }
    let canonical = soup.find(root, |s, n| {
        s.name(n) == Some("link") && s.is_tag(n) && n != root && s.attribute_matches(n, "rel", "canonical")
    });
    if let Some(link) = canonical {
        metadata.canonical_url = attr_string(soup, link, "href");
    }
    metadata
}

/// `_extract_clean_text(soup)`: remove `script`/`style`/`noscript`, then clean the text.
pub(crate) fn extract_clean_text(soup: &mut Soup) -> String {
    let root = soup.root();
    for element in soup.find_all_named(root, &["script", "style", "noscript"]) {
        soup.decompose(element);
    }
    clean_whitespace(&soup.get_text(root))
}

fn join_url(base_url: Option<&str>, href: &str) -> PResult<String> {
    match base_url.filter(|b| !b.is_empty()) {
        Some(base) => urljoin(base, href).map_err(SemanticaError::Value),
        None => Ok(href.to_string()),
    }
}

/// `_extract_links(soup, base_url)`.
pub(crate) fn extract_links_from(soup: &Soup, base_url: Option<&str>) -> PResult<Vec<Map<String, Value>>> {
    let root = soup.root();
    let mut links = Vec::new();
    for a in soup.find_all(root, |s, n| s.name(n) == Some("a") && n != root && s.has_attr(n, "href")) {
        let href = attr_string(soup, a, "href").unwrap_or_default();
        let url = join_url(base_url, &href)?;
        let mut info = Map::new();
        info.insert("url".to_string(), Value::String(url));
        info.insert("text".to_string(), Value::String(py_strip(&soup.get_text(a)).to_string()));
        info.insert("title".to_string(), attr_json(soup.get(a, "title")));
        info.insert("rel".to_string(), attr_json(soup.get(a, "rel")));
        links.push(info);
    }
    Ok(links)
}

/// `_extract_images(soup, base_url)`.
pub(crate) fn extract_images_from(soup: &Soup, base_url: Option<&str>) -> PResult<Vec<Map<String, Value>>> {
    let root = soup.root();
    let mut images = Vec::new();
    for img in soup.find_all_named(root, &["img"]) {
        let Some(src) = attr_string(soup, img, "src").filter(|s| !s.is_empty()) else {
            continue;
        };
        let url = join_url(base_url, &src)?;
        let mut info = Map::new();
        info.insert("url".to_string(), Value::String(url));
        info.insert(
            "alt".to_string(),
            soup.get(img, "alt").map(|v| attr_json(Some(v))).unwrap_or_else(|| json!("")),
        );
        info.insert("title".to_string(), attr_json(soup.get(img, "title")));
        info.insert("width".to_string(), attr_json(soup.get(img, "width")));
        info.insert("height".to_string(), attr_json(soup.get(img, "height")));
        images.push(info);
    }
    Ok(images)
}

/// `_extract_forms(soup)`.
pub(crate) fn extract_forms_from(soup: &Soup) -> Vec<Map<String, Value>> {
    let root = soup.root();
    let mut forms = Vec::new();
    for form in soup.find_all_named(root, &["form"]) {
        let action = soup.get(form, "action").map(|v| attr_json(Some(v))).unwrap_or_else(|| json!(""));
        let method = attr_string(soup, form, "method").unwrap_or_else(|| "get".to_string()).to_lowercase();
        let mut fields = Vec::new();
        for input in soup.find_all_named(form, &["input", "textarea", "select"]) {
            let type_value = match soup.get(input, "type") {
                Some(v) if !matches!(v, AttrValue::Str(s) if s.is_empty()) => attr_json(Some(v)),
                _ => Value::String(soup.name(input).unwrap_or_default().to_string()),
            };
            let mut field = Map::new();
            field.insert("name".to_string(), attr_json(soup.get(input, "name")));
            field.insert("type".to_string(), type_value);
            field.insert(
                "value".to_string(),
                soup.get(input, "value").map(|v| attr_json(Some(v))).unwrap_or_else(|| json!("")),
            );
            field.insert("required".to_string(), Value::Bool(soup.has_attr(input, "required")));
            fields.push(Value::Object(field));
        }
        let mut info = Map::new();
        info.insert("action".to_string(), action);
        info.insert("method".to_string(), Value::String(method));
        info.insert("fields".to_string(), Value::Array(fields));
        forms.push(info);
    }
    forms
}

/// `_extract_tables(soup)`.
pub(crate) fn extract_tables_from(soup: &Soup) -> Vec<Map<String, Value>> {
    let root = soup.root();
    let mut tables = Vec::new();
    for table in soup.find_all_named(root, &["table"]) {
        let mut headers = Vec::new();
        if let Some(thead) = soup.find_named(table, "thead") {
            for th in soup.find_all_named(thead, &["th", "td"]) {
                headers.push(Value::String(py_strip(&soup.get_text(th)).to_string()));
            }
        }
        let body = soup.find_named(table, "tbody").unwrap_or(table);
        let mut rows = Vec::new();
        for row in soup.find_all_named(body, &["tr"]) {
            let cells: Vec<Value> = soup
                .find_all_named(row, &["td", "th"])
                .into_iter()
                .map(|c| Value::String(py_strip(&soup.get_text(c)).to_string()))
                .collect();
            if !cells.is_empty() {
                rows.push(Value::Array(cells));
            }
        }
        let mut data = Map::new();
        data.insert("headers".to_string(), Value::Array(headers));
        data.insert("rows".to_string(), Value::Array(rows));
        tables.push(data);
    }
    tables
}

/// `_extract_structure(soup)`: headings by level, then document order.
pub(crate) fn extract_structure_from(soup: &Soup) -> Map<String, Value> {
    let root = soup.root();
    let mut headings = Vec::new();
    for level in 1..=6 {
        let name = format!("h{level}");
        for heading in soup.find_all_named(root, &[name.as_str()]) {
            headings.push(json!({
                "level": level,
                "text": py_strip(&soup.get_text(heading)),
                "id": attr_json(soup.get(heading, "id")),
            }));
        }
    }
    let mut structure = Map::new();
    structure.insert("headings".to_string(), Value::Array(headings));
    structure.insert("sections".to_string(), Value::Array(Vec::new()));
    structure
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitlines_and_clean_whitespace() {
        assert_eq!(py_splitlines("a\r\nb\rc\x0bd\n"), vec!["a", "b", "c", "d"]);
        assert_eq!(py_splitlines(""), Vec::<&str>::new());
        // Expected value from CPython 3.11 (the original's clean-text expression).
        assert_eq!(clean_whitespace("  Hello   World \n\n  x\u{a0}  "), "Hello World x");
    }
}
