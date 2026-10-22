//! `urllib.parse.urljoin` of CPython 3.11.14 (with the `urlsplit`/`urlparse`/`urlunparse`
//! machinery it uses), as `HTMLParser` calls it to resolve relative links.

/// `uses_relative`.
const USES_RELATIVE: &[&str] = &[
    "", "ftp", "http", "gopher", "nntp", "imap", "wais", "file", "https", "shttp", "mms",
    "prospero", "rtsp", "rtsps", "rtspu", "sftp", "svn", "svn+ssh", "ws", "wss",
];

/// `uses_netloc`.
const USES_NETLOC: &[&str] = &[
    "", "ftp", "http", "gopher", "nntp", "telnet", "imap", "wais", "file", "mms", "https",
    "shttp", "snews", "prospero", "rtsp", "rtsps", "rtspu", "rsync", "svn", "svn+ssh", "sftp",
    "nfs", "git", "git+ssh", "ws", "wss",
];

/// `uses_params`.
const USES_PARAMS: &[&str] = &[
    "", "ftp", "hdl", "prospero", "http", "imap", "https", "shttp", "rtsp", "rtsps", "rtspu",
    "sip", "sips", "mms", "sftp", "tel",
];

/// Characters whose NFKC normalisation contains one of `/?#@:` (Unicode 14, as used by
/// CPython 3.11's `unicodedata`).
const NFKC_DELIMITER_CHARS: &[u32] = &[
    0x2047, 0x2048, 0x2049, 0x2100, 0x2101, 0x2105, 0x2106, 0x2a74, 0xfe13, 0xfe16, 0xfe55,
    0xfe56, 0xfe5f, 0xfe6b, 0xff03, 0xff0f, 0xff1a, 0xff1f, 0xff20,
];

fn is_scheme_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.'
}

/// `urlsplit(url, scheme)`: `(scheme, netloc, path, query, fragment)`, or the `ValueError`
/// message.
pub(crate) fn urlsplit(url: &str, scheme: &str) -> Result<[String; 5], String> {
    let c0 = |c: char| c <= '\x1f' || c == ' ';
    let mut url: String = url.trim_start_matches(c0).to_string();
    let mut scheme: String = scheme.trim_matches(c0).to_string();
    for b in ['\t', '\r', '\n'] {
        url = url.replace(b, "");
        scheme = scheme.replace(b, "");
    }
    let mut netloc = String::new();
    let mut query = String::new();
    let mut fragment = String::new();
    if let Some(i) = url.find(':')
        && i > 0
        && url.as_bytes()[0].is_ascii_alphabetic()
        && url[..i].chars().all(is_scheme_char)
    {
        scheme = url[..i].to_lowercase();
        url = url[i + 1..].to_string();
    }
    if url.starts_with("//") {
        let rest = &url[2..];
        let delim = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        netloc = rest[..delim].to_string();
        url = rest[delim..].to_string();
        let has_open = netloc.contains('[');
        let has_close = netloc.contains(']');
        if has_open != has_close {
            return Err("Invalid IPv6 URL".to_string());
        }
        if has_open && has_close {
            check_bracketed_netloc(&netloc)?;
        }
    }
    if let Some(p) = url.find('#') {
        fragment = url[p + 1..].to_string();
        url.truncate(p);
    }
    if let Some(p) = url.find('?') {
        query = url[p + 1..].to_string();
        url.truncate(p);
    }
    check_netloc(&netloc)?;
    Ok([scheme, netloc, url, query, fragment])
}

fn check_netloc(netloc: &str) -> Result<(), String> {
    if netloc.is_empty() || netloc.is_ascii() {
        return Ok(());
    }
    let n: String = netloc.chars().filter(|c| !matches!(c, '@' | ':' | '#' | '?')).collect();
    if n.chars().any(|c| NFKC_DELIMITER_CHARS.contains(&(c as u32))) {
        return Err(format!(
            "netloc '{netloc}' contains invalid characters under NFKC normalization"
        ));
    }
    Ok(())
}

fn check_bracketed_netloc(netloc: &str) -> Result<(), String> {
    let hostname_and_port = netloc.rsplit_once('@').map(|(_, h)| h).unwrap_or(netloc);
    let (hostname, port) = match hostname_and_port.split_once('[') {
        Some((before, bracketed)) => {
            if !before.is_empty() {
                return Err("Invalid IPv6 URL".to_string());
            }
            let (host, port) = bracketed.split_once(']').unwrap_or((bracketed, ""));
            if !port.is_empty() && !port.starts_with(':') {
                return Err("Invalid IPv6 URL".to_string());
            }
            (host, port)
        }
        None => hostname_and_port.split_once(':').unwrap_or((hostname_and_port, "")),
    };
    let _ = port;
    check_bracketed_host(hostname)
}

fn check_bracketed_host(hostname: &str) -> Result<(), String> {
    if let Some(rest) = hostname.strip_prefix('v') {
        // `\Av[a-fA-F0-9]+\..+\Z`
        let hex = rest.chars().take_while(|c| c.is_ascii_hexdigit()).count();
        let after = &rest[hex..];
        if hex == 0 || !after.starts_with('.') || after.len() < 2 || after[1..].contains('\n') {
            return Err("IPvFuture address is invalid".to_string());
        }
        return Ok(());
    }
    if hostname.parse::<std::net::Ipv4Addr>().is_ok() && is_strict_ipv4(hostname) {
        return Err("An IPv4 address cannot be in brackets".to_string());
    }
    let without_zone = hostname.split_once('%').map(|(a, _)| a).unwrap_or(hostname);
    if without_zone.parse::<std::net::Ipv6Addr>().is_ok() {
        return Ok(());
    }
    Err(format!(
        "{} does not appear to be an IPv4 or IPv6 address",
        sem_utils::pycompat::fmt::str_repr(hostname)
    ))
}

/// `ipaddress.IPv4Address` rejects leading zeros (Rust's parser does too); this keeps the
/// check explicit.
fn is_strict_ipv4(s: &str) -> bool {
    s.split('.')
        .all(|p| !p.is_empty() && (p == "0" || !p.starts_with('0')))
}

/// `urlparse(url, scheme)`: `(scheme, netloc, path, params, query, fragment)`.
pub(crate) fn urlparse(url: &str, scheme: &str) -> Result<[String; 6], String> {
    let [scheme, netloc, mut path, query, fragment] = urlsplit(url, scheme)?;
    let mut params = String::new();
    if USES_PARAMS.contains(&scheme.as_str()) && path.contains(';') {
        let i = match path.rfind('/') {
            Some(slash) => match path[slash..].find(';') {
                Some(p) => Some(slash + p),
                None => None,
            },
            None => path.find(';'),
        };
        if let Some(i) = i {
            params = path[i + 1..].to_string();
            path.truncate(i);
        }
    }
    Ok([scheme, netloc, path, params, query, fragment])
}

/// `urlunsplit((scheme, netloc, url, query, fragment))`.
pub(crate) fn urlunsplit(scheme: &str, netloc: &str, url: &str, query: &str, fragment: &str) -> String {
    let mut url = url.to_string();
    if !netloc.is_empty() || (!scheme.is_empty() && USES_NETLOC.contains(&scheme)) || url.starts_with("//") {
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

/// `urlunparse((scheme, netloc, url, params, query, fragment))`.
pub(crate) fn urlunparse(parts: [&str; 6]) -> String {
    let [scheme, netloc, url, params, query, fragment] = parts;
    let url = if params.is_empty() {
        url.to_string()
    } else {
        format!("{url};{params}")
    };
    urlunsplit(scheme, netloc, &url, query, fragment)
}

/// `urljoin(base, url)`; `Err` carries the `ValueError` message.
pub(crate) fn urljoin(base: &str, url: &str) -> Result<String, String> {
    if base.is_empty() {
        return Ok(url.to_string());
    }
    if url.is_empty() {
        return Ok(base.to_string());
    }
    let [bscheme, bnetloc, bpath, bparams, bquery, _bfragment] = urlparse(base, "")?;
    let [scheme, netloc, path, params, query, fragment] = urlparse(url, &bscheme)?;
    if scheme != bscheme || !USES_RELATIVE.contains(&scheme.as_str()) {
        return Ok(url.to_string());
    }
    let mut netloc = netloc;
    if USES_NETLOC.contains(&scheme.as_str()) {
        if !netloc.is_empty() {
            return Ok(urlunparse([&scheme, &netloc, &path, &params, &query, &fragment]));
        }
        netloc = bnetloc;
    }
    if path.is_empty() && params.is_empty() {
        let query = if query.is_empty() { bquery } else { query };
        return Ok(urlunparse([&scheme, &netloc, &bpath, &bparams, &query, &fragment]));
    }
    let mut base_parts: Vec<&str> = bpath.split('/').collect();
    if base_parts.last() != Some(&"") {
        base_parts.pop();
    }
    let segments: Vec<&str> = if path.starts_with('/') {
        path.split('/').collect()
    } else {
        let mut segs: Vec<&str> = base_parts;
        segs.extend(path.split('/'));
        if segs.len() > 2 {
            let last = segs.len() - 1;
            let middle: Vec<&str> = segs[1..last].iter().copied().filter(|s| !s.is_empty()).collect();
            let mut rebuilt = vec![segs[0]];
            rebuilt.extend(middle);
            rebuilt.push(segs[last]);
            segs = rebuilt;
        }
        segs
    };
    let mut resolved: Vec<&str> = Vec::new();
    for seg in &segments {
        match *seg {
            ".." => {
                resolved.pop();
            }
            "." => {}
            other => resolved.push(other),
        }
    }
    if matches!(segments.last(), Some(&".") | Some(&"..")) {
        resolved.push("");
    }
    let joined = resolved.join("/");
    let joined = if joined.is_empty() { "/".to_string() } else { joined };
    Ok(urlunparse([&scheme, &netloc, &joined, &params, &query, &fragment]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3986_examples() {
        let base = "http://a/b/c/d;p?q";
        let cases = [
            ("g:h", "g:h"),
            ("g", "http://a/b/c/g"),
            ("./g", "http://a/b/c/g"),
            ("g/", "http://a/b/c/g/"),
            ("/g", "http://a/g"),
            ("//g", "http://g"),
            ("?y", "http://a/b/c/d;p?y"),
            ("g?y", "http://a/b/c/g?y"),
            ("#s", "http://a/b/c/d;p?q#s"),
            (";x", "http://a/b/c/;x"),
            ("", "http://a/b/c/d;p?q"),
            (".", "http://a/b/c/"),
            ("..", "http://a/b/"),
            ("../..", "http://a/"),
            ("../../../g", "http://a/g"),
        ];
        for (rel, want) in cases {
            assert_eq!(urljoin(base, rel).unwrap(), want, "{rel}");
        }
        assert_eq!(urljoin("http://a/", "http://[::1").unwrap_err(), "Invalid IPv6 URL");
    }
}
