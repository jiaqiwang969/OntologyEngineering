//! Port of the parts of CPython 3.11's `urllib.parse` the ingest module relies on
//! (`urlsplit`, `urlparse`, `urljoin`, `urlunsplit`, `urlunparse`, `parse_qs(l)`, `quote`,
//! `unquote`, `urlencode`). The WHATWG parser of the `url` crate normalises URLs (lowercases
//! hosts, adds trailing slashes, ...), which the original never does, so the Python algorithm
//! is reproduced here line by line.

use indexmap::IndexMap;
use sem_utils::SemanticaError;
use unicode_normalization::UnicodeNormalization;

const USES_RELATIVE: &[&str] = &[
    "", "ftp", "http", "gopher", "nntp", "imap", "wais", "file", "https", "shttp", "mms",
    "prospero", "rtsp", "rtsps", "rtspu", "sftp", "svn", "svn+ssh", "ws", "wss",
];

const USES_NETLOC: &[&str] = &[
    "", "ftp", "http", "gopher", "nntp", "telnet", "imap", "wais", "file", "mms", "https",
    "shttp", "snews", "prospero", "rtsp", "rtsps", "rtspu", "rsync", "svn", "svn+ssh", "sftp",
    "nfs", "git", "git+ssh", "ws", "wss",
];

const USES_PARAMS: &[&str] = &[
    "", "ftp", "hdl", "prospero", "http", "imap", "https", "shttp", "rtsp", "rtsps", "rtspu",
    "sip", "sips", "mms", "sftp", "tel",
];

const SCHEME_CHARS: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789+-.";

fn value_error(message: impl Into<String>) -> SemanticaError {
    SemanticaError::Value(message.into())
}

/// `urllib.parse.SplitResult`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SplitResult {
    /// `scheme`.
    pub scheme: String,
    /// `netloc`.
    pub netloc: String,
    /// `path`.
    pub path: String,
    /// `query`.
    pub query: String,
    /// `fragment`.
    pub fragment: String,
}

/// `urllib.parse.ParseResult`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParseResult {
    /// `scheme`.
    pub scheme: String,
    /// `netloc`.
    pub netloc: String,
    /// `path`.
    pub path: String,
    /// `params`.
    pub params: String,
    /// `query`.
    pub query: String,
    /// `fragment`.
    pub fragment: String,
}

fn rpartition<'a>(s: &'a str, sep: &str) -> (&'a str, bool, &'a str) {
    match s.rfind(sep) {
        Some(i) => (&s[..i], true, &s[i + sep.len()..]),
        None => ("", false, s),
    }
}

fn partition<'a>(s: &'a str, sep: &str) -> (&'a str, bool, &'a str) {
    match s.find(sep) {
        Some(i) => (&s[..i], true, &s[i + sep.len()..]),
        None => (s, false, ""),
    }
}

/// Netloc accessors shared by both result types (`_NetlocResultMixinStr`).
fn netloc_userinfo(netloc: &str) -> (Option<String>, Option<String>) {
    let (userinfo, have_info, _) = rpartition(netloc, "@");
    if have_info {
        let (username, have_password, password) = partition(userinfo, ":");
        (
            Some(username.to_string()),
            if have_password {
                Some(password.to_string())
            } else {
                None
            },
        )
    } else {
        (None, None)
    }
}

fn netloc_hostinfo(netloc: &str) -> (String, Option<String>) {
    let (_, _, hostinfo) = rpartition(netloc, "@");
    let (_, have_open_br, bracketed) = partition(hostinfo, "[");
    let (hostname, port) = if have_open_br {
        let (hostname, _, port) = partition(bracketed, "]");
        let (_, _, port) = partition(port, ":");
        (hostname, port)
    } else {
        let (hostname, _, port) = partition(hostinfo, ":");
        (hostname, port)
    };
    (
        hostname.to_string(),
        if port.is_empty() {
            None
        } else {
            Some(port.to_string())
        },
    )
}

fn netloc_hostname(netloc: &str) -> Option<String> {
    let (hostname, _) = netloc_hostinfo(netloc);
    if hostname.is_empty() {
        return None;
    }
    let (host, percent, zone) = partition(&hostname, "%");
    let mut out = host.to_lowercase();
    if percent {
        out.push('%');
        out.push_str(zone);
    }
    Some(out)
}

fn netloc_port(netloc: &str) -> Result<Option<u16>, SemanticaError> {
    let (_, port) = netloc_hostinfo(netloc);
    match port {
        None => Ok(None),
        Some(port) => {
            if !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) {
                match port.parse::<u64>() {
                    Ok(p) if p <= 65535 => Ok(Some(p as u16)),
                    _ => Err(value_error("Port out of range 0-65535")),
                }
            } else {
                Err(value_error(format!(
                    "Port could not be cast to integer value as {}",
                    sem_utils::pycompat::fmt::str_repr(&port)
                )))
            }
        }
    }
}

macro_rules! netloc_accessors {
    ($t:ty) => {
        impl $t {
            /// `.username`.
            pub fn username(&self) -> Option<String> {
                netloc_userinfo(&self.netloc).0
            }

            /// `.password`.
            pub fn password(&self) -> Option<String> {
                netloc_userinfo(&self.netloc).1
            }

            /// `.hostname` (lowercased, IPv6 brackets removed; `None` when empty).
            pub fn hostname(&self) -> Option<String> {
                netloc_hostname(&self.netloc)
            }

            /// `.port` (raises `ValueError` for non-numeric or out-of-range ports).
            pub fn port(&self) -> Result<Option<u16>, SemanticaError> {
                netloc_port(&self.netloc)
            }
        }
    };
}

netloc_accessors!(SplitResult);
netloc_accessors!(ParseResult);

impl SplitResult {
    /// `.geturl()`.
    pub fn geturl(&self) -> String {
        urlunsplit(
            &self.scheme,
            &self.netloc,
            &self.path,
            &self.query,
            &self.fragment,
        )
    }
}

impl ParseResult {
    /// `.geturl()`.
    pub fn geturl(&self) -> String {
        urlunparse(self)
    }
}

const WHATWG_C0_CONTROL_OR_SPACE: &[char] = &[
    '\x00', '\x01', '\x02', '\x03', '\x04', '\x05', '\x06', '\x07', '\x08', '\t', '\n', '\x0b',
    '\x0c', '\r', '\x0e', '\x0f', '\x10', '\x11', '\x12', '\x13', '\x14', '\x15', '\x16', '\x17',
    '\x18', '\x19', '\x1a', '\x1b', '\x1c', '\x1d', '\x1e', '\x1f', ' ',
];

fn splitnetloc(url: &str, start: usize) -> (String, String) {
    let mut delim = url.len();
    for c in ['/', '?', '#'] {
        if let Some(w) = url[start..].find(c) {
            delim = delim.min(start + w);
        }
    }
    (url[start..delim].to_string(), url[delim..].to_string())
}

fn checknetloc(netloc: &str) -> Result<(), SemanticaError> {
    if netloc.is_empty() || netloc.is_ascii() {
        return Ok(());
    }
    let n: String = netloc
        .chars()
        .filter(|c| !matches!(c, '@' | ':' | '#' | '?'))
        .collect();
    let netloc2: String = n.nfkc().collect();
    if n == netloc2 {
        return Ok(());
    }
    for c in ['/', '?', '#', '@', ':'] {
        if netloc2.contains(c) {
            return Err(value_error(format!(
                "netloc '{netloc}' contains invalid characters under NFKC normalization"
            )));
        }
    }
    Ok(())
}

fn check_bracketed_netloc(netloc: &str) -> Result<(), SemanticaError> {
    let (_, _, hostname_and_port) = rpartition(netloc, "@");
    let (before_bracket, have_open_br, bracketed) = partition(hostname_and_port, "[");
    let hostname = if have_open_br {
        if !before_bracket.is_empty() {
            return Err(value_error("Invalid IPv6 URL"));
        }
        let (hostname, _, port) = partition(bracketed, "]");
        if !port.is_empty() && !port.starts_with(':') {
            return Err(value_error("Invalid IPv6 URL"));
        }
        hostname
    } else {
        partition(hostname_and_port, ":").0
    };
    check_bracketed_host(hostname)
}

fn check_bracketed_host(hostname: &str) -> Result<(), SemanticaError> {
    if hostname.starts_with('v') {
        static IPVFUTURE: once_cell::sync::Lazy<regex::Regex> =
            once_cell::sync::Lazy::new(|| regex::Regex::new(r"\Av[a-fA-F0-9]+\..+\z").unwrap());
        if !IPVFUTURE.is_match(hostname) {
            return Err(value_error("IPvFuture address is invalid"));
        }
        Ok(())
    } else {
        let ip = super::ipaddr::ip_address(hostname)?;
        if ip.is_ipv4() {
            return Err(value_error("An IPv4 address cannot be in brackets"));
        }
        Ok(())
    }
}

/// `urllib.parse.urlsplit(url, scheme='', allow_fragments=True)`.
pub fn urlsplit_with(
    url: &str,
    scheme: &str,
    allow_fragments: bool,
) -> Result<SplitResult, SemanticaError> {
    let url = url.trim_start_matches(WHATWG_C0_CONTROL_OR_SPACE);
    let mut scheme = scheme.trim_matches(WHATWG_C0_CONTROL_OR_SPACE).to_string();
    let mut url: String = url
        .chars()
        .filter(|c| !matches!(c, '\t' | '\r' | '\n'))
        .collect();
    scheme.retain(|c| !matches!(c, '\t' | '\r' | '\n'));

    let mut netloc = String::new();
    let mut query = String::new();
    let mut fragment = String::new();
    if let Some(i) = url.find(':') {
        let first = url.chars().next();
        if i > 0
            && first.is_some_and(|c| c.is_ascii() && c.is_alphabetic())
            && url[..i].chars().all(|c| SCHEME_CHARS.contains(c))
        {
            scheme = url[..i].to_lowercase();
            url = url[i + 1..].to_string();
        }
    }
    if url.starts_with("//") {
        let (n, rest) = splitnetloc(&url, 2);
        netloc = n;
        url = rest;
        if (netloc.contains('[') && !netloc.contains(']'))
            || (netloc.contains(']') && !netloc.contains('['))
        {
            return Err(value_error("Invalid IPv6 URL"));
        }
        if netloc.contains('[') && netloc.contains(']') {
            check_bracketed_netloc(&netloc)?;
        }
    }
    if allow_fragments && let Some(i) = url.find('#') {
        fragment = url[i + 1..].to_string();
        url.truncate(i);
    }
    if let Some(i) = url.find('?') {
        query = url[i + 1..].to_string();
        url.truncate(i);
    }
    checknetloc(&netloc)?;
    Ok(SplitResult {
        scheme,
        netloc,
        path: url,
        query,
        fragment,
    })
}

/// `urllib.parse.urlsplit(url)`.
pub fn urlsplit(url: &str) -> Result<SplitResult, SemanticaError> {
    urlsplit_with(url, "", true)
}

fn splitparams(url: &str) -> (String, String) {
    let i = if url.contains('/') {
        let start = url.rfind('/').unwrap_or(0);
        match url[start..].find(';') {
            Some(i) => start + i,
            None => return (url.to_string(), String::new()),
        }
    } else {
        match url.find(';') {
            Some(i) => i,
            None => return (url.to_string(), String::new()),
        }
    };
    (url[..i].to_string(), url[i + 1..].to_string())
}

/// `urllib.parse.urlparse(url, scheme='', allow_fragments=True)`.
pub fn urlparse_with(
    url: &str,
    scheme: &str,
    allow_fragments: bool,
) -> Result<ParseResult, SemanticaError> {
    let split = urlsplit_with(url, scheme, allow_fragments)?;
    let (path, params) = if USES_PARAMS.contains(&split.scheme.as_str()) && split.path.contains(';')
    {
        splitparams(&split.path)
    } else {
        (split.path.clone(), String::new())
    };
    Ok(ParseResult {
        scheme: split.scheme,
        netloc: split.netloc,
        path,
        params,
        query: split.query,
        fragment: split.fragment,
    })
}

/// `urllib.parse.urlparse(url)`.
pub fn urlparse(url: &str) -> Result<ParseResult, SemanticaError> {
    urlparse_with(url, "", true)
}

/// `urllib.parse.urlunsplit((scheme, netloc, url, query, fragment))`.
pub fn urlunsplit(scheme: &str, netloc: &str, url: &str, query: &str, fragment: &str) -> String {
    let mut url = url.to_string();
    if !netloc.is_empty()
        || (!scheme.is_empty() && USES_NETLOC.contains(&scheme))
        || url.starts_with("//")
    {
        if !url.is_empty() && !url.starts_with('/') {
            url = format!("/{url}");
        }
        url = format!("//{netloc}{url}");
    }
    if !scheme.is_empty() {
        url = format!("{scheme}:{url}");
    }
    if !query.is_empty() {
        url = format!("{url}?{query}");
    }
    if !fragment.is_empty() {
        url = format!("{url}#{fragment}");
    }
    url
}

/// `urllib.parse.urlunparse(parts)`.
pub fn urlunparse(parts: &ParseResult) -> String {
    let url = if parts.params.is_empty() {
        parts.path.clone()
    } else {
        format!("{};{}", parts.path, parts.params)
    };
    urlunsplit(
        &parts.scheme,
        &parts.netloc,
        &url,
        &parts.query,
        &parts.fragment,
    )
}

/// `urllib.parse.urljoin(base, url)`.
pub fn urljoin(base: &str, url: &str) -> Result<String, SemanticaError> {
    if base.is_empty() {
        return Ok(url.to_string());
    }
    if url.is_empty() {
        return Ok(base.to_string());
    }
    let b = urlparse_with(base, "", true)?;
    let u = urlparse_with(url, &b.scheme, true)?;
    let ParseResult {
        scheme,
        mut netloc,
        mut path,
        mut params,
        mut query,
        fragment,
    } = u;
    if scheme != b.scheme || !USES_RELATIVE.contains(&scheme.as_str()) {
        return Ok(url.to_string());
    }
    if USES_NETLOC.contains(&scheme.as_str()) {
        if !netloc.is_empty() {
            return Ok(urlunparse(&ParseResult {
                scheme,
                netloc,
                path,
                params,
                query,
                fragment,
            }));
        }
        netloc = b.netloc.clone();
    }
    if path.is_empty() && params.is_empty() {
        path = b.path.clone();
        params = b.params.clone();
        if query.is_empty() {
            query = b.query.clone();
        }
        return Ok(urlunparse(&ParseResult {
            scheme,
            netloc,
            path,
            params,
            query,
            fragment,
        }));
    }

    let mut base_parts: Vec<&str> = b.path.split('/').collect();
    if base_parts.last().is_some_and(|p| !p.is_empty()) {
        base_parts.pop();
    }
    let segments: Vec<String> = if path.starts_with('/') {
        path.split('/').map(str::to_string).collect()
    } else {
        let mut segs: Vec<String> = base_parts
            .iter()
            .map(|s| s.to_string())
            .chain(path.split('/').map(str::to_string))
            .collect();
        if segs.len() > 2 {
            let last = segs.pop().unwrap();
            let first = segs.remove(0);
            let mut middle: Vec<String> = segs.into_iter().filter(|s| !s.is_empty()).collect();
            middle.insert(0, first);
            middle.push(last);
            segs = middle;
        }
        segs
    };
    let mut resolved: Vec<String> = Vec::new();
    for seg in &segments {
        if seg == ".." {
            resolved.pop();
        } else if seg == "." {
            continue;
        } else {
            resolved.push(seg.clone());
        }
    }
    if segments
        .last()
        .is_some_and(|s| s == "." || s == "..")
    {
        resolved.push(String::new());
    }
    let joined = resolved.join("/");
    Ok(urlunparse(&ParseResult {
        scheme,
        netloc,
        path: if joined.is_empty() {
            "/".to_string()
        } else {
            joined
        },
        params,
        query,
        fragment,
    }))
}

fn is_hex(b: u8) -> bool {
    b.is_ascii_hexdigit()
}

/// `urllib.parse.unquote_to_bytes(string)`.
pub fn unquote_to_bytes(s: &str) -> Vec<u8> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && is_hex(bytes[i + 1])
            && is_hex(bytes[i + 2])
        {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("00");
            out.push(u8::from_str_radix(hex, 16).unwrap_or(0));
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

/// `urllib.parse.unquote(string)` (UTF-8, errors="replace").
pub fn unquote(s: &str) -> String {
    if !s.contains('%') {
        return s.to_string();
    }
    String::from_utf8_lossy(&unquote_to_bytes(s)).into_owned()
}

/// `urllib.parse.unquote_plus(string)`.
pub fn unquote_plus(s: &str) -> String {
    unquote(&s.replace('+', " "))
}

const ALWAYS_SAFE: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_.-~";

/// `urllib.parse.quote(string, safe)` (UTF-8).
pub fn quote(s: &str, safe: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        let c = *b as char;
        if b.is_ascii() && (ALWAYS_SAFE.contains(c) || safe.contains(c)) {
            out.push(c);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `urllib.parse.quote_plus(string, safe='')`.
pub fn quote_plus(s: &str, safe: &str) -> String {
    if !s.contains(' ') {
        return quote(s, safe);
    }
    let safe = format!("{safe} ");
    quote(s, &safe).replace(' ', "+")
}

/// `urllib.parse.urlencode(pairs)` with `quote_plus` (the default `quote_via`).
pub fn urlencode<K: AsRef<str>, V: AsRef<str>>(pairs: &[(K, V)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", quote_plus(k.as_ref(), ""), quote_plus(v.as_ref(), "")))
        .collect::<Vec<_>>()
        .join("&")
}

/// `urllib.parse.parse_qsl(qs, keep_blank_values)` (separator `&`).
pub fn parse_qsl(qs: &str, keep_blank_values: bool) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if qs.is_empty() {
        return out;
    }
    for name_value in qs.split('&') {
        if name_value.is_empty() {
            continue;
        }
        let (name, has_eq, value) = partition(name_value, "=");
        if !has_eq && !keep_blank_values {
            // `nv = name_value.split('=', 1)`; with strict_parsing off a missing '=' keeps
            // the pair only when blank values are kept.
            continue;
        }
        if !value.is_empty() || keep_blank_values {
            out.push((unquote_plus(name), unquote_plus(value)));
        }
    }
    out
}

/// `urllib.parse.parse_qs(qs, keep_blank_values)`.
pub fn parse_qs(qs: &str, keep_blank_values: bool) -> IndexMap<String, Vec<String>> {
    let mut out: IndexMap<String, Vec<String>> = IndexMap::new();
    for (k, v) in parse_qsl(qs, keep_blank_values) {
        out.entry(k).or_default().push(v);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_cases() {
        assert_eq!(urljoin("http://site.com", "/rss.xml").unwrap(), "http://site.com/rss.xml");
        assert_eq!(urljoin("https://example.com", "/link").unwrap(), "https://example.com/link");
        assert_eq!(urljoin("http://a/b/c/d;p?q", "../g").unwrap(), "http://a/b/g");
        assert_eq!(urljoin("http://a/b/c/d;p?q", "g?y").unwrap(), "http://a/b/c/g?y");
        assert_eq!(urljoin("http://a/b/c/d;p?q", "").unwrap(), "http://a/b/c/d;p?q");
        assert_eq!(urljoin("http://a/b/c/d;p?q", "..").unwrap(), "http://a/b/");
        assert_eq!(urljoin("http://example.com", "/robots.txt").unwrap(), "http://example.com/robots.txt");
    }

    #[test]
    fn split_cases() {
        let r = urlparse("http://[::1").unwrap_err();
        assert_eq!(r.to_string(), "Invalid IPv6 URL");
        let r = urlparse("https://user@[::1/repo.git").unwrap_err();
        assert_eq!(r.to_string(), "Invalid IPv6 URL");
        let p = urlparse("https://Example.COM:8443/a;b?c#d").unwrap();
        assert_eq!(p.hostname().as_deref(), Some("example.com"));
        assert_eq!(p.port().unwrap(), Some(8443));
        assert_eq!(p.params, "b");
        let p = urlparse("http://s.xml").unwrap();
        assert_eq!(p.netloc, "s.xml");
        assert_eq!(p.path, "");
    }
}
