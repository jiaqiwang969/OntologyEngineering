//! A port of Python 3.11's `urllib.robotparser.RobotFileParser`. `read()` fetches the file
//! with a plain GET that follows redirects (like `urllib.request.urlopen`, without the SSRF
//! guard, as in the original); everything else is pure.

use std::time::{SystemTime, UNIX_EPOCH};

use sem_utils::SemanticaError;

use super::pyurl;

/// `RequestRate(requests, seconds)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestRate {
    /// `requests`.
    pub requests: u64,
    /// `seconds`.
    pub seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuleLine {
    path: String,
    allowance: bool,
}

impl RuleLine {
    fn new(path: &str, allowance: bool) -> RuleLine {
        let allowance = if path.is_empty() && !allowance {
            true
        } else {
            allowance
        };
        let path = match pyurl::urlparse(path) {
            Ok(p) => pyurl::urlunparse(&p),
            Err(_) => path.to_string(),
        };
        RuleLine {
            path: pyurl::quote(&path, "/"),
            allowance,
        }
    }

    fn applies_to(&self, filename: &str) -> bool {
        self.path == "*" || filename.starts_with(&self.path)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Entry {
    useragents: Vec<String>,
    rulelines: Vec<RuleLine>,
    delay: Option<u64>,
    req_rate: Option<RequestRate>,
}

impl Entry {
    fn applies_to(&self, useragent: &str) -> bool {
        let useragent = useragent.split('/').next().unwrap_or("").to_lowercase();
        for agent in &self.useragents {
            if agent == "*" {
                return true;
            }
            if useragent.contains(&agent.to_lowercase()) {
                return true;
            }
        }
        false
    }

    fn allowance(&self, filename: &str) -> bool {
        for line in &self.rulelines {
            if line.applies_to(filename) {
                return line.allowance;
            }
        }
        true
    }
}

/// `RobotFileParser`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RobotFileParser {
    entries: Vec<Entry>,
    sitemaps: Vec<String>,
    default_entry: Option<Entry>,
    /// `disallow_all`.
    pub disallow_all: bool,
    /// `allow_all`.
    pub allow_all: bool,
    /// `url`.
    pub url: String,
    /// `host`.
    pub host: String,
    /// `path`.
    pub path: String,
    last_checked: f64,
}

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

impl RobotFileParser {
    /// `RobotFileParser(url='')`.
    pub fn new(url: &str) -> RobotFileParser {
        let mut parser = RobotFileParser::default();
        parser.set_url(url);
        parser
    }

    /// `mtime()`.
    pub fn mtime(&self) -> f64 {
        self.last_checked
    }

    /// `modified()`.
    pub fn modified(&mut self) {
        self.last_checked = now();
    }

    /// `set_url(url)`.
    pub fn set_url(&mut self, url: &str) {
        self.url = url.to_string();
        if let Ok(p) = pyurl::urlparse(url) {
            self.host = p.netloc;
            self.path = p.path;
        }
    }

    /// `read()`: fetches and parses the robots.txt URL. 401/403 disallow everything, other
    /// 4xx allow everything, network errors propagate.
    pub fn read(&mut self) -> Result<(), SemanticaError> {
        let client = reqwest::blocking::Client::builder()
            .user_agent("Python-urllib/3.11")
            .build()
            .map_err(|e| SemanticaError::Other {
                module: "urllib.error".to_string(),
                type_name: "URLError".to_string(),
                message: format!("<urlopen error {e}>"),
            })?;
        let response = client.get(&self.url).send().map_err(|e| SemanticaError::Other {
            module: "urllib.error".to_string(),
            type_name: "URLError".to_string(),
            message: format!("<urlopen error {e}>"),
        })?;
        let status = response.status().as_u16();
        if status >= 400 {
            if status == 401 || status == 403 {
                self.disallow_all = true;
            } else if status < 500 {
                self.allow_all = true;
            }
            return Ok(());
        }
        let bytes = response.bytes().map_err(|e| SemanticaError::Other {
            module: "urllib.error".to_string(),
            type_name: "URLError".to_string(),
            message: format!("<urlopen error {e}>"),
        })?;
        let text = String::from_utf8(bytes.to_vec()).map_err(|e| SemanticaError::Other {
            module: "builtins".to_string(),
            type_name: "UnicodeDecodeError".to_string(),
            message: e.to_string(),
        })?;
        let lines: Vec<&str> = text.lines().collect();
        self.parse(&lines);
        Ok(())
    }

    fn add_entry(&mut self, entry: Entry) {
        if entry.useragents.iter().any(|a| a == "*") {
            if self.default_entry.is_none() {
                self.default_entry = Some(entry);
            }
        } else {
            self.entries.push(entry);
        }
    }

    /// `parse(lines)`.
    pub fn parse<S: AsRef<str>>(&mut self, lines: &[S]) {
        let mut state = 0;
        let mut entry = Entry::default();
        self.modified();
        for line in lines {
            let mut line = line.as_ref().to_string();
            if line.is_empty() {
                if state == 1 {
                    entry = Entry::default();
                    state = 0;
                } else if state == 2 {
                    self.add_entry(std::mem::take(&mut entry));
                    state = 0;
                }
            }
            if let Some(i) = line.find('#') {
                line.truncate(i);
            }
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let key = key.trim().to_lowercase();
            let value = pyurl::unquote(value.trim());
            match key.as_str() {
                "user-agent" => {
                    if state == 2 {
                        self.add_entry(std::mem::take(&mut entry));
                    }
                    entry.useragents.push(value);
                    state = 1;
                }
                "disallow" => {
                    if state != 0 {
                        entry.rulelines.push(RuleLine::new(&value, false));
                        state = 2;
                    }
                }
                "allow" => {
                    if state != 0 {
                        entry.rulelines.push(RuleLine::new(&value, true));
                        state = 2;
                    }
                }
                "crawl-delay" => {
                    if state != 0 {
                        let v = value.trim();
                        if !v.is_empty() && v.chars().all(|c| c.is_ascii_digit()) {
                            entry.delay = v.parse().ok();
                        }
                        state = 2;
                    }
                }
                "request-rate" => {
                    if state != 0 {
                        let numbers: Vec<&str> = value.split('/').collect();
                        if numbers.len() == 2 {
                            let a = numbers[0].trim();
                            let b = numbers[1].trim();
                            if !a.is_empty()
                                && !b.is_empty()
                                && a.chars().all(|c| c.is_ascii_digit())
                                && b.chars().all(|c| c.is_ascii_digit())
                            {
                                entry.req_rate = Some(RequestRate {
                                    requests: a.parse().unwrap_or(0),
                                    seconds: b.parse().unwrap_or(0),
                                });
                            }
                        }
                        state = 2;
                    }
                }
                "sitemap" => self.sitemaps.push(value),
                _ => {}
            }
        }
        if state == 2 {
            self.add_entry(entry);
        }
    }

    /// `can_fetch(useragent, url)`.
    pub fn can_fetch(&self, useragent: &str, url: &str) -> bool {
        if self.disallow_all {
            return false;
        }
        if self.allow_all {
            return true;
        }
        if self.last_checked == 0.0 {
            return false;
        }
        let path = match pyurl::urlparse(&pyurl::unquote(url)) {
            Ok(p) => pyurl::urlunparse(&pyurl::ParseResult {
                scheme: String::new(),
                netloc: String::new(),
                path: p.path,
                params: p.params,
                query: p.query,
                fragment: p.fragment,
            }),
            Err(_) => String::new(),
        };
        let mut url = pyurl::quote(&path, "/");
        if url.is_empty() {
            url = "/".to_string();
        }
        for entry in &self.entries {
            if entry.applies_to(useragent) {
                return entry.allowance(&url);
            }
        }
        if let Some(default) = &self.default_entry {
            return default.allowance(&url);
        }
        true
    }

    /// `crawl_delay(useragent)`.
    pub fn crawl_delay(&self, useragent: &str) -> Option<u64> {
        if self.mtime() == 0.0 {
            return None;
        }
        for entry in &self.entries {
            if entry.applies_to(useragent) {
                return entry.delay;
            }
        }
        self.default_entry.as_ref().and_then(|e| e.delay)
    }

    /// `site_maps()`.
    pub fn site_maps(&self) -> Option<Vec<String>> {
        if self.sitemaps.is_empty() {
            None
        } else {
            Some(self.sitemaps.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules() {
        let mut p = RobotFileParser::new("http://example.com/robots.txt");
        assert!(!p.can_fetch("SemanticaBot/1.0", "http://example.com/a"));
        p.parse(&["User-agent: *", "Disallow: /private", "", "User-agent: Bad", "Disallow: /"]);
        assert!(p.can_fetch("SemanticaBot/1.0", "http://example.com/a"));
        assert!(!p.can_fetch("SemanticaBot/1.0", "http://example.com/private/x"));
        assert!(!p.can_fetch("BadBot", "http://example.com/a"));
    }
}
