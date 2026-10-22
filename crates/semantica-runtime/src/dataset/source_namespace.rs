//! RDFLib 7.6.0 namespace.py compute_qname and Turtle get_pname preparation.
//! BSD-3-Clause: docs/RDFLIB-SNAPSHOT-LICENSE.md. This is the reachable source
//! runtime domain (29 defaults plus nsN allocation), not a public bind API.
//! Default-graph and Dataset managers have separate caches/trie objects over
//! one binding store. Loading user prefixes does not bind them in this owner.
use anyhow::{Result, bail, ensure};
use std::collections::BTreeMap;

const XML: &str = "http://www.w3.org/XML/1998/namespace";
pub(super) const DEFAULTS: &[(&str, &str)] = &[
    ("brick", "https://brickschema.org/schema/Brick#"),
    ("csvw", "http://www.w3.org/ns/csvw#"),
    ("dc", "http://purl.org/dc/elements/1.1/"),
    ("dcat", "http://www.w3.org/ns/dcat#"),
    ("dcmitype", "http://purl.org/dc/dcmitype/"),
    ("dcterms", "http://purl.org/dc/terms/"),
    ("dcam", "http://purl.org/dc/dcam/"),
    ("doap", "http://usefulinc.com/ns/doap#"),
    ("foaf", "http://xmlns.com/foaf/0.1/"),
    ("geo", "http://www.opengis.net/ont/geosparql#"),
    ("odrl", "http://www.w3.org/ns/odrl/2/"),
    ("org", "http://www.w3.org/ns/org#"),
    ("prof", "http://www.w3.org/ns/dx/prof/"),
    ("prov", "http://www.w3.org/ns/prov#"),
    ("qb", "http://purl.org/linked-data/cube#"),
    ("schema", "https://schema.org/"),
    ("sh", "http://www.w3.org/ns/shacl#"),
    ("skos", "http://www.w3.org/2004/02/skos/core#"),
    ("sosa", "http://www.w3.org/ns/sosa/"),
    ("ssn", "http://www.w3.org/ns/ssn/"),
    ("time", "http://www.w3.org/2006/time#"),
    ("vann", "http://purl.org/vocab/vann/"),
    ("void", "http://rdfs.org/ns/void#"),
    ("wgs", "https://www.w3.org/2003/01/geo/wgs84_pos#"),
    ("owl", "http://www.w3.org/2002/07/owl#"),
    ("rdf", "http://www.w3.org/1999/02/22-rdf-syntax-ns#"),
    ("rdfs", "http://www.w3.org/2000/01/rdf-schema#"),
    ("xsd", "http://www.w3.org/2001/XMLSchema#"),
    ("xml", XML),
];

#[derive(Debug)]
pub(super) struct OriginalNamespaceError {
    pub class: &'static str,
    pub message: String,
}
impl std::fmt::Display for OriginalNamespaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.class == "KeyError" {
            match semantica_json::python_repr(&serde_json::json!(self.message)) {
                Ok(v) => f.write_str(&v),
                Err(_) => f.write_str(&self.message),
            }
        } else {
            f.write_str(&self.message)
        }
    }
}
impl std::error::Error for OriginalNamespaceError {}
fn original(class: &'static str, message: String) -> anyhow::Error {
    OriginalNamespaceError { class, message }.into()
}

fn split(uri: &str) -> Result<(String, String)> {
    if uri.starts_with(XML) {
        // The original deliberately uses split(XML)[1], not a single strip.
        return Ok((XML.into(), uri.split(XML).nth(1).unwrap().into()));
    }
    for (boundary, c) in uri.char_indices().rev() {
        if semantica_json::python_xml_name_class(c).1 || "··-._%()".contains(c) {
            continue;
        }
        // Python's original loop starts at a negative index. Visit its suffix
        // first, then the nonnegative range; retain its empty-namespace break.
        for (offset, c) in uri[boundary..]
            .char_indices()
            .map(|(i, c)| (boundary + i, c))
            .chain(uri.char_indices())
        {
            if semantica_json::python_xml_name_class(c).0 || c == '_' {
                if offset == 0 {
                    break;
                }
                return Ok((uri[..offset].into(), uri[offset..].into()));
            }
        }
        break;
    }
    Err(original("ValueError", format!("Can't split '{uri}'")))
}

/// Trie nodes are stable arena identities: cached strie references survive
/// insertion of a new parent namespace. Scanning only current bindings would
/// lose unbound split namespaces and the original cross-manager behavior.
#[derive(Clone, Default)]
struct TrieNode {
    value: String,
    children: Vec<usize>,
}
#[derive(Clone)]
struct Manager {
    trie: Vec<TrieNode>,
    strie: BTreeMap<String, usize>,
    cache: BTreeMap<String, [String; 3]>,
}
impl Default for Manager {
    fn default() -> Self {
        let mut value = Self {
            trie: vec![TrieNode::default()],
            strie: BTreeMap::new(),
            cache: BTreeMap::new(),
        };
        for (_, namespace) in DEFAULTS {
            value.insert(namespace);
        }
        value
    }
}
impl Manager {
    fn insert(&mut self, value: &str) -> usize {
        let mut root = 0;
        loop {
            if let Some(i) = self.trie[root]
                .children
                .iter()
                .copied()
                .find(|i| self.trie[*i].value == value)
            {
                return i;
            }
            if let Some(i) = self.trie[root]
                .children
                .iter()
                .copied()
                .find(|i| value.starts_with(&self.trie[*i].value))
            {
                root = i;
                continue;
            }
            let new = self.trie.len();
            self.trie.push(TrieNode {
                value: value.into(),
                children: Vec::new(),
            });
            let moved: Vec<_> = self.trie[root]
                .children
                .iter()
                .copied()
                .filter(|i| self.trie[*i].value.starts_with(value))
                .collect();
            self.trie[root].children.retain(|i| !moved.contains(i));
            self.trie[new].children = moved;
            self.trie[root].children.push(new);
            return new;
        }
    }
    fn longest(&self, mut root: usize, uri: &str) -> Option<&str> {
        let mut found = None;
        while let Some(i) = self.trie[root]
            .children
            .iter()
            .copied()
            .find(|i| uri.starts_with(&self.trie[*i].value))
        {
            root = i;
            found = Some(self.trie[i].value.as_str());
        }
        found
    }
}

#[derive(Clone, Default)]
pub(super) struct NamespaceState {
    bindings: Vec<[String; 2]>,
    managers: [Option<Manager>; 2],
    charged_bytes: usize,
    charged_entries: usize,
}
impl NamespaceState {
    /// The original UPDATE uses a temporary ConjunctiveGraph manager over the
    /// shared bindings, without creating either retained graph manager. Prepare
    /// its namespace arguments without publishing effects before typed input
    /// conversion has reached the backend.
    pub(super) fn temporary_namespaces(
        &self,
        max_bytes: usize,
        max_entries: usize,
    ) -> Result<BTreeMap<String, String>> {
        let (bytes, entries) = self.default_binding_charge();
        self.check_charge(bytes, entries, max_bytes, max_entries)?;
        Ok(DEFAULTS
            .iter()
            .map(|(p, n)| ((*p).to_owned(), (*n).to_owned()))
            .chain(self.bindings.iter().map(|b| (b[0].clone(), b[1].clone())))
            .collect())
    }
    fn default_binding_charge(&self) -> (usize, usize) {
        DEFAULTS
            .iter()
            .filter(|(p, _)| !self.bindings.iter().any(|b| b[0] == *p))
            .fold((0, 0), |(bytes, count), (p, n)| {
                (bytes + p.len() + n.len() + 128, count + 1)
            })
    }
    pub(super) fn initialize_temporary(
        &mut self,
        max_bytes: usize,
        max_entries: usize,
    ) -> Result<()> {
        let (bytes, entries) = self.default_binding_charge();
        self.charge(bytes, entries, max_bytes, max_entries)?;
        for (prefix, namespace) in DEFAULTS {
            if !self.bindings.iter().any(|b| b[0] == *prefix) {
                self.bindings.push([(*prefix).into(), (*namespace).into()]);
            }
        }
        Ok(())
    }
    fn charge(
        &mut self,
        bytes: usize,
        entries: usize,
        max_bytes: usize,
        max_entries: usize,
    ) -> Result<()> {
        let (bytes, entries) = self.check_charge(bytes, entries, max_bytes, max_entries)?;
        self.charged_bytes = bytes;
        self.charged_entries = entries;
        Ok(())
    }
    fn check_charge(
        &self,
        bytes: usize,
        entries: usize,
        max_bytes: usize,
        max_entries: usize,
    ) -> Result<(usize, usize)> {
        let bytes = self
            .charged_bytes
            .checked_add(bytes)
            .filter(|v| *v <= max_bytes);
        let entries = self
            .charged_entries
            .checked_add(entries)
            .filter(|v| *v <= max_entries);
        let (Some(bytes), Some(entries)) = (bytes, entries) else {
            bail!("source_namespace_retained_budget");
        };
        Ok((bytes, entries))
    }
    pub(super) fn initialize(
        &mut self,
        named: bool,
        max_bytes: usize,
        max_entries: usize,
    ) -> Result<()> {
        let index = usize::from(named);
        if self.managers[index].is_none() {
            self.charge(
                DEFAULTS
                    .iter()
                    .map(|(p, n)| p.len() + n.len() * 2 + 160)
                    .sum(),
                DEFAULTS.len(),
                max_bytes,
                max_entries,
            )?;
            for (prefix, namespace) in DEFAULTS {
                if !self.bindings.iter().any(|b| b[0] == *prefix) {
                    self.bindings.push([(*prefix).into(), (*namespace).into()]);
                }
            }
            self.managers[index] = Some(Manager::default());
        }
        Ok(())
    }
    pub(super) fn bindings(&self) -> &[[String; 2]] {
        &self.bindings
    }
    fn prefix(&self, namespace: &str) -> Option<&str> {
        self.bindings
            .iter()
            .find(|b| b[1] == namespace)
            .map(|b| b[0].as_str())
    }
    pub(super) fn compute(
        &mut self,
        named: bool,
        uri: &str,
        generate: bool,
        max_bytes: usize,
        max_entries: usize,
    ) -> Result<[String; 3]> {
        ensure!(uri.len() <= max_bytes, "source_namespace_input_budget");
        self.initialize(named, max_bytes, max_entries)?;
        let index = usize::from(named);
        if let Some(hit) = self.managers[index].as_ref().unwrap().cache.get(uri) {
            return Ok(hit.clone());
        }
        if uri.contains(['<', '>', '"', ' ', '{', '}', '|', '\\', '^', '`']) {
            return Err(original(
                "ValueError",
                format!(
                    "\"{uri}\" does not look like a valid URI, cannot serialize this. Did you want to urlencode it?"
                ),
            ));
        }
        let (mut namespace, mut local) = match split(uri) {
            Ok(v) => v,
            Err(e) => {
                if self.prefix(uri).is_none_or(str::is_empty) {
                    return Err(e);
                }
                (uri.into(), String::new())
            }
        };
        if !self.managers[index]
            .as_ref()
            .unwrap()
            .strie
            .contains_key(&namespace)
        {
            self.charge(namespace.len() * 2 + 160, 1, max_bytes, max_entries)?;
            let manager = self.managers[index].as_mut().unwrap();
            let i = manager.insert(&namespace);
            manager.strie.insert(namespace.clone(), i);
        }
        let manager = self.managers[index].as_ref().unwrap();
        if let Some(longest) = manager.longest(manager.strie[&namespace], uri) {
            namespace = longest.into();
            local = uri[namespace.len()..].into();
        }
        let prefix = if let Some(prefix) = self.prefix(&namespace) {
            prefix.to_owned()
        } else {
            if !generate {
                return Err(original(
                    "KeyError",
                    format!("No known prefix for {namespace} and generate=False"),
                ));
            }
            let mut num = 1usize;
            let prefix = loop {
                let prefix = format!("ns{num}");
                if !self
                    .bindings
                    .iter()
                    .any(|b| b[0] == prefix && !b[1].is_empty())
                {
                    break prefix;
                }
                num += 1;
            };
            self.charge(
                prefix.len() + namespace.len() + 128,
                1,
                max_bytes,
                max_entries,
            )?;
            self.bindings.push([prefix.clone(), namespace.clone()]);
            self.managers[index].as_mut().unwrap().insert(&namespace);
            prefix
        };
        self.charge(
            uri.len() + prefix.len() + namespace.len() + local.len() + 192,
            1,
            max_bytes,
            max_entries,
        )?;
        let value = [prefix, namespace, local];
        self.managers[index]
            .as_mut()
            .unwrap()
            .cache
            .insert(uri.into(), value.clone());
        Ok(value)
    }
}

#[derive(Debug, Default)]
pub(super) struct TurtleNames {
    pub namespaces: BTreeMap<String, String>,
}
impl TurtleNames {
    pub(super) fn pname(
        &mut self,
        state: &mut NamespaceState,
        named: bool,
        uri: &str,
        generate: bool,
        bytes: usize,
        entries: usize,
    ) -> Result<Option<String>> {
        let [prefix, namespace, local] = match state.compute(named, uri, generate, bytes, entries) {
            Ok(v) => v,
            Err(e) if e.is::<OriginalNamespaceError>() => {
                if let Some(pfx) = state.prefix(uri) {
                    [pfx.into(), uri.into(), String::new()]
                } else {
                    return Ok(None);
                }
            }
            Err(e) => return Err(e), // Never swallow a local resource refusal.
        };
        let mut escaped = String::new();
        let raw = local.as_bytes();
        for (i, c) in local.char_indices() {
            if matches!(c, '(' | ')')
                || c == '%'
                    && !raw
                        .get(i + 1..i + 3)
                        .is_some_and(|v| v.iter().all(u8::is_ascii_hexdigit))
            {
                escaped.push('\\');
            }
            escaped.push(c);
        }
        if escaped.ends_with('.') {
            return Ok(None);
        }
        // Reachable original wrapper bindings are fixed defaults or nsN. No
        // user-prefix bind API is silently admitted; _prefix rewriting is pending.
        ensure!(
            !prefix.starts_with('_')
                && self.namespaces.get(&prefix).is_none_or(|v| *v == namespace),
            "source_namespace_external_binding_not_received"
        );
        self.namespaces.insert(prefix.clone(), namespace);
        Ok(Some(format!("{prefix}:{escaped}")))
    }
}

pub(super) fn can_relativize(uri: &str, base: Option<&str>) -> bool {
    base.is_some_and(|base| uri.starts_with(base) && !uri.replace(base, "").contains(['#', '/']))
}

#[cfg(test)]
#[path = "source_namespace_receiving.rs"]
mod receiving;

#[path = "source_namespace_projection.rs"]
mod projection;

#[path = "source_namespace_capture.rs"]
mod capture;

#[cfg(test)]
#[path = "source_namespace_witness_receiving.rs"]
mod witness_receiving;

#[cfg(test)]
#[path = "source_session_lifecycle_receiving.rs"]
mod lifecycle_receiving;
