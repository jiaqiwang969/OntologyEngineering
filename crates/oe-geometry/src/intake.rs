//! Bounded development STEP intake: exact bytes and lexical header/entity hints.
//! These checks do not parse full Part 21 or validate topology, units or geometry.

use anyhow::{Result, ensure};
use oe_core::sha256;
use serde_json::{Value, json};
use std::path::Path;

pub const MAX_STEP_BYTES: u64 = 2 * 1024 * 1024 * 1024; // 2 GiB hard limit
pub const STEP_SCHEMA: &str = "ontology-engineering.step-preflight/v1";

const SCHEMA_FAMILIES: &[(&str, &[&[u8]])] = &[
    ("AP203", &[b"AP203", b"CONFIG_CONTROL_DESIGN"]),
    ("AP214", &[b"AP214", b"AUTOMOTIVE_DESIGN"]),
    ("AP242", &[b"AP242", b"MANAGED_MODEL_BASED_3D_ENGINEERING"]),
];

// STEP entity names tracked in preflight
const TRACKED_ENTITIES: &[&str] = &[
    "PRODUCT",
    "PRODUCT_DEFINITION",
    "NEXT_ASSEMBLY_USAGE_OCCURRENCE",
    "SPECIFIED_HIGHER_USAGE_OCCURRENCE",
    "MAPPED_ITEM",
    "ITEM_DEFINED_TRANSFORMATION",
    "CARTESIAN_TRANSFORMATION_OPERATOR",
    "AXIS2_PLACEMENT_3D",
    "MANIFOLD_SOLID_BREP",
    "BREP_WITH_VOIDS",
    "CLOSED_SHELL",
    "OPEN_SHELL",
    "ADVANCED_FACE",
];

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct StepPreflight {
    pub path: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub schema_family: Option<String>,
    pub entity_counts: std::collections::BTreeMap<String, u64>,
    pub status: String,
    pub errors: Vec<String>,
}

impl StepPreflight {
    pub fn to_json(&self) -> Value {
        json!({
            "schema": STEP_SCHEMA,
            "path": self.path,
            "sha256": self.sha256,
            "size_bytes": self.size_bytes,
            "schema_family": self.schema_family,
            "entity_counts": self.entity_counts,
            "status": self.status,
            "errors": self.errors,
            "inspection_scope": "lexical_header_and_entity_hints",
            "native_geometry_validation": "not_run",
        })
    }
}

// ---------------------------------------------------------------------------
// Preflight
// ---------------------------------------------------------------------------

pub fn run(step_path: &Path) -> Result<StepPreflight> {
    ensure!(
        !step_path.is_symlink(),
        "STEP file must not be a symbolic link"
    );
    ensure!(
        step_path.is_file(),
        "STEP file not found: {}",
        step_path.display()
    );

    let meta = std::fs::metadata(step_path)?;
    let size = meta.len();
    ensure!(
        size <= MAX_STEP_BYTES,
        "STEP file exceeds size limit ({size} > {MAX_STEP_BYTES})"
    );

    let bytes = oe_core::read_regular_bytes(step_path, usize::try_from(MAX_STEP_BYTES)?)?;
    ensure!(
        bytes.len() as u64 == size,
        "STEP file changed after size observation"
    );
    let file_sha256 = sha256(&bytes);

    let mut errors = Vec::new();

    // Detect P21 header
    if !bytes.starts_with(b"ISO-10303-21") {
        errors.push("missing ISO-10303-21 header".to_owned());
    }

    // Schema family detection via FILE_SCHEMA scan
    let schema_family = detect_schema_family(&bytes);
    if schema_family.is_none() {
        errors.push("could not identify STEP schema family (AP203/AP214/AP242)".to_owned());
    }

    // Entity counting
    let entity_counts = count_entities(&bytes);

    let status = if errors.is_empty() { "ok" } else { "warnings" }.to_owned();

    Ok(StepPreflight {
        path: step_path.to_string_lossy().into_owned(),
        sha256: file_sha256,
        size_bytes: size,
        schema_family,
        entity_counts,
        status,
        errors,
    })
}

// ---------------------------------------------------------------------------
// Schema family detection
// ---------------------------------------------------------------------------

fn detect_schema_family(bytes: &[u8]) -> Option<String> {
    // Stop at the bounded header. A comment or description is not a declaration.
    let header = &bytes[..bytes.len().min(65536)];
    let mut tokens = Tokens::new(header);
    let mut families = std::collections::BTreeSet::new();
    while let Some(token) = tokens.next() {
        if matches!(token, Token::Name(value) if value.eq_ignore_ascii_case(b"ENDSEC")) {
            break;
        }
        if !matches!(token, Token::Name(value) if value.eq_ignore_ascii_case(b"FILE_SCHEMA")) {
            continue;
        }
        if !matches!(tokens.next(), Some(Token::Symbol(b'('))) {
            return None;
        }
        let mut depth = 1usize;
        let mut complete = false;
        for token in tokens.by_ref() {
            match token {
                Token::Symbol(b'(') => depth += 1,
                Token::Symbol(b')') => {
                    depth -= 1;
                    if depth == 0 {
                        complete = true;
                        break;
                    }
                }
                Token::Quoted(value) => {
                    for (family, aliases) in SCHEMA_FAMILIES {
                        if aliases.iter().any(|alias| {
                            value.len() >= alias.len()
                                && value[..alias.len()].eq_ignore_ascii_case(alias)
                                && (value.len() == alias.len()
                                    || value[alias.len()] == b'_'
                                    || value[alias.len()].is_ascii_digit())
                        }) {
                            families.insert(*family);
                        }
                    }
                }
                _ => (),
            }
        }
        if !complete {
            return None;
        }
    }
    if families.len() == 1 {
        families.first().map(|v| (*v).to_owned())
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Entity counting — fast byte scan, no full P21 parser
// ---------------------------------------------------------------------------

fn count_entities(bytes: &[u8]) -> std::collections::BTreeMap<String, u64> {
    let mut counts = std::collections::BTreeMap::new();
    let mut pending: Option<&str> = None;
    for token in Tokens::new(bytes) {
        if matches!(token, Token::Symbol(b'('))
            && let Some(name) = pending.take()
        {
            *counts.entry(name.to_owned()).or_insert(0) += 1;
        }
        pending = match token {
            Token::Name(value) => TRACKED_ENTITIES
                .iter()
                .copied()
                .find(|name| value.eq_ignore_ascii_case(name.as_bytes())),
            _ => None,
        };
    }
    counts
}

#[derive(Clone, Copy)]
enum Token<'a> {
    Name(&'a [u8]),
    Quoted(&'a [u8]),
    Symbol(u8),
}
struct Tokens<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Tokens<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }
}
impl<'a> Iterator for Tokens<'a> {
    type Item = Token<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        while self.at < self.bytes.len() {
            let at = self.at;
            let value = self.bytes[at];
            self.at += 1;
            if value.is_ascii_whitespace() {
                continue;
            }
            if value == b'/' && self.bytes.get(self.at) == Some(&b'*') {
                self.at += 1;
                let end = self.bytes[self.at..].windows(2).position(|v| v == b"*/");
                self.at = end.map_or(self.bytes.len(), |v| self.at + v + 2);
                continue;
            }
            if value == b'\'' || value == b'"' {
                let start = self.at;
                while self.at < self.bytes.len() {
                    let pos = self.at;
                    self.at += 1;
                    if self.bytes[pos] == value {
                        if self.bytes.get(self.at) == Some(&value) {
                            self.at += 1;
                        } else {
                            return Some(Token::Quoted(&self.bytes[start..pos]));
                        }
                    }
                }
                return None;
            }
            if value.is_ascii_alphabetic() || value == b'_' {
                while self
                    .bytes
                    .get(self.at)
                    .is_some_and(|v| v.is_ascii_alphanumeric() || *v == b'_')
                {
                    self.at += 1;
                }
                return Some(Token::Name(&self.bytes[at..self.at]));
            }
            return Some(Token::Symbol(value));
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptions_comments_and_unknown_schema_suffixes_do_not_declare_a_family() {
        for bytes in [
            b"FILE_DESCRIPTION(('AP242'), '2;1'); FILE_SCHEMA(('OTHER'));".as_slice(),
            b"/* FILE_SCHEMA(('AP203')); */ FILE_SCHEMA(('OTHER'));",
            b"FILE_SCHEMA(('AP203_FAKE')); FILE_SCHEMA(('AP214_IS'));",
            b"FILE_SCHEMA(('FAKEAP242'));",
            b"'FILE_SCHEMA((AP203))';",
            b"FILE_SCHEMA(('AP203')",
        ] {
            assert!(detect_schema_family(bytes).is_none());
        }
    }
    #[test]
    fn lexical_counts_exclude_comments_strings_and_longer_names() {
        let bytes = b"/* PRODUCT() */ #1=MY_PRODUCT(); #2=product /* gap */ ('PRODUCT(''text'); #3=PRODUCT_DEFINITION(); #4='PRODUCT()';";
        let counts = count_entities(bytes);
        assert_eq!(counts.get("PRODUCT"), Some(&1));
        assert_eq!(counts.get("PRODUCT_DEFINITION"), Some(&1));
    }

    // --- detect_schema_family ---

    #[test]
    fn detects_ap203_by_keyword() {
        let bytes = b"ISO-10303-21;\nFILE_SCHEMA(('AP203'))\n";
        assert_eq!(detect_schema_family(bytes).as_deref(), Some("AP203"));
    }

    #[test]
    fn detects_ap203_by_alias() {
        let bytes = b"ISO-10303-21;\nFILE_SCHEMA(('CONFIG_CONTROL_DESIGN'))\n";
        assert_eq!(detect_schema_family(bytes).as_deref(), Some("AP203"));
    }

    #[test]
    fn detects_ap214() {
        let bytes = b"ISO-10303-21;\nFILE_SCHEMA(('AP214_IS'))\n";
        assert_eq!(detect_schema_family(bytes).as_deref(), Some("AP214"));
    }

    #[test]
    fn detects_ap242() {
        let bytes = b"ISO-10303-21;\nFILE_SCHEMA(('MANAGED_MODEL_BASED_3D_ENGINEERING'))\n";
        assert_eq!(detect_schema_family(bytes).as_deref(), Some("AP242"));
    }

    #[test]
    fn returns_none_for_unknown_schema() {
        let bytes = b"ISO-10303-21;\nFILE_SCHEMA(('SOME_UNKNOWN_SCHEMA'))\n";
        assert!(detect_schema_family(bytes).is_none());
    }

    #[test]
    fn detection_is_case_insensitive() {
        // detect_schema_family uppercases bytes before scan
        let bytes = b"ISO-10303-21;\nFILE_SCHEMA(('ap203'))\n";
        assert_eq!(detect_schema_family(bytes).as_deref(), Some("AP203"));
    }

    // --- count_entities ---

    #[test]
    fn counts_product_entities() {
        let bytes = b"#1=PRODUCT('id','name','',(#2));\n#2=PRODUCT('id2','n','',(#3));\n";
        let counts = count_entities(bytes);
        assert_eq!(counts.get("PRODUCT"), Some(&2u64));
    }

    #[test]
    fn counts_multiple_entity_types() {
        let bytes = b"#1=PRODUCT('id','name','',(#2));\n#2=PRODUCT_DEFINITION('','',(#3),(#4));\n";
        let counts = count_entities(bytes);
        assert_eq!(counts.get("PRODUCT"), Some(&1u64));
        assert_eq!(counts.get("PRODUCT_DEFINITION"), Some(&1u64));
    }

    #[test]
    fn does_not_count_absent_entities() {
        let bytes = b"#1=PRODUCT('id','name','',(#2));\n";
        let counts = count_entities(bytes);
        assert!(!counts.contains_key("MANIFOLD_SOLID_BREP"));
    }

    #[test]
    fn entity_count_zero_is_omitted() {
        let bytes = b"";
        let counts = count_entities(bytes);
        assert!(counts.is_empty());
    }

    // --- P21 header check (via run) ---

    #[test]
    fn run_rejects_symlink() {
        // Creating a real symlink in a temp dir — skip if OS doesn't support it
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().canonicalize().unwrap();
        let target = real.join("real.stp");
        std::fs::write(&target, b"ISO-10303-21;\n").unwrap();
        let link = real.join("link.stp");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&target, &link).unwrap();
            let result = run(&link);
            assert!(result.is_err());
            let msg = format!("{:#}", result.unwrap_err());
            assert!(
                msg.contains("symbolic link"),
                "expected symlink error, got: {msg}"
            );
        }
    }
}
