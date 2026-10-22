//! Python 3.11's `mimetypes` module: the built-in tables, the `knownfiles` the module reads at
//! `init()` (on macOS `/etc/apache2/mime.types`, which changes several answers), and
//! `guess_type` / `guess_extension` / `guess_all_extensions` with Python's suffix and
//! encoding handling.

use indexmap::IndexMap;
use once_cell::sync::Lazy;

/// `mimetypes.knownfiles`.
pub const KNOWNFILES: &[&str] = &[
    "/etc/mime.types",
    "/etc/httpd/mime.types",
    "/etc/httpd/conf/mime.types",
    "/etc/apache/mime.types",
    "/etc/apache2/mime.types",
    "/usr/local/etc/httpd/conf/mime.types",
    "/usr/local/lib/netscape/mime.types",
    "/usr/local/etc/httpd/conf/mime.types",
    "/usr/local/etc/mime.types",
];

const SUFFIX_MAP: &[(&str, &str)] = &[
    (".svgz", ".svg.gz"),
    (".tgz", ".tar.gz"),
    (".taz", ".tar.gz"),
    (".tz", ".tar.gz"),
    (".tbz2", ".tar.bz2"),
    (".txz", ".tar.xz"),
];

const ENCODINGS_MAP: &[(&str, &str)] = &[
    (".gz", "gzip"),
    (".Z", "compress"),
    (".bz2", "bzip2"),
    (".xz", "xz"),
    (".br", "br"),
];

const TYPES_MAP_STRICT: &[(&str, &str)] = &[
    (".js", "application/javascript"),
    (".mjs", "application/javascript"),
    (".json", "application/json"),
    (".webmanifest", "application/manifest+json"),
    (".doc", "application/msword"),
    (".dot", "application/msword"),
    (".wiz", "application/msword"),
    (".nq", "application/n-quads"),
    (".nt", "application/n-triples"),
    (".bin", "application/octet-stream"),
    (".a", "application/octet-stream"),
    (".dll", "application/octet-stream"),
    (".exe", "application/octet-stream"),
    (".o", "application/octet-stream"),
    (".obj", "application/octet-stream"),
    (".so", "application/octet-stream"),
    (".oda", "application/oda"),
    (".pdf", "application/pdf"),
    (".p7c", "application/pkcs7-mime"),
    (".ps", "application/postscript"),
    (".ai", "application/postscript"),
    (".eps", "application/postscript"),
    (".trig", "application/trig"),
    (".m3u", "application/vnd.apple.mpegurl"),
    (".m3u8", "application/vnd.apple.mpegurl"),
    (".xls", "application/vnd.ms-excel"),
    (".xlb", "application/vnd.ms-excel"),
    (".ppt", "application/vnd.ms-powerpoint"),
    (".pot", "application/vnd.ms-powerpoint"),
    (".ppa", "application/vnd.ms-powerpoint"),
    (".pps", "application/vnd.ms-powerpoint"),
    (".pwz", "application/vnd.ms-powerpoint"),
    (".wasm", "application/wasm"),
    (".bcpio", "application/x-bcpio"),
    (".cpio", "application/x-cpio"),
    (".csh", "application/x-csh"),
    (".dvi", "application/x-dvi"),
    (".gtar", "application/x-gtar"),
    (".hdf", "application/x-hdf"),
    (".h5", "application/x-hdf5"),
    (".latex", "application/x-latex"),
    (".mif", "application/x-mif"),
    (".cdf", "application/x-netcdf"),
    (".nc", "application/x-netcdf"),
    (".p12", "application/x-pkcs12"),
    (".pfx", "application/x-pkcs12"),
    (".ram", "application/x-pn-realaudio"),
    (".pyc", "application/x-python-code"),
    (".pyo", "application/x-python-code"),
    (".sh", "application/x-sh"),
    (".shar", "application/x-shar"),
    (".swf", "application/x-shockwave-flash"),
    (".sv4cpio", "application/x-sv4cpio"),
    (".sv4crc", "application/x-sv4crc"),
    (".tar", "application/x-tar"),
    (".tcl", "application/x-tcl"),
    (".tex", "application/x-tex"),
    (".texi", "application/x-texinfo"),
    (".texinfo", "application/x-texinfo"),
    (".roff", "application/x-troff"),
    (".t", "application/x-troff"),
    (".tr", "application/x-troff"),
    (".man", "application/x-troff-man"),
    (".me", "application/x-troff-me"),
    (".ms", "application/x-troff-ms"),
    (".ustar", "application/x-ustar"),
    (".src", "application/x-wais-source"),
    (".xsl", "application/xml"),
    (".rdf", "application/xml"),
    (".wsdl", "application/xml"),
    (".xpdl", "application/xml"),
    (".zip", "application/zip"),
    (".3gp", "audio/3gpp"),
    (".3gpp", "audio/3gpp"),
    (".3g2", "audio/3gpp2"),
    (".3gpp2", "audio/3gpp2"),
    (".aac", "audio/aac"),
    (".adts", "audio/aac"),
    (".loas", "audio/aac"),
    (".ass", "audio/aac"),
    (".au", "audio/basic"),
    (".snd", "audio/basic"),
    (".mp3", "audio/mpeg"),
    (".mp2", "audio/mpeg"),
    (".opus", "audio/opus"),
    (".aif", "audio/x-aiff"),
    (".aifc", "audio/x-aiff"),
    (".aiff", "audio/x-aiff"),
    (".ra", "audio/x-pn-realaudio"),
    (".wav", "audio/x-wav"),
    (".avif", "image/avif"),
    (".bmp", "image/bmp"),
    (".gif", "image/gif"),
    (".ief", "image/ief"),
    (".jpg", "image/jpeg"),
    (".jpe", "image/jpeg"),
    (".jpeg", "image/jpeg"),
    (".heic", "image/heic"),
    (".heif", "image/heif"),
    (".png", "image/png"),
    (".svg", "image/svg+xml"),
    (".tiff", "image/tiff"),
    (".tif", "image/tiff"),
    (".ico", "image/vnd.microsoft.icon"),
    (".ras", "image/x-cmu-raster"),
    (".pnm", "image/x-portable-anymap"),
    (".pbm", "image/x-portable-bitmap"),
    (".pgm", "image/x-portable-graymap"),
    (".ppm", "image/x-portable-pixmap"),
    (".rgb", "image/x-rgb"),
    (".xbm", "image/x-xbitmap"),
    (".xpm", "image/x-xpixmap"),
    (".xwd", "image/x-xwindowdump"),
    (".eml", "message/rfc822"),
    (".mht", "message/rfc822"),
    (".mhtml", "message/rfc822"),
    (".nws", "message/rfc822"),
    (".css", "text/css"),
    (".csv", "text/csv"),
    (".html", "text/html"),
    (".htm", "text/html"),
    (".n3", "text/n3"),
    (".txt", "text/plain"),
    (".bat", "text/plain"),
    (".c", "text/plain"),
    (".h", "text/plain"),
    (".ksh", "text/plain"),
    (".pl", "text/plain"),
    (".srt", "text/plain"),
    (".rtx", "text/richtext"),
    (".tsv", "text/tab-separated-values"),
    (".vtt", "text/vtt"),
    (".py", "text/x-python"),
    (".etx", "text/x-setext"),
    (".sgm", "text/x-sgml"),
    (".sgml", "text/x-sgml"),
    (".vcf", "text/x-vcard"),
    (".xml", "text/xml"),
    (".mp4", "video/mp4"),
    (".mpeg", "video/mpeg"),
    (".m1v", "video/mpeg"),
    (".mpa", "video/mpeg"),
    (".mpe", "video/mpeg"),
    (".mpg", "video/mpeg"),
    (".mov", "video/quicktime"),
    (".qt", "video/quicktime"),
    (".webm", "video/webm"),
    (".avi", "video/x-msvideo"),
    (".movie", "video/x-sgi-movie"),
];
const TYPES_MAP_LOOSE: &[(&str, &str)] = &[
    (".rtf", "application/rtf"),
    (".midi", "audio/midi"),
    (".mid", "audio/midi"),
    (".jpg", "image/jpg"),
    (".pict", "image/pict"),
    (".pct", "image/pict"),
    (".pic", "image/pict"),
    (".webp", "image/webp"),
    (".xul", "text/xul"),
];

/// The `MimeTypes` database after `init()`.
pub struct MimeDb {
    types_map: [IndexMap<String, String>; 2],
    types_map_inv: [IndexMap<String, Vec<String>>; 2],
}

impl MimeDb {
    fn add_type(&mut self, mime: &str, ext: &str, strict: bool) {
        let idx = usize::from(strict);
        self.types_map[idx].insert(ext.to_string(), mime.to_string());
        let exts = self.types_map_inv[idx].entry(mime.to_string()).or_default();
        if !exts.iter().any(|e| e == ext) {
            exts.push(ext.to_string());
        }
    }

    /// The built-in tables only (`MimeTypes(filenames=())`).
    pub fn builtin() -> MimeDb {
        let mut db = MimeDb {
            types_map: [IndexMap::new(), IndexMap::new()],
            types_map_inv: [IndexMap::new(), IndexMap::new()],
        };
        for (ext, mime) in TYPES_MAP_STRICT {
            db.add_type(mime, ext, true);
        }
        for (ext, mime) in TYPES_MAP_LOOSE {
            db.add_type(mime, ext, false);
        }
        db
    }

    /// `MimeTypes.readfp`: adds the entries of a `mime.types` file.
    pub fn read_mime_types_text(&mut self, text: &str, strict: bool) {
        for line in text.lines() {
            let mut words: Vec<&str> = line.split_whitespace().collect();
            if let Some(pos) = words.iter().position(|w| w.starts_with('#')) {
                words.truncate(pos);
            }
            if words.is_empty() {
                continue;
            }
            let mime = words[0];
            for suffix in &words[1..] {
                self.add_type(mime, &format!(".{suffix}"), strict);
            }
        }
    }

    /// `guess_type(url, strict=True)`.
    pub fn guess_type(&self, url: &str, strict: bool) -> (Option<String>, Option<String>) {
        let parsed = super::pyurl::urlparse(url).ok();
        let path = match &parsed {
            Some(p) if p.scheme.len() > 1 => {
                if p.scheme == "data" {
                    let path = p.path.clone();
                    let Some(comma) = path.find(',') else {
                        return (None, None);
                    };
                    let head = &path[..comma];
                    let mime = match head.find(';') {
                        Some(semi) => &head[..semi],
                        None => head,
                    };
                    let mime = if mime.contains('=') || !mime.contains('/') {
                        "text/plain"
                    } else {
                        mime
                    };
                    return (Some(mime.to_string()), None);
                }
                p.path.clone()
            }
            _ => url.to_string(),
        };
        let (mut base, mut ext) = splitext(&path);
        while let Some((_, target)) = SUFFIX_MAP.iter().find(|(s, _)| *s == ext.to_lowercase()) {
            let joined = format!("{base}{target}");
            let (b, e) = splitext(&joined);
            base = b;
            ext = e;
        }
        let encoding = match ENCODINGS_MAP.iter().find(|(s, _)| *s == ext) {
            Some((_, enc)) => {
                let (b, e) = splitext(&base);
                base = b;
                ext = e;
                Some(enc.to_string())
            }
            None => None,
        };
        let _ = base;
        let ext = ext.to_lowercase();
        if let Some(t) = self.types_map[1].get(&ext) {
            return (Some(t.clone()), encoding);
        }
        if strict {
            return (None, encoding);
        }
        (self.types_map[0].get(&ext).cloned(), encoding)
    }

    /// `guess_all_extensions(type, strict=True)`.
    pub fn guess_all_extensions(&self, mime: &str, strict: bool) -> Vec<String> {
        let mime = mime.to_lowercase();
        let mut exts = self.types_map_inv[1].get(&mime).cloned().unwrap_or_default();
        if !strict {
            for e in self.types_map_inv[0].get(&mime).cloned().unwrap_or_default() {
                if !exts.contains(&e) {
                    exts.push(e);
                }
            }
        }
        exts
    }

    /// `guess_extension(type, strict=True)`.
    pub fn guess_extension(&self, mime: &str, strict: bool) -> Option<String> {
        self.guess_all_extensions(mime, strict).into_iter().next()
    }
}

/// `posixpath.splitext`.
pub fn splitext(path: &str) -> (String, String) {
    let sep = path.rfind('/').map(|i| i + 1).unwrap_or(0);
    let name = &path[sep..];
    let leading_dots = name.chars().take_while(|&c| c == '.').count();
    match name[leading_dots..].rfind('.') {
        Some(i) => {
            let dot = sep + leading_dots + i;
            (path[..dot].to_string(), path[dot..].to_string())
        }
        None => (path.to_string(), String::new()),
    }
}

static DB: Lazy<MimeDb> = Lazy::new(|| {
    let mut db = MimeDb::builtin();
    for file in KNOWNFILES {
        if let Ok(text) = std::fs::read_to_string(file) {
            db.read_mime_types_text(&text, true);
        }
    }
    db
});

/// The module-level database (`mimetypes.init()` on first use).
pub fn db() -> &'static MimeDb {
    &DB
}

/// `mimetypes.guess_type(url)`.
pub fn guess_type(url: &str) -> (Option<String>, Option<String>) {
    DB.guess_type(url, true)
}

/// `mimetypes.guess_extension(type)`.
pub fn guess_extension(mime: &str) -> Option<String> {
    DB.guess_extension(mime, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_guesses() {
        let db = MimeDb::builtin();
        assert_eq!(db.guess_type("x.txt", true).0.as_deref(), Some("text/plain"));
        assert_eq!(
            db.guess_type("x.tar.gz", true),
            (Some("application/x-tar".to_string()), Some("gzip".to_string()))
        );
        assert_eq!(db.guess_extension("video/mp4", true).as_deref(), Some(".mp4"));
        assert_eq!(splitext(".bashrc"), (".bashrc".to_string(), String::new()));
        assert_eq!(splitext("a/b.c.d"), ("a/b.c".to_string(), ".d".to_string()));
    }
}
