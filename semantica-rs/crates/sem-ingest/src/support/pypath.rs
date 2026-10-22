//! `pathlib.PurePosixPath` / `Path` semantics the ingestors observe: `str(Path(s))`
//! normalisation, `.name`, `.suffix`, `.stem`, `.parent`, `.absolute()`, the traversal order
//! of `iterdir()` / `rglob("*")` (scandir order, directories pre-order), `os.walk`, and
//! `fnmatch.fnmatch`.

use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;

/// `str(Path(s))`: collapses repeated slashes and `.` components (`..` is kept).
pub fn normalize(s: &str) -> String {
    if s.is_empty() {
        return ".".to_string();
    }
    let absolute = s.starts_with('/');
    // POSIX keeps exactly two leading slashes.
    let double = s.starts_with("//") && !s.starts_with("///");
    let parts: Vec<&str> = s.split('/').filter(|p| !p.is_empty() && *p != ".").collect();
    let body = parts.join("/");
    if double {
        format!("//{body}")
    } else if absolute {
        format!("/{body}")
    } else if body.is_empty() {
        ".".to_string()
    } else {
        body
    }
}

/// `Path(s).name`.
pub fn name(s: &str) -> String {
    let n = normalize(s);
    if n == "." || n == "/" || n == "//" {
        return String::new();
    }
    n.rsplit('/').next().unwrap_or("").to_string()
}

/// `Path(s).suffix`.
pub fn suffix(s: &str) -> String {
    let name = name(s);
    match name.rfind('.') {
        Some(i) if i > 0 && i < name.len() - 1 => name[i..].to_string(),
        _ => String::new(),
    }
}

/// `Path(s).stem`.
pub fn stem(s: &str) -> String {
    let name = name(s);
    match name.rfind('.') {
        Some(i) if i > 0 && i < name.len() - 1 => name[..i].to_string(),
        _ => name,
    }
}

/// `str(Path(s).parent)`.
pub fn parent(s: &str) -> String {
    let n = normalize(s);
    if n == "/" || n == "//" || n == "." {
        return n;
    }
    match n.rfind('/') {
        Some(0) => "/".to_string(),
        Some(i) => {
            let p = &n[..i];
            if p == "/" || p.is_empty() { "/".to_string() } else { p.to_string() }
        }
        None => ".".to_string(),
    }
}

/// `str(Path(s).absolute())` (relative paths joined to the working directory, no `..`
/// resolution).
pub fn absolute(s: &str) -> String {
    let n = normalize(s);
    if n.starts_with('/') {
        return n;
    }
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    if n == "." {
        return cwd;
    }
    normalize(&format!("{cwd}/{n}"))
}

/// `str(Path(a) / b)`.
pub fn join(a: &str, b: &str) -> String {
    if b.starts_with('/') {
        return normalize(b);
    }
    normalize(&format!("{}/{}", a, b))
}

fn scandir(dir: &Path) -> Vec<fs::DirEntry> {
    match fs::read_dir(dir) {
        Ok(it) => it.filter_map(Result::ok).collect(),
        Err(_) => Vec::new(),
    }
}

fn entry_path(base: &str, entry: &fs::DirEntry) -> String {
    let name = entry.file_name().to_string_lossy().into_owned();
    if base == "." {
        name
    } else {
        join(base, &name)
    }
}

/// `Path(dir).iterdir()` as path strings (scandir order).
pub fn iterdir(dir: &str) -> Vec<String> {
    let base = normalize(dir);
    scandir(Path::new(&base))
        .iter()
        .map(|e| entry_path(&base, e))
        .collect()
}

fn iterate_directories(dir: &str, out: &mut Vec<String>) {
    out.push(dir.to_string());
    for entry in scandir(Path::new(dir)) {
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if is_dir {
            let p = entry_path(dir, &entry);
            iterate_directories(&p, out);
        }
    }
}

/// `Path(dir).rglob("*")` as path strings: for each directory in pre-order (scandir order),
/// its entries in scandir order.
pub fn rglob_all(dir: &str) -> Vec<String> {
    let base = normalize(dir);
    let mut dirs = Vec::new();
    iterate_directories(&base, &mut dirs);
    let mut out = Vec::new();
    for d in dirs {
        out.extend(iterdir(&d));
    }
    out
}

/// One `os.walk(top)` step: `(root, dirnames, filenames)`, top-down.
pub fn walk(top: &str) -> Vec<(String, Vec<String>, Vec<String>)> {
    let mut out = Vec::new();
    let mut stack = vec![top.to_string()];
    while let Some(root) = stack.pop() {
        let mut dirs = Vec::new();
        let mut files = Vec::new();
        for entry in scandir(Path::new(&root)) {
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false)
                || (entry.file_type().map(|t| t.is_symlink()).unwrap_or(false)
                    && entry.path().is_dir());
            if is_dir {
                dirs.push(name);
            } else {
                files.push(name);
            }
        }
        for d in dirs.iter().rev() {
            stack.push(format!("{}/{}", root.trim_end_matches('/'), d));
        }
        out.push((root, dirs, files));
    }
    out
}

/// `fnmatch.translate(pattern)` as an anchored regex.
pub fn fnmatch_regex(pattern: &str) -> Regex {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = String::from("(?s)^");
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        i += 1;
        match c {
            '*' => {
                while i < chars.len() && chars[i] == '*' {
                    i += 1;
                }
                out.push_str(".*");
            }
            '?' => out.push('.'),
            '[' => {
                let mut j = i;
                if j < chars.len() && chars[j] == '!' {
                    j += 1;
                }
                if j < chars.len() && chars[j] == ']' {
                    j += 1;
                }
                while j < chars.len() && chars[j] != ']' {
                    j += 1;
                }
                if j >= chars.len() {
                    out.push_str("\\[");
                } else {
                    let mut stuff: String = chars[i..j].iter().collect();
                    i = j + 1;
                    stuff = stuff.replace('\\', "\\\\");
                    if let Some(rest) = stuff.strip_prefix('!') {
                        stuff = format!("^{rest}");
                    } else if stuff.starts_with('^') || stuff.starts_with('[') {
                        stuff = format!("\\{stuff}");
                    }
                    out.push('[');
                    out.push_str(&stuff);
                    out.push(']');
                }
            }
            other => out.push_str(&regex::escape(&other.to_string())),
        }
    }
    out.push('$');
    Regex::new(&out).unwrap_or_else(|_| Regex::new("^$").expect("valid"))
}

/// `fnmatch.fnmatch(name, pattern)` (case-sensitive on POSIX).
pub fn fnmatch(name: &str, pattern: &str) -> bool {
    fnmatch_regex(pattern).is_match(name)
}

/// `Path(s).exists()`.
pub fn exists(s: &str) -> bool {
    Path::new(s).exists()
}

/// `Path(s).is_file()`.
pub fn is_file(s: &str) -> bool {
    Path::new(s).is_file()
}

/// `Path(s).is_dir()`.
pub fn is_dir(s: &str) -> bool {
    Path::new(s).is_dir()
}

/// A `PathBuf` for a Python path string.
pub fn to_path(s: &str) -> PathBuf {
    PathBuf::from(s)
}

/// `st_mtime` as a float.
pub fn mtime(s: &str) -> Option<f64> {
    let meta = fs::metadata(s).ok()?;
    let modified = meta.modified().ok()?;
    let d = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(d.as_secs_f64())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pure_path_parts() {
        assert_eq!(suffix("file.tar.gz"), ".gz");
        assert_eq!(suffix(".bashrc"), "");
        assert_eq!(suffix("x."), "");
        assert_eq!(name("a//b/"), "b");
        assert_eq!(parent("/a/b"), "/a");
        assert_eq!(parent("b"), ".");
        assert_eq!(normalize("./a/./b//c"), "a/b/c");
        assert_eq!(stem("report.final.pdf"), "report.final");
        assert!(fnmatch("data.csv", "*.csv"));
        assert!(!fnmatch("data.csv", "*.txt"));
        assert!(fnmatch("a1", "a[0-9]"));
    }
}
