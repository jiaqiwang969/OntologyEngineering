//! Open Packaging Convention reader: the part graph python-docx (`docx.opc`) and
//! python-pptx (`pptx.opc`) load from a zip package, with their failure messages.
//!
//! Loading follows each library: the package relationships are walked depth-first from
//! `/_rels/.rels`, every internal target becomes a part with the content type
//! `[Content_Types].xml` gives it, and the XML parts the library registers are parsed
//! eagerly (so malformed XML fails at load time, as in the originals). python-docx fails
//! on a relationship target missing from the archive; python-pptx skips it.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use indexmap::IndexMap;

use super::dom::{self, Element};

/// The package-relationships namespace.
pub(crate) const NS_PKG_RELS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
/// The content-types namespace.
pub(crate) const NS_CT: &str = "http://schemas.openxmlformats.org/package/2006/content-types";

/// Relationship types.
pub(crate) mod rt {
    pub const OFFICE_DOCUMENT: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
    pub const CORE_PROPERTIES: &str =
        "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties";
    pub const STYLES: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles";
    pub const NUMBERING: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering";
    pub const IMAGE: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
    #[allow(dead_code)] // kept for the Docling office backends
    pub const SLIDE: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide";
    pub const SLIDE_LAYOUT: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout";
    pub const SLIDE_MASTER: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster";
    pub const NOTES_SLIDE: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesSlide";
}

/// Content types.
pub(crate) mod ct {
    pub const OPC_CORE_PROPERTIES: &str = "application/vnd.openxmlformats-package.core-properties+xml";
    pub const WML_DOCUMENT_MAIN: &str =
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
    pub const PML_PRESENTATION_MAIN: &str =
        "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml";
    pub const PML_PRES_MACRO_MAIN: &str = "application/vnd.ms-powerpoint.presentation.macroEnabled.main+xml";
}

/// Which library's loading rules to apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Flavor {
    /// python-docx 1.2.0.
    Docx,
    /// python-pptx 1.0.2.
    Pptx,
}

/// A relationship from a source part (or the package).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Relationship {
    pub r_id: String,
    pub reltype: String,
    pub target_ref: String,
    pub external: bool,
    /// The absolute part name of an internal target.
    pub target_partname: Option<String>,
}

/// A loaded part.
#[derive(Debug, Clone)]
pub(crate) struct Part {
    pub content_type: String,
    pub blob: Vec<u8>,
    pub rels: Vec<Relationship>,
    /// The parsed XML of the parts the library parses at load time.
    pub xml: Option<Element>,
}

/// A loaded package.
#[derive(Debug, Clone)]
pub(crate) struct Package {
    pub rels: Vec<Relationship>,
    pub parts: IndexMap<String, Part>,
}

/// `posixpath.normpath` for absolute paths (a leading `//` is kept, as POSIX allows).
pub(crate) fn normpath(path: &str) -> String {
    if path.is_empty() {
        return ".".to_string();
    }
    let initial = if path.starts_with("//") && !path.starts_with("///") {
        "//"
    } else if path.starts_with('/') {
        "/"
    } else {
        ""
    };
    let mut parts: Vec<&str> = Vec::new();
    for comp in path.split('/') {
        if comp.is_empty() || comp == "." {
            continue;
        }
        if comp == ".." {
            if !initial.is_empty() && parts.is_empty() {
                continue;
            }
            if parts.last().is_some_and(|p| *p != "..") {
                parts.pop();
                continue;
            }
        }
        parts.push(comp);
    }
    let joined = parts.join("/");
    let out = format!("{initial}{joined}");
    if out.is_empty() { ".".to_string() } else { out }
}

/// `PackURI.from_rel_ref(baseURI, ref)`: `posixpath.abspath(posixpath.join(base, ref))`.
fn from_rel_ref(base_uri: &str, target_ref: &str) -> String {
    let joined = if target_ref.starts_with('/') {
        target_ref.to_string()
    } else if base_uri.ends_with('/') {
        format!("{base_uri}{target_ref}")
    } else {
        format!("{base_uri}/{target_ref}")
    };
    normpath(&joined)
}

/// `PackURI.baseURI`: the directory portion.
fn base_uri(partname: &str) -> String {
    match partname.rfind('/') {
        Some(0) => "/".to_string(),
        Some(i) => partname[..i].to_string(),
        None => String::new(),
    }
}

/// `PackURI.rels_uri`.
fn rels_uri(partname: &str) -> String {
    let base = base_uri(partname);
    let filename = &partname[partname.rfind('/').map(|i| i + 1).unwrap_or(0)..];
    if base.ends_with('/') {
        format!("{base}_rels/{filename}.rels")
    } else {
        format!("{base}/_rels/{filename}.rels")
    }
}

/// `PackURI.ext` (without the dot).
fn partname_ext(partname: &str) -> &str {
    let filename = &partname[partname.rfind('/').map(|i| i + 1).unwrap_or(0)..];
    // posixpath.splitext ignores leading dots of the file name.
    let trimmed = filename.trim_start_matches('.');
    match trimmed.rfind('.') {
        Some(i) => &trimmed[i + 1..],
        None => "",
    }
}

struct ContentTypes {
    overrides: HashMap<String, String>,
    defaults: HashMap<String, String>,
}

impl ContentTypes {
    fn from_xml(xml: &[u8]) -> Result<ContentTypes, String> {
        let root = dom::parse(xml)?;
        let mut overrides = HashMap::new();
        let mut defaults = HashMap::new();
        for e in root.children_named(NS_CT, "Override") {
            if let (Some(p), Some(c)) = (e.attr(None, "PartName"), e.attr(None, "ContentType")) {
                overrides.insert(p.to_lowercase(), c.to_string());
            }
        }
        for e in root.children_named(NS_CT, "Default") {
            if let (Some(x), Some(c)) = (e.attr(None, "Extension"), e.attr(None, "ContentType")) {
                defaults.insert(x.to_lowercase(), c.to_string());
            }
        }
        Ok(ContentTypes {
            overrides,
            defaults,
        })
    }

    fn get(&self, partname: &str, flavor: Flavor) -> Result<String, String> {
        if let Some(c) = self.overrides.get(&partname.to_lowercase()) {
            return Ok(c.clone());
        }
        if let Some(c) = self.defaults.get(&partname_ext(partname).to_lowercase()) {
            return Ok(c.clone());
        }
        Err(match flavor {
            Flavor::Docx => format!(
                "\"no content type for partname '{partname}' in [Content_Types].xml\""
            ),
            Flavor::Pptx => format!(
                "\"no content-type for partname '{partname}' in [Content_Types].xml\""
            ),
        })
    }
}

/// `str(KeyError(msg))`: the message's `repr()`.
fn key_error(msg: &str) -> String {
    sem_utils::pycompat::fmt::str_repr(msg)
}

struct Zip {
    members: HashMap<String, Vec<u8>>,
}

impl Zip {
    fn open(path: &Path, display: &str) -> Result<Zip, String> {
        let not_found = || format!("Package not found at '{display}'");
        let file = std::fs::File::open(path).map_err(|_| not_found())?;
        let mut archive = zip::ZipArchive::new(file).map_err(|_| not_found())?;
        let mut members = HashMap::new();
        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| format!("Bad zip member: {e}"))?;
            if entry.is_dir() {
                continue;
            }
            let name = entry.name().to_string();
            let mut buf = Vec::new();
            entry
                .read_to_end(&mut buf)
                .map_err(|e| format!("Bad CRC-32 for file '{name}': {e}"))?;
            members.insert(name, buf);
        }
        Ok(Zip { members })
    }

    /// The blob for an absolute part name.
    fn get(&self, partname: &str) -> Option<&Vec<u8>> {
        self.members.get(partname.strip_prefix('/').unwrap_or(partname))
    }
}

fn parse_rels(zip: &Zip, source_partname: &str) -> Result<Vec<Relationship>, String> {
    let uri = if source_partname == "/" {
        "/_rels/.rels".to_string()
    } else {
        rels_uri(source_partname)
    };
    let Some(blob) = zip.get(&uri) else {
        return Ok(Vec::new());
    };
    let root = dom::parse(blob)?;
    let base = if source_partname == "/" {
        "/".to_string()
    } else {
        base_uri(source_partname)
    };
    let mut rels = Vec::new();
    for e in root.children_named(NS_PKG_RELS, "Relationship") {
        let r_id = e.attr(None, "Id").unwrap_or_default().to_string();
        let reltype = e.attr(None, "Type").unwrap_or_default().to_string();
        let target_ref = e.attr(None, "Target").unwrap_or_default().to_string();
        let external = e.attr(None, "TargetMode") == Some("External");
        let target_partname = if external {
            None
        } else {
            Some(from_rel_ref(&base, &target_ref))
        };
        rels.push(Relationship {
            r_id,
            reltype,
            target_ref,
            external,
            target_partname,
        });
    }
    Ok(rels)
}

/// The XML content types each library parses when loading a package.
fn parses_xml(flavor: Flavor, content_type: &str, reltype: &str) -> bool {
    match flavor {
        Flavor::Docx => {
            reltype != rt::IMAGE
                && matches!(
                    content_type,
                    ct::OPC_CORE_PROPERTIES
                        | "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml"
                        | ct::WML_DOCUMENT_MAIN
                        | "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"
                        | "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"
                        | "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"
                        | "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"
                        | "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"
                )
        }
        Flavor::Pptx => matches!(
            content_type,
            ct::OPC_CORE_PROPERTIES
                | ct::PML_PRESENTATION_MAIN
                | ct::PML_PRES_MACRO_MAIN
                | "application/vnd.openxmlformats-officedocument.presentationml.template.main+xml"
                | "application/vnd.openxmlformats-officedocument.presentationml.slideshow.main+xml"
                | "application/vnd.openxmlformats-officedocument.presentationml.slide+xml"
                | "application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"
                | "application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"
                | "application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml"
                | "application/vnd.openxmlformats-officedocument.presentationml.notesMaster+xml"
                | "application/vnd.openxmlformats-officedocument.drawingml.chart+xml"
        ),
    }
}

impl Package {
    /// Load the package at `path` (`display` is the path text the library reports).
    pub(crate) fn open(path: &Path, display: &str, flavor: Flavor) -> Result<Package, String> {
        let zip = Zip::open(path, display)?;
        let ct_blob = zip.get("/[Content_Types].xml").ok_or_else(|| match flavor {
            Flavor::Docx => key_error("There is no item named '[Content_Types].xml' in the archive"),
            Flavor::Pptx => key_error("no member '/[Content_Types].xml' in package"),
        })?;
        let content_types = ContentTypes::from_xml(ct_blob)?;
        let pkg_rels = parse_rels(&zip, "/")?;
        let mut parts: IndexMap<String, Part> = IndexMap::new();
        let mut reltypes: HashMap<String, String> = HashMap::new();
        // Depth-first walk of the relationship graph.
        let mut stack: Vec<(Vec<Relationship>, usize)> = vec![(pkg_rels.clone(), 0)];
        while let Some((rels, idx)) = stack.pop() {
            if idx >= rels.len() {
                continue;
            }
            stack.push((rels.clone(), idx + 1));
            let rel = &rels[idx];
            let Some(partname) = rel.target_partname.clone() else {
                continue;
            };
            if parts.contains_key(&partname) {
                continue;
            }
            let blob = match zip.get(&partname) {
                Some(b) => b.clone(),
                None => match flavor {
                    Flavor::Docx => {
                        return Err(key_error(&format!(
                            "There is no item named '{}' in the archive",
                            partname.trim_start_matches('/')
                        )));
                    }
                    // python-pptx ignores relationships to parts not in the package.
                    Flavor::Pptx => continue,
                },
            };
            let part_rels = parse_rels(&zip, &partname)?;
            let content_type = content_types.get(&partname, flavor)?;
            reltypes.insert(partname.clone(), rel.reltype.clone());
            parts.insert(
                partname.clone(),
                Part {
                    content_type,
                    blob,
                    rels: part_rels.clone(),
                    xml: None,
                },
            );
            stack.push((part_rels, 0));
        }
        for (name, part) in parts.iter_mut() {
            let reltype = reltypes.get(name).map(String::as_str).unwrap_or("");
            if parses_xml(flavor, &part.content_type, reltype) {
                part.xml = Some(dom::parse(&part.blob)?);
            }
        }
        Ok(Package {
            rels: pkg_rels,
            parts,
        })
    }

    /// `part_related_by(reltype)` from the package: the single target part name.
    pub(crate) fn related(&self, rels: &[Relationship], reltype: &str) -> Result<String, String> {
        let matches: Vec<&Relationship> = rels
            .iter()
            .filter(|r| r.reltype == reltype && !r.external)
            .filter(|r| {
                r.target_partname
                    .as_ref()
                    .is_some_and(|p| self.parts.contains_key(p))
            })
            .collect();
        match matches.len() {
            0 => Err(key_error(&format!(
                "no relationship of type '{reltype}' in collection"
            ))),
            1 => Ok(matches[0].target_partname.clone().unwrap_or_default()),
            _ => Err(format!(
                "multiple relationships of type '{reltype}' in collection"
            )),
        }
    }

    /// The main document part name (`RT.OFFICE_DOCUMENT` from the package).
    pub(crate) fn main_document_part(&self) -> Result<String, String> {
        self.related(&self.rels, rt::OFFICE_DOCUMENT)
    }

    /// A part by name.
    pub(crate) fn part(&self, name: &str) -> Option<&Part> {
        self.parts.get(name)
    }

    /// The part a part's relationship `r_id` targets.
    pub(crate) fn related_by_id(&self, source: &str, r_id: &str) -> Option<&str> {
        let part = self.parts.get(source)?;
        part.rels
            .iter()
            .find(|r| r.r_id == r_id && !r.external)
            .and_then(|r| r.target_partname.as_deref())
            .filter(|p| self.parts.contains_key(*p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_uri_helpers_follow_posixpath() {
        assert_eq!(normpath("/word/../word/./document.xml"), "/word/document.xml");
        assert_eq!(normpath("//word/x"), "//word/x");
        assert_eq!(normpath("/../a"), "/a");
        assert_eq!(from_rel_ref("/word", "media/image1.png"), "/word/media/image1.png");
        assert_eq!(from_rel_ref("/ppt/slides", "../slideLayouts/l1.xml"), "/ppt/slideLayouts/l1.xml");
        assert_eq!(from_rel_ref("/", "word/document.xml"), "/word/document.xml");
        assert_eq!(rels_uri("/word/document.xml"), "/word/_rels/document.xml.rels");
        assert_eq!(partname_ext("/word/document.xml"), "xml");
        assert_eq!(partname_ext("/a/.rels"), "");
    }
}
