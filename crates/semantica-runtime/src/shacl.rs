//! Bounded SHACL Core and historical local SPARQL constraints.
//! https://www.w3.org/TR/shacl/ remains the semantics authority; unsupported is an error.
#[path = "shacl/constraints.rs"]
mod constraints;
#[path = "shacl/core.rs"]
mod core;
#[path = "shacl/path.rs"]
mod path;
use anyhow::{Context, Result, bail, ensure};
use oxigraph::model::{Dataset, Term};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const SH: &str = "http://www.w3.org/ns/shacl#";
const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
const SUPPORTED: &[&str] = &[
    "targetClass",
    "targetNode",
    "targetObjectsOf",
    "targetSubjectsOf",
    "property",
    "path",
    "minCount",
    "maxCount",
    "hasValue",
    "minExclusive",
    "minInclusive",
    "maxExclusive",
    "maxInclusive",
    "lessThan",
    "lessThanOrEquals",
    "equals",
    "disjoint",
    "datatype",
    "minLength",
    "maxLength",
    "pattern",
    "flags",
    "languageIn",
    "uniqueLang",
    "nodeKind",
    "in",
    "or",
    "and",
    "xone",
    "not",
    "node",
    "class",
    "closed",
    "ignoredProperties",
    "qualifiedValueShape",
    "qualifiedMinCount",
    "qualifiedMaxCount",
    "qualifiedValueShapesDisjoint",
    "inversePath",
    "alternativePath",
    "zeroOrMorePath",
    "oneOrMorePath",
    "zeroOrOnePath",
    "sparql",
    "select",
    "prefixes",
    "declare",
    "prefix",
    "namespace",
    "message",
    "severity",
    "deactivated",
    "name",
    "description",
    "order",
    "group",
    "defaultValue",
];
pub fn supported() -> Value {
    json!({"profile":"semantica.shacl-core/3","predecessor_profile":"oev2.shacl-v1-corpus/2","predicates":SUPPORTED,"paths":["predicate","inverse","sequence","alternative","zero-or-more","one-or-more","zero-or-one"],"sparql":"local SELECT with this prebinding; local declared prefixes/imports; no custom components/remote datasets","prefix_compatibility":"historical document namespace fallback /1 reported separately when used","comparison":"pinned RDF evaluator; comparison errors are violations, not successful coercion","limits":{"evaluation_visits":100000,"path_steps":100000,"nested_shape_depth":32},"unsupported":"returns error/unknown; never conforms","conformance_claim":"bounded profile, not full SHACL conformance"})
}
#[derive(Debug, Clone, Serialize)]
pub struct Violation {
    pub focus: String,
    pub path: Option<String>,
    pub component: String,
    pub value: Option<String>,
    pub message: String,
    pub source_shape: String,
    pub source_constraint_component: String,
    pub severity: String,
    pub messages: Vec<Value>,
}
#[derive(Default)]
struct G {
    by_subject: BTreeMap<String, Vec<(String, Term)>>,
    by_object: BTreeMap<String, Vec<(String, Term)>>,
    subjects: BTreeMap<String, Term>,
}
impl G {
    fn new(store: &Dataset) -> Result<Self> {
        let mut g = Self::default();
        for q in store.iter() {
            let q = q.into_owned();
            let subject: Term = q.subject.into();
            g.subjects.insert(subject.to_string(), subject.clone());
            g.by_object
                .entry(q.object.to_string())
                .or_default()
                .push((q.predicate.as_str().into(), subject.clone()));
            g.by_subject
                .entry(subject.to_string())
                .or_default()
                .push((q.predicate.as_str().into(), q.object));
        }
        Ok(g)
    }
    fn objects(&self, s: &Term, p: &str) -> Vec<Term> {
        self.by_subject
            .get(&s.to_string())
            .into_iter()
            .flatten()
            .filter(|(pred, _)| pred == p)
            .map(|(_, o)| o.clone())
            .collect()
    }
    fn sh(&self, s: &Term, p: &str) -> Vec<Term> {
        self.objects(s, &format!("{SH}{p}"))
    }
    fn subjects(&self, object: &Term, predicate: &str) -> Vec<Term> {
        self.by_object
            .get(&object.to_string())
            .into_iter()
            .flatten()
            .filter(|(p, _)| p == predicate)
            .map(|(_, subject)| subject.clone())
            .collect()
    }
    fn one(&self, s: &Term, p: &str) -> Result<Option<Term>> {
        let values = self.sh(s, p);
        ensure!(values.len() <= 1, "shacl_parameter_cardinality:{p}");
        Ok(values.into_iter().next())
    }
    fn list(&self, head: &Term) -> Result<Vec<Term>> {
        let mut current = head.clone();
        let mut result = Vec::new();
        let mut visited = BTreeSet::new();
        while current.to_string() != format!("<{RDF}nil>") {
            ensure!(
                visited.insert(current.to_string()) && result.len() < 1000,
                "shacl_list_cycle_or_budget"
            );
            let first = self.objects(&current, &format!("{RDF}first"));
            let rest = self.objects(&current, &format!("{RDF}rest"));
            ensure!(first.len() == 1 && rest.len() == 1, "shacl_list_malformed");
            result.push(first[0].clone());
            current = rest[0].clone();
        }
        Ok(result)
    }
    fn instance(&self, value: &Term, class: &Term) -> bool {
        let mut pending = self.objects(value, &format!("{RDF}type"));
        let mut seen = BTreeSet::new();
        while let Some(c) = pending.pop() {
            if &c == class {
                return true;
            }
            if seen.insert(c.to_string()) {
                pending.extend(self.objects(&c, &format!("{RDFS}subClassOf")));
            }
        }
        false
    }
}
fn lexical(t: &Term) -> Result<&str> {
    match t {
        Term::Literal(l) => Ok(l.value()),
        _ => bail!("shacl_parameter_must_be_literal"),
    }
}
fn count(t: &Term) -> Result<usize> {
    lexical(t)?.parse().context("shacl_count_invalid")
}
fn datatype_valid(t: &Term, expected: &str) -> bool {
    let Term::Literal(l) = t else {
        return false;
    };
    if l.datatype().as_str() != expected {
        return false;
    }
    let v = l.value();
    match expected.strip_prefix(XSD).unwrap_or("") {
        "boolean" => ["true", "false", "1", "0"].contains(&v),
        "integer" => {
            let s = v.strip_prefix(['+', '-']).unwrap_or(v);
            !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
        }
        "int" => v.parse::<i32>().is_ok(),
        "long" => v.parse::<i64>().is_ok(),
        "short" => v.parse::<i16>().is_ok(),
        "byte" => v.parse::<i8>().is_ok(),
        "nonNegativeInteger" | "positiveInteger" => {
            let s = v.strip_prefix('+').unwrap_or(v);
            !s.is_empty()
                && s.bytes().all(|b| b.is_ascii_digit())
                && (expected.ends_with("nonNegativeInteger") || s.bytes().any(|b| b != b'0'))
        }
        "unsignedInt" => v.parse::<u32>().is_ok(),
        "unsignedLong" => v.parse::<u64>().is_ok(),
        "decimal" => regex::Regex::new(r"^[+-]?(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)$")
            .expect("literal regex")
            .is_match(v),
        "double" | "float" => {
            // XSD 1.1 doubleRep/floatRep, not Rust's wider special spellings.
            // This is lexical validation: a valid large exponent may overflow.
            static FLOAT_LEXICAL: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(
                || {
                    regex::Regex::new(
                        r"\A(?:[+-]?(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[Ee][+-]?[0-9]+)?|[+-]?INF|NaN)\z",
                    )
                    .expect("XSD floating lexical grammar")
                },
            );
            FLOAT_LEXICAL.is_match(v)
        }
        "string" => true,
        "date" => v.parse::<oxsdatatypes::Date>().is_ok(),
        "dateTime" => v.parse::<oxsdatatypes::DateTime>().is_ok(),
        _ => true, // Other datatypes are rejected in the preflight below.
    }
}
fn preflight(shapes: &G) -> Result<()> {
    core::parameters(shapes)?;
    for statements in shapes.by_subject.values() {
        for (p, o) in statements {
            if let Some(local) = p.strip_prefix(SH) {
                ensure!(
                    SUPPORTED.contains(&local),
                    "shacl_unsupported_predicate:{local}"
                );
            }
            if p == &format!("{SH}path") {
                path::Path::parse(shapes, o)?;
            }
            if ["lessThan", "lessThanOrEquals"]
                .iter()
                .any(|k| p == &format!("{SH}{k}"))
            {
                ensure!(
                    matches!(o, Term::NamedNode(_)),
                    "shacl_pair_parameter_must_be_iri"
                );
                ensure!(
                    statements
                        .iter()
                        .any(|(pred, value)| pred == &format!("{SH}path")
                            && matches!(value, Term::NamedNode(_) | Term::BlankNode(_))),
                    "shacl_pair_requires_property_shape"
                );
            }
            if [
                "minExclusive",
                "minInclusive",
                "maxExclusive",
                "maxInclusive",
            ]
            .iter()
            .any(|k| p == &format!("{SH}{k}"))
            {
                ensure!(
                    matches!(o, Term::Literal(_)),
                    "shacl_range_parameter_must_be_literal"
                );
            }
            if p == &format!("{SH}datatype") {
                let Term::NamedNode(n) = o else {
                    bail!("shacl_datatype_invalid");
                };
                let local = n
                    .as_str()
                    .strip_prefix(XSD)
                    .context("shacl_datatype_unsupported")?;
                ensure!(
                    [
                        "string",
                        "boolean",
                        "integer",
                        "int",
                        "long",
                        "short",
                        "byte",
                        "nonNegativeInteger",
                        "positiveInteger",
                        "unsignedInt",
                        "unsignedLong",
                        "decimal",
                        "double",
                        "float",
                        "date",
                        "dateTime"
                    ]
                    .contains(&local),
                    "shacl_datatype_unsupported:{local}"
                );
            }
        }
    }
    Ok(())
}
fn sparql_prefixes(
    shapes: &G,
    constraint: &Term,
    namespaces: &BTreeMap<String, String>,
    legacy: &mut BTreeSet<String>,
) -> Result<String> {
    let mut pending = shapes.sh(constraint, "prefixes");
    let mut visited = BTreeSet::new();
    let mut mappings = BTreeMap::<String, String>::new();
    while let Some(owner) = pending.pop() {
        if !visited.insert(owner.to_string()) {
            continue;
        }
        ensure!(visited.len() <= 128, "shacl_prefix_import_budget");
        let declarations = shapes.sh(&owner, "declare");
        if declarations.is_empty() {
            // The frozen chapter used a bare namespace as sh:prefixes. This
            // compatibility path is explicit; it does not fetch an ontology or
            // pretend the document had a W3C sh:declare statement.
            if let Term::NamedNode(uri) = &owner {
                for (prefix, namespace) in namespaces {
                    if namespace == uri.as_str() {
                        legacy.insert(prefix.clone());
                        if let Some(old) = mappings.insert(prefix.clone(), namespace.clone()) {
                            ensure!(old == *namespace, "shacl_prefix_conflict");
                        }
                    }
                }
            }
        }
        for declaration in declarations {
            let prefix = shapes
                .one(&declaration, "prefix")?
                .context("shacl_prefix_missing")?;
            let namespace = shapes
                .one(&declaration, "namespace")?
                .context("shacl_namespace_missing")?;
            ensure!(
                datatype_valid(&prefix, &format!("{XSD}string")),
                "shacl_prefix_datatype_invalid"
            );
            let Term::Literal(uri) = namespace else {
                bail!("shacl_namespace_invalid")
            };
            ensure!(
                uri.datatype().as_str() == format!("{XSD}anyURI") && uri.language().is_none(),
                "shacl_namespace_datatype_invalid"
            );
            let prefix = lexical(&prefix)?.to_owned();
            ensure!(
                prefix.is_empty()
                    || regex::Regex::new(r"^[A-Za-z_][A-Za-z0-9_-]*$")?.is_match(&prefix),
                "shacl_prefix_syntax_unsupported"
            );
            oxigraph::model::NamedNode::new(uri.value())?;
            if let Some(old) = mappings.insert(prefix, uri.value().into()) {
                ensure!(old == uri.value(), "shacl_prefix_conflict");
            }
        }
        pending.extend(shapes.objects(&owner, "http://www.w3.org/2002/07/owl#imports"));
    }
    Ok(mappings
        .into_iter()
        .map(|(p, n)| format!("PREFIX {p}: <{n}>\n"))
        .collect())
}
#[derive(Default)]
struct EvaluationBudget {
    visits: usize,
    path_steps: usize,
}
fn component_iri(component: &str) -> String {
    if component == "sparql" {
        return format!("{SH}SPARQLConstraintComponent");
    }
    let mut name = component.to_owned();
    if let Some(first) = name.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    format!("{SH}{name}ConstraintComponent")
}
fn messages(shapes: &G, shape: &Term) -> Vec<Term> {
    let mut values = shapes.sh(shape, "message");
    values.sort_by_key(ToString::to_string);
    values
}
fn deactivated(shapes: &G, subject: &Term) -> Result<bool> {
    let Some(value) = shapes.one(subject, "deactivated")? else {
        return Ok(false);
    };
    ensure!(
        datatype_valid(&value, &format!("{XSD}boolean")),
        "shacl_deactivated_boolean_invalid"
    );
    Ok(["true", "1"].contains(&lexical(&value)?))
}
fn evaluate(
    shape: &Term,
    focus: &Term,
    shapes: &G,
    data: &G,
    store: &Dataset,
    depth: usize,
    namespaces: &BTreeMap<String, String>,
    legacy_prefixes: &mut BTreeSet<String>,
    budget: &mut EvaluationBudget,
) -> Result<Vec<Violation>> {
    ensure!(depth < 32, "shacl_recursion_unsupported");
    budget.visits += 1;
    ensure!(budget.visits <= 100_000, "shacl_evaluation_visit_budget");
    if deactivated(shapes, shape)? {
        return Ok(Vec::new());
    }
    let path = shapes.one(shape, "path")?;
    let values = if let Some(path) = &path {
        path::Path::parse(shapes, path)?.values(data, focus, &mut budget.path_steps)?
    } else {
        vec![focus.clone()]
    };
    let mut violations = Vec::new();
    let declared_messages = messages(shapes, shape);
    let declared_message = declared_messages
        .first()
        .map(|t| lexical(t).map(str::to_owned))
        .transpose()?;
    let severity = shapes
        .one(shape, "severity")?
        .map(|t| match t {
            Term::NamedNode(n) => Ok(n.as_str().to_owned()),
            _ => bail!("shacl_severity_invalid"),
        })
        .transpose()?
        .unwrap_or_else(|| format!("{SH}Violation"));
    let mut emit = |component: &str, value: Option<&Term>, message: &str| {
        violations.push(Violation {
            focus: focus.to_string(),
            path: path.as_ref().map(ToString::to_string),
            component: component.into(),
            value: value.map(ToString::to_string),
            message: declared_message.as_deref().unwrap_or(message).into(),
            source_shape: shape.to_string(),
            source_constraint_component: component_iri(component),
            severity: severity.clone(),
            messages: declared_messages
                .iter()
                .map(crate::graph::term_value)
                .collect(),
        });
    };
    for p in ["minCount", "maxCount"] {
        if let Some(v) = shapes.one(shape, p)? {
            let n = count(&v)?;
            if (p == "minCount" && values.len() < n) || (p == "maxCount" && values.len() > n) {
                emit(p, None, "cardinality outside declared bound");
            }
        }
    }
    for v in shapes.sh(shape, "hasValue") {
        if !values.contains(&v) {
            emit("hasValue", None, "required value absent");
        }
    }
    for (parameter, operator) in [
        ("minExclusive", ">"),
        ("minInclusive", ">="),
        ("maxExclusive", "<"),
        ("maxInclusive", "<="),
    ] {
        if let Some(bound) = shapes.one(shape, parameter)? {
            for value in &values {
                if crate::graph::compare_terms(value, &bound, operator)? != Some(true) {
                    emit(
                        parameter,
                        Some(value),
                        "value violates range or cannot be compared",
                    );
                }
            }
        }
    }
    for (parameter, operator) in [("lessThan", "<"), ("lessThanOrEquals", "<=")] {
        for predicate in shapes.sh(shape, parameter) {
            let Term::NamedNode(predicate) = predicate else {
                bail!("shacl_pair_parameter_must_be_iri");
            };
            let others = data.objects(focus, predicate.as_str());
            ensure!(
                values
                    .len()
                    .checked_mul(others.len())
                    .is_some_and(|n| n <= 100_000),
                "shacl_pair_comparison_budget"
            );
            for value in &values {
                for other in &others {
                    if crate::graph::compare_terms(value, other, operator)? != Some(true) {
                        emit(
                            parameter,
                            Some(value),
                            "property pair ordering fails or values are incomparable",
                        );
                    }
                }
            }
        }
    }
    if let Some(Term::NamedNode(dt)) = shapes.one(shape, "datatype")? {
        for v in &values {
            if !datatype_valid(v, dt.as_str()) {
                emit("datatype", Some(v), "RDF datatype or lexical form differs");
            }
        }
    }
    if let Some(kind) = shapes.one(shape, "nodeKind")? {
        let kind = kind.to_string();
        for v in &values {
            let okay = match kind.as_str() {
                s if s == format!("<{SH}IRI>") => matches!(v, Term::NamedNode(_)),
                s if s == format!("<{SH}Literal>") => matches!(v, Term::Literal(_)),
                s if s == format!("<{SH}BlankNode>") => matches!(v, Term::BlankNode(_)),
                s if s == format!("<{SH}BlankNodeOrIRI>") => {
                    matches!(v, Term::BlankNode(_) | Term::NamedNode(_))
                }
                s if s == format!("<{SH}BlankNodeOrLiteral>") => {
                    matches!(v, Term::BlankNode(_) | Term::Literal(_))
                }
                s if s == format!("<{SH}IRIOrLiteral>") => {
                    matches!(v, Term::NamedNode(_) | Term::Literal(_))
                }
                _ => bail!("shacl_nodekind_unsupported"),
            };
            if !okay {
                emit("nodeKind", Some(v), "RDF node kind differs");
            }
        }
    }
    for p in ["minLength", "maxLength"] {
        if let Some(v) = shapes.one(shape, p)? {
            let n = count(&v)?;
            for val in &values {
                let text = match val {
                    Term::Literal(l) => Some(l.value()),
                    Term::NamedNode(n) => Some(n.as_str()),
                    _ => None,
                };
                if text.is_none_or(|s| {
                    if p == "minLength" {
                        s.chars().count() < n
                    } else {
                        s.chars().count() > n
                    }
                }) {
                    emit(p, Some(val), "string length outside bound");
                }
            }
        }
    }
    if let Some(pattern) = shapes.one(shape, "pattern")? {
        let pattern = lexical(&pattern)?;
        let flags = shapes
            .one(shape, "flags")?
            .map(|t| lexical(&t).map(str::to_owned))
            .transpose()?
            .unwrap_or_default();
        ensure!(
            flags.chars().all(|c| "imsx".contains(c)),
            "shacl_regex_flags_unsupported"
        );
        let p = if flags.is_empty() {
            pattern.to_owned()
        } else {
            format!("(?{flags}){pattern}")
        };
        let re = regex::Regex::new(&p).context("shacl_regex_unsupported")?;
        for v in &values {
            let text = match v {
                Term::Literal(l) => Some(l.value()),
                Term::NamedNode(n) => Some(n.as_str()),
                _ => None,
            };
            if text.is_none_or(|t| !re.is_match(t)) {
                emit("pattern", Some(v), "string does not satisfy pattern");
            }
        }
    }
    if let Some(head) = shapes.one(shape, "in")? {
        let choices = shapes.list(&head)?;
        for v in &values {
            if !choices.contains(v) {
                emit("in", Some(v), "value outside declared RDF list");
            }
        }
    }
    for class in shapes.sh(shape, "class") {
        for v in &values {
            if !data.instance(v, &class) {
                emit("class", Some(v), "value lacks required RDF class");
            }
        }
    }
    for p in ["or", "and", "xone"] {
        for head in shapes.sh(shape, p) {
            let choices = shapes.list(&head)?;
            for v in &values {
                let mut matches = 0;
                for choice in &choices {
                    if evaluate(
                        choice,
                        v,
                        shapes,
                        data,
                        store,
                        depth + 1,
                        namespaces,
                        legacy_prefixes,
                        budget,
                    )?
                    .is_empty()
                    {
                        matches += 1;
                    }
                }
                if (p == "or" && matches == 0)
                    || (p == "and" && matches != choices.len())
                    || (p == "xone" && matches != 1)
                {
                    emit(p, Some(v), "logical shape constraint failed");
                }
            }
        }
    }
    for p in ["node", "not"] {
        for other in shapes.sh(shape, p) {
            for v in &values {
                let valid = evaluate(
                    &other,
                    v,
                    shapes,
                    data,
                    store,
                    depth + 1,
                    namespaces,
                    legacy_prefixes,
                    budget,
                )?
                .is_empty();
                if (p == "node" && !valid) || (p == "not" && valid) {
                    emit(p, Some(v), "nested shape constraint failed");
                }
            }
        }
    }
    let extra =
        constraints::evaluate(shape, focus, &values, shapes, data, &mut |nested, value| {
            Ok(evaluate(
                nested,
                value,
                shapes,
                data,
                store,
                depth + 1,
                namespaces,
                legacy_prefixes,
                budget,
            )?
            .is_empty())
        })?;
    for finding in &extra {
        emit(finding.component, finding.value.as_ref(), finding.message);
    }
    drop(emit);
    let extra_start = violations.len() - extra.len();
    for (violation, finding) in violations[extra_start..].iter_mut().zip(extra) {
        if let Some(path) = finding.path {
            violation.path = Some(path.to_string());
        }
    }
    // End closure borrow before appending nested property findings.
    for property in shapes.sh(shape, "property") {
        for value in &values {
            violations.extend(evaluate(
                &property,
                value,
                shapes,
                data,
                store,
                depth + 1,
                namespaces,
                legacy_prefixes,
                budget,
            )?);
        }
    }
    for constraint in shapes.sh(shape, "sparql") {
        let select = shapes
            .one(&constraint, "select")?
            .context("shacl_sparql_select_missing")?;
        let select = lexical(&select)?;
        // SHACL 2017 §5.3: activation belongs to each constraint as well as
        // its parent shape. Preserve parameter preflight and the existing
        // select presence/cardinality/literal checks before skipping execution.
        if deactivated(shapes, &constraint)? {
            continue;
        }
        let raw = format!(
            "{}{}",
            sparql_prefixes(shapes, &constraint, namespaces, legacy_prefixes)?,
            select.replace("$this", "?this")
        );
        crate::graph::local_query(&raw)?;
        let constraint_messages = messages(shapes, &constraint);
        let effective_messages = if constraint_messages.is_empty() {
            &declared_messages
        } else {
            &constraint_messages
        };
        let message = effective_messages
            .first()
            .map(|t| lexical(t).map(str::to_owned))
            .transpose()?
            .unwrap_or_else(|| "SPARQL constraint failed".into());
        for row in crate::graph::select_bound(store, &raw, focus.clone())? {
            violations.push(Violation {
                focus: focus.to_string(),
                path: row
                    .get("path")
                    .map(ToString::to_string)
                    .or_else(|| path.as_ref().map(ToString::to_string)),
                component: "sparql".into(),
                value: row.get("value").map(ToString::to_string),
                message: message.clone(),
                source_shape: shape.to_string(),
                source_constraint_component: component_iri("sparql"),
                severity: severity.clone(),
                messages: effective_messages
                    .iter()
                    .map(crate::graph::term_value)
                    .collect(),
            });
        }
    }
    Ok(violations)
}
pub fn validate(data: &[u8], shapes: &[u8]) -> Result<Value> {
    let store = crate::graph::load(data)?;
    validate_dataset(&store, shapes)
}

/// The same bounded SHACL algorithm on an already loaded data session.
/// G's adjacency representation is a graph, not a dataset union policy.
/// Refuse named graphs before target selection instead of silently flattening.
pub fn validate_dataset(store: &Dataset, shapes: &[u8]) -> Result<Value> {
    ensure!(store.len() <= 100_000, "rdf_quad_budget");
    ensure!(
        store.iter().all(|q| q.graph_name.is_default_graph()),
        "shacl_named_graph_view_unsupported"
    );
    let shape_store = crate::graph::load(shapes)?;
    let namespaces = crate::graph::document_prefixes(shapes)?;
    let mut legacy_prefixes = BTreeSet::new();
    let shapes = G::new(&shape_store)?;
    preflight(&shapes)?;
    let graph = G::new(&store)?;
    let mut targets: BTreeMap<String, (Term, BTreeMap<String, Term>)> = BTreeMap::new();
    let node_shape = Term::NamedNode(oxigraph::model::NamedNode::new(format!("{SH}NodeShape"))?);
    let property_shape = Term::NamedNode(oxigraph::model::NamedNode::new(format!(
        "{SH}PropertyShape"
    ))?);
    let rdfs_class = Term::NamedNode(oxigraph::model::NamedNode::new(format!("{RDFS}Class"))?);
    for shape in shapes.subjects.values() {
        let is_shape = shapes.instance(shape, &node_shape)
            || shapes.instance(shape, &property_shape)
            || shapes
                .by_subject
                .get(&shape.to_string())
                .into_iter()
                .flatten()
                .any(|(p, _)| {
                    p.strip_prefix(SH).is_some_and(|local| {
                        matches!(
                            local,
                            "targetClass"
                                | "targetNode"
                                | "targetObjectsOf"
                                | "targetSubjectsOf"
                                | "property"
                                | "class"
                                | "datatype"
                                | "nodeKind"
                                | "minCount"
                                | "maxCount"
                                | "minExclusive"
                                | "minInclusive"
                                | "maxExclusive"
                                | "maxInclusive"
                                | "minLength"
                                | "maxLength"
                                | "pattern"
                                | "languageIn"
                                | "uniqueLang"
                                | "equals"
                                | "disjoint"
                                | "lessThan"
                                | "lessThanOrEquals"
                                | "and"
                                | "or"
                                | "xone"
                                | "not"
                                | "node"
                                | "qualifiedValueShape"
                                | "closed"
                                | "hasValue"
                                | "in"
                                | "sparql"
                        )
                    })
                })
            || shapes.by_subject.values().flatten().any(|(p, value)| {
                matches!(
                    p.strip_prefix(SH),
                    Some("property" | "node" | "not" | "qualifiedValueShape")
                ) && value == shape
            });
        if is_shape && shapes.instance(shape, &rdfs_class) {
            for focus in graph.subjects.values() {
                if graph.instance(focus, shape) {
                    targets
                        .entry(shape.to_string())
                        .or_insert_with(|| (shape.clone(), BTreeMap::new()))
                        .1
                        .insert(focus.to_string(), focus.clone());
                }
            }
        }
    }
    for q in shape_store.iter() {
        let q = q.into_owned();
        let s: Term = q.subject.into();
        let p = q.predicate.as_str();
        if p == format!("{SH}targetNode") {
            targets
                .entry(s.to_string())
                .or_insert_with(|| (s.clone(), BTreeMap::new()))
                .1
                .insert(q.object.to_string(), q.object);
        } else if p == format!("{SH}targetClass") {
            for term in graph.subjects.values() {
                if graph.instance(term, &q.object) {
                    targets
                        .entry(s.to_string())
                        .or_insert_with(|| (s.clone(), BTreeMap::new()))
                        .1
                        .insert(term.to_string(), term.clone());
                }
            }
        } else if p == format!("{SH}targetObjectsOf") || p == format!("{SH}targetSubjectsOf") {
            let Term::NamedNode(pred) = q.object else {
                bail!("shacl_target_predicate_invalid");
            };
            for triple in store.iter() {
                let triple = triple.into_owned();
                if triple.predicate == pred {
                    let focus = if p.ends_with("ObjectsOf") {
                        triple.object
                    } else {
                        triple.subject.into()
                    };
                    targets
                        .entry(s.to_string())
                        .or_insert_with(|| (s.clone(), BTreeMap::new()))
                        .1
                        .insert(focus.to_string(), focus);
                }
            }
        }
    }
    let mut findings = Vec::new();
    let mut focus_count = 0;
    let mut focus_nodes = BTreeMap::new();
    let mut budget = EvaluationBudget::default();
    for (shape, focuses) in targets.values() {
        for focus in focuses.values() {
            focus_count += 1;
            focus_nodes.insert(focus.to_string(), crate::graph::term_value(focus));
            findings.extend(evaluate(
                shape,
                focus,
                &shapes,
                &graph,
                &store,
                0,
                &namespaces,
                &mut legacy_prefixes,
                &mut budget,
            )?);
        }
    }
    Ok(
        json!({"conforms":findings.is_empty(),"violation_count":findings.len(),"findings":findings,"focus_count":focus_count,"focus_nodes":focus_nodes.into_values().collect::<Vec<_>>(),"profile":supported(),"legacy_document_prefix_fallbacks":legacy_prefixes,"scope":"shape targets only; empty focus does not establish engineering evidence"}),
    )
}
