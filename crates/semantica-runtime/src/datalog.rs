//! Positive, function-free Datalog with bounded semi-naive fixed-point evaluation.
//!
//! This is the single generic algorithm owner, independent of engineering
//! Projects, adoption decisions, providers, and credentials. Its syntax nodes
//! are internal library types, not a second generated transport contract.
//! Locked source/test identities and pre-implementation expectations live in
//! private var/maintenance/datalog-parity/20261002-01.
//!
//! Facts have set semantics. Results are sorted rather than inheriting Python
//! set iteration order. All head variables must occur in the body, and the
//! complete rule body must parse: ignored trailing tokens and unsafe rules are
//! explicit errors. Inference is transactional; budget failure never commits a
//! partial closure or marks it as a fixed point.
use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub type Bindings = BTreeMap<String, String>;
type FactIndex = BTreeMap<String, BTreeSet<DatalogFact>>;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct DatalogFact {
    pub predicate: String,
    pub args: Vec<String>,
}
impl fmt::Display for DatalogFact {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({})", self.predicate, self.args.join(", "))
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BodyAtom {
    pub predicate: String,
    pub args: Vec<String>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DatalogRule {
    pub head_predicate: String,
    pub head_args: Vec<String>,
    pub body: Vec<BodyAtom>,
}

/// Bounds are hard execution conditions, not acceptance scores. max_fact_bytes
/// bounds logical term payload; it is not a measurement of allocator/RSS bytes.
#[derive(Clone, Debug)]
pub struct DatalogLimits {
    pub max_input_bytes: usize,
    pub max_term_bytes: usize,
    pub max_arity: usize,
    pub max_body_atoms: usize,
    pub max_rules: usize,
    pub max_program_bytes: usize,
    pub max_facts: usize,
    pub max_fact_bytes: usize,
    pub max_iterations: usize,
    pub max_match_attempts: usize,
    pub max_bindings: usize,
    pub max_query_rows: usize,
    pub max_context_entries: usize,
}
impl Default for DatalogLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: 64 * 1024,
            max_term_bytes: 4096,
            max_arity: 64,
            max_body_atoms: 64,
            max_rules: 4096,
            max_program_bytes: 8 * 1024 * 1024,
            max_facts: 100_000,
            max_fact_bytes: 32 * 1024 * 1024,
            max_iterations: 1024,
            max_match_attempts: 10_000_000,
            max_bindings: 100_000,
            max_query_rows: 10_000,
            max_context_entries: 100_000,
        }
    }
}
impl DatalogLimits {
    fn validate(&self) -> Result<()> {
        ensure!(
            [
                self.max_input_bytes,
                self.max_term_bytes,
                self.max_arity,
                self.max_body_atoms,
                self.max_rules,
                self.max_program_bytes,
                self.max_facts,
                self.max_fact_bytes,
                self.max_iterations,
                self.max_match_attempts,
                self.max_bindings,
                self.max_query_rows,
                self.max_context_entries,
            ]
            .iter()
            .all(|v| *v > 0),
            "datalog_limits_invalid"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DatalogStats {
    pub iterations: usize,
    pub derived_facts: usize,
    pub match_attempts: usize,
}
pub enum FactInput<'a> {
    Text(&'a str),
    Dictionary(&'a Value),
    Ground(&'a DatalogFact),
}
impl<'a> From<&'a str> for FactInput<'a> {
    fn from(v: &'a str) -> Self {
        Self::Text(v)
    }
}
impl<'a> From<&'a String> for FactInput<'a> {
    fn from(v: &'a String) -> Self {
        Self::Text(v)
    }
}
impl<'a> From<&'a Value> for FactInput<'a> {
    fn from(v: &'a Value) -> Self {
        Self::Dictionary(v)
    }
}
impl<'a> From<&'a DatalogFact> for FactInput<'a> {
    fn from(v: &'a DatalogFact) -> Self {
        Self::Ground(v)
    }
}

/// A ContextGraph exposes the same nodes and edges to the algorithm. The
/// application adapter supplies them; no shadow graph/project state is created.
pub trait ContextGraphSource {
    fn find_nodes(&self) -> &[Value];
    fn find_edges(&self) -> &[Value];
}
#[derive(Clone, Debug, Default)]
pub struct ContextGraphData {
    pub nodes: Vec<Value>,
    pub edges: Vec<Value>,
}
impl ContextGraphSource for ContextGraphData {
    fn find_nodes(&self) -> &[Value] {
        &self.nodes
    }
    fn find_edges(&self) -> &[Value] {
        &self.edges
    }
}

#[derive(Clone, Debug, Default)]
pub struct DatalogReasoner {
    limits: DatalogLimits,
    all_facts: BTreeSet<DatalogFact>,
    fact_index: FactIndex,
    rules: Vec<DatalogRule>,
    derived: bool,
    fact_bytes: usize,
    program_bytes: usize,
    last_stats: Option<DatalogStats>,
}
impl DatalogReasoner {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_limits(limits: DatalogLimits) -> Result<Self> {
        limits.validate()?;
        Ok(Self {
            limits,
            ..Self::default()
        })
    }
    pub fn limits(&self) -> &DatalogLimits {
        &self.limits
    }
    pub fn all_facts(&self) -> &BTreeSet<DatalogFact> {
        &self.all_facts
    }
    pub fn rules(&self) -> &[DatalogRule] {
        &self.rules
    }
    pub fn is_derived(&self) -> bool {
        self.derived
    }
    pub fn indexed_predicates(&self) -> usize {
        self.fact_index.len()
    }
    pub fn last_stats(&self) -> Option<&DatalogStats> {
        self.last_stats.as_ref()
    }
    pub fn clear(&mut self) {
        self.all_facts.clear();
        self.fact_index.clear();
        self.rules.clear();
        self.derived = false;
        self.fact_bytes = 0;
        self.program_bytes = 0;
        self.last_stats = None;
    }

    /// Unknown dictionary formats are skipped, matching the old API. All
    /// recognized inputs pass the same ground-fact validation before insertion.
    pub fn add_fact<'a>(&mut self, input: impl Into<FactInput<'a>>) -> Result<bool> {
        let fact = match input.into() {
            FactInput::Text(text) => {
                let atom = parse_atom(text, true, &self.limits)?;
                DatalogFact {
                    predicate: atom.predicate,
                    args: atom.args,
                }
            }
            FactInput::Dictionary(value) => match dict_fact(value, &self.limits)? {
                Some(fact) => fact,
                None => return Ok(false),
            },
            FactInput::Ground(fact) => {
                validate_fact(fact, &self.limits)?;
                fact.clone()
            }
        };
        validate_fact(&fact, &self.limits)?;
        if self.all_facts.contains(&fact) {
            return Ok(false);
        }
        ensure!(
            self.all_facts.len() < self.limits.max_facts,
            "datalog_fact_budget"
        );
        let size = fact_size(&fact)?;
        let next = self
            .fact_bytes
            .checked_add(size)
            .context("datalog_fact_byte_budget")?;
        ensure!(
            next <= self.limits.max_fact_bytes,
            "datalog_fact_byte_budget"
        );
        self.fact_index
            .entry(fact.predicate.clone())
            .or_default()
            .insert(fact.clone());
        self.all_facts.insert(fact);
        self.fact_bytes = next;
        self.derived = false;
        self.last_stats = None;
        Ok(true)
    }
    pub fn add_rule(&mut self, text: &str) -> Result<()> {
        ensure!(
            self.rules.len() < self.limits.max_rules,
            "datalog_rule_budget"
        );
        let next = self
            .program_bytes
            .checked_add(text.len())
            .context("datalog_program_byte_budget")?;
        ensure!(
            next <= self.limits.max_program_bytes,
            "datalog_program_byte_budget"
        );
        let rule = parse_rule(text, &self.limits)?;
        self.rules.push(rule);
        self.program_bytes = next;
        self.derived = false;
        self.last_stats = None;
        Ok(())
    }

    /// Derive the complete finite closure or return an explicit error. Work
    /// state and deltas are local until a fixed point has actually been reached.
    pub fn derive_all(&mut self) -> Result<Vec<String>> {
        if self.derived {
            return Ok(self.fact_strings());
        }
        self.last_stats = None;
        let mut all = self.all_facts.clone();
        let mut index = self.fact_index.clone();
        let mut delta = all.clone();
        let mut bytes = self.fact_bytes;
        let mut stats = DatalogStats::default();
        while !delta.is_empty() {
            ensure!(
                stats.iterations < self.limits.max_iterations,
                "datalog_iteration_budget"
            );
            stats.iterations += 1;
            let delta_index = index_facts(&delta);
            let mut next = BTreeSet::new();
            for rule in &self.rules {
                for fact in apply_rule(rule, &index, &delta_index, &self.limits, &mut stats)? {
                    if !all.contains(&fact) && !next.contains(&fact) {
                        ensure!(
                            all.len() + next.len() < self.limits.max_facts,
                            "datalog_fact_budget"
                        );
                        bytes = bytes
                            .checked_add(fact_size(&fact)?)
                            .context("datalog_fact_byte_budget")?;
                        ensure!(
                            bytes <= self.limits.max_fact_bytes,
                            "datalog_fact_byte_budget"
                        );
                        next.insert(fact);
                    }
                }
            }
            stats.derived_facts += next.len();
            for fact in &next {
                index
                    .entry(fact.predicate.clone())
                    .or_default()
                    .insert(fact.clone());
                all.insert(fact.clone());
            }
            delta = next;
        }
        self.all_facts = all;
        self.fact_index = index;
        self.fact_bytes = bytes;
        self.derived = true;
        self.last_stats = Some(stats);
        Ok(self.fact_strings())
    }
    fn fact_strings(&self) -> Vec<String> {
        self.all_facts.iter().map(ToString::to_string).collect()
    }

    /// Project variable bindings, preserving the caller's ?y/?Y key.
    /// As in the old API, a fully ground query has an empty projection and
    /// returns []; it is not an ASK interface.
    pub fn query(&mut self, pattern: &str, bindings: Option<&Bindings>) -> Result<Vec<Bindings>> {
        let atom = parse_atom(pattern, true, &self.limits)?;
        let mut query_vars = Vec::new();
        let mut args = Vec::new();
        for arg in atom.args {
            if let Some(name) = arg.strip_prefix('?') {
                ensure!(!name.is_empty(), "datalog_empty_query_variable");
                let internal = internal_variable(name);
                query_vars.push((name.to_string(), internal.clone()));
                args.push(internal);
            } else {
                if is_variable(&arg) {
                    query_vars.push((arg.clone(), arg.clone()));
                }
                args.push(arg);
            }
        }
        let mut initial = Bindings::new();
        if let Some(bindings) = bindings {
            ensure!(
                bindings.len() <= self.limits.max_arity,
                "datalog_binding_budget"
            );
            for (key, value) in bindings {
                ensure!(
                    key.len() <= self.limits.max_term_bytes
                        && value.len() <= self.limits.max_term_bytes,
                    "datalog_term_byte_budget"
                );
                initial.insert(internal_variable(key), value.clone());
            }
        }
        if !self.rules.is_empty() && !self.derived {
            self.derive_all()?;
        }
        let mut rows = BTreeSet::new();
        let mut stats = DatalogStats::default();
        if let Some(facts) = self.fact_index.get(&atom.predicate) {
            for fact in facts {
                count_match(&mut stats, &self.limits)?;
                if let Some(found) = unify(&args, &fact.args, &initial) {
                    let row: Bindings = query_vars
                        .iter()
                        .filter_map(|(original, internal)| {
                            found
                                .get(internal)
                                .map(|value| (original.clone(), value.clone()))
                        })
                        .collect();
                    if !row.is_empty() && !rows.contains(&row) {
                        ensure!(
                            rows.len() < self.limits.max_query_rows,
                            "datalog_query_row_budget"
                        );
                        rows.insert(row);
                    }
                }
            }
        }
        Ok(rows.into_iter().collect())
    }

    /// Original query order, including inference that precedes a syntax error.
    /// The existing query method retains its validate-before-inference behavior.
    pub fn query_source_order(
        &mut self,
        pattern: &str,
        bindings: Option<&Bindings>,
    ) -> Result<Vec<Bindings>> {
        if !self.rules.is_empty() && !self.derived {
            self.derive_all()?;
        }
        self.query(pattern, bindings)
    }

    pub fn load_from_graph(&mut self, graph: &impl ContextGraphSource) -> Result<usize> {
        self.load_records(graph.find_edges().iter().chain(graph.find_nodes()))
    }

    /// Original per-record graph import. A failed later record keeps the valid
    /// prefix; the existing load_from_graph method remains transactional.
    pub fn load_from_graph_incremental(
        &mut self,
        graph: &impl ContextGraphSource,
    ) -> Result<usize> {
        self.load_records_incremental(graph.find_edges().iter().chain(graph.find_nodes()))
    }
    /// Transport adapters may supply arrays of nodes/edges or a node-id map.
    /// Dynamic language object inspection is performed by the adapter, not by
    /// this generic reasoner.
    pub fn load_from_graph_value(&mut self, graph: &Value) -> Result<usize> {
        let object = graph.as_object().context("datalog_context_graph_invalid")?;
        let mut entries = Vec::new();
        if let Some(edges) = object.get("edges") {
            let edges = edges.as_array().context("datalog_context_edges_invalid")?;
            ensure!(
                edges.len() <= self.limits.max_context_entries,
                "datalog_context_entry_budget"
            );
            entries.extend(edges);
        }
        if let Some(nodes) = object.get("nodes") {
            let node_count = match nodes {
                Value::Array(nodes) => nodes.len(),
                Value::Object(nodes) => nodes.len(),
                _ => bail!("datalog_context_nodes_invalid"),
            };
            ensure!(
                node_count
                    <= self
                        .limits
                        .max_context_entries
                        .saturating_sub(entries.len()),
                "datalog_context_entry_budget"
            );
            match nodes {
                Value::Array(nodes) => entries.extend(nodes),
                Value::Object(nodes) => entries.extend(nodes.values()),
                _ => bail!("datalog_context_nodes_invalid"),
            }
        }
        self.load_records(entries)
    }
    fn load_records<'a>(&mut self, entries: impl IntoIterator<Item = &'a Value>) -> Result<usize> {
        let mut staged = self.clone();
        let added = staged.load_records_incremental(entries)?;
        *self = staged;
        Ok(added)
    }

    fn load_records_incremental<'a>(
        &mut self,
        entries: impl IntoIterator<Item = &'a Value>,
    ) -> Result<usize> {
        let before = self.all_facts.len();
        for (index, entry) in entries.into_iter().enumerate() {
            ensure!(
                index < self.limits.max_context_entries,
                "datalog_context_entry_budget"
            );
            self.add_fact(entry)?;
        }
        Ok(self.all_facts.len() - before)
    }
}

fn is_variable(term: &str) -> bool {
    term.chars().next().is_some_and(char::is_uppercase)
}
fn internal_variable(term: &str) -> String {
    let mut chars = term.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}
fn parse_atom(text: &str, terminal_period: bool, limits: &DatalogLimits) -> Result<BodyAtom> {
    ensure!(
        text.len() <= limits.max_input_bytes,
        "datalog_input_byte_budget"
    );
    let mut text = text.trim();
    if terminal_period {
        text = text.strip_suffix('.').unwrap_or(text).trim_end();
    }
    let (predicate, rest) = text
        .split_once('(')
        .context("datalog_atom_syntax_invalid")?;
    let predicate = predicate.trim();
    ensure!(
        !predicate.is_empty()
            && predicate
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_'),
        "datalog_predicate_syntax_invalid"
    );
    ensure!(
        predicate.len() <= limits.max_term_bytes,
        "datalog_term_byte_budget"
    );
    let values = rest
        .strip_suffix(')')
        .context("datalog_atom_syntax_invalid")?;
    ensure!(
        !values.contains(['(', ')']),
        "datalog_function_terms_unsupported"
    );
    let mut args = Vec::new();
    for arg in values.split(',') {
        let arg = arg.trim();
        ensure!(!arg.is_empty(), "datalog_empty_argument: Empty argument");
        ensure!(!arg.contains('\0'), "datalog_nul_term_unsupported");
        ensure!(
            arg.len() <= limits.max_term_bytes,
            "datalog_term_byte_budget"
        );
        ensure!(args.len() < limits.max_arity, "datalog_arity_budget");
        args.push(arg.to_string());
    }
    Ok(BodyAtom {
        predicate: predicate.to_string(),
        args,
    })
}
fn parse_rule(text: &str, limits: &DatalogLimits) -> Result<DatalogRule> {
    ensure!(
        text.len() <= limits.max_input_bytes,
        "datalog_input_byte_budget"
    );
    let (head, body) = text
        .trim()
        .split_once(":-")
        .context("datalog_rule_separator_missing")?;
    let head = parse_atom(head, false, limits)?;
    let mut tail = body
        .trim()
        .strip_suffix('.')
        .unwrap_or(body.trim())
        .trim_end();
    let mut atoms = Vec::new();
    while !tail.is_empty() {
        let close = tail.find(')').context("datalog_rule_body_syntax_invalid")?;
        let atom = parse_atom(&tail[..=close], false, limits)?;
        ensure!(
            atoms.len() < limits.max_body_atoms,
            "datalog_body_atom_budget"
        );
        atoms.push(atom);
        tail = tail[close + 1..].trim_start();
        if !tail.is_empty() {
            tail = tail
                .strip_prefix(',')
                .context("datalog_rule_body_trailing_tokens")?
                .trim_start();
            ensure!(!tail.is_empty(), "datalog_rule_body_syntax_invalid");
        }
    }
    ensure!(!atoms.is_empty(), "datalog_rule_body_missing");
    let variables: BTreeSet<_> = atoms
        .iter()
        .flat_map(|a| &a.args)
        .filter(|a| is_variable(a))
        .collect();
    ensure!(
        head.args
            .iter()
            .filter(|a| is_variable(a))
            .all(|a| variables.contains(a)),
        "datalog_unsafe_head_variable"
    );
    Ok(DatalogRule {
        head_predicate: head.predicate,
        head_args: head.args,
        body: atoms,
    })
}
fn validate_fact(fact: &DatalogFact, limits: &DatalogLimits) -> Result<()> {
    ensure!(
        fact.predicate.len() <= limits.max_term_bytes && !fact.predicate.contains('\0'),
        "datalog_predicate_invalid"
    );
    ensure!(
        !fact.args.is_empty() && fact.args.len() <= limits.max_arity,
        "datalog_arity_budget"
    );
    for arg in &fact.args {
        ensure!(!arg.is_empty(), "datalog_empty_argument: Empty argument");
        ensure!(
            arg.len() <= limits.max_term_bytes,
            "datalog_term_byte_budget"
        );
        ensure!(!arg.contains('\0'), "datalog_nul_term_unsupported");
        ensure!(!is_variable(arg), "datalog_fact_variable_unsupported");
    }
    Ok(())
}
fn fact_size(fact: &DatalogFact) -> Result<usize> {
    fact.args
        .iter()
        .try_fold(fact.predicate.len(), |total, arg| {
            total
                .checked_add(arg.len())
                .context("datalog_fact_byte_budget")
        })
}
fn python_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64() != Some(0.0),
        Value::String(v) => !v.is_empty(),
        Value::Array(v) => !v.is_empty(),
        Value::Object(v) => !v.is_empty(),
    }
}
fn dict_term(value: &Value, limits: &DatalogLimits) -> Result<String> {
    // Scalar source identifiers have portable JSON identity. Complex Python
    // repr constants remain an explicit unsupported input in this first port.
    let text = match value {
        Value::String(v) => {
            ensure!(v.len() <= limits.max_term_bytes, "datalog_term_byte_budget");
            v.clone()
        }
        // Original dictionary facts use Python str before symbol normalization.
        // JSON display differs for e.g. 0.00001 and would change joins/dedup.
        Value::Number(_) => {
            semantica_json::python_str(value).context("datalog_dictionary_numeric_term_invalid")?
        }
        Value::Bool(v) => v.to_string(),
        Value::Null => "None".to_string(),
        _ => bail!("datalog_dictionary_complex_term_unsupported"),
    };
    ensure!(
        text.len() <= limits.max_term_bytes,
        "datalog_term_byte_budget"
    );
    Ok(text.replace(' ', "_").to_lowercase())
}
fn first<'a>(object: &'a serde_json::Map<String, Value>, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|key| object.get(*key))
}
fn dict_fact(value: &Value, limits: &DatalogLimits) -> Result<Option<DatalogFact>> {
    let Some(object) = value.as_object() else {
        bail!("datalog_dictionary_fact_invalid");
    };
    let fact = if let (Some(subject), Some(predicate), Some(object_value)) = (
        object.get("subject"),
        object.get("predicate"),
        object.get("object"),
    ) {
        Some(DatalogFact {
            predicate: dict_term(predicate, limits)?,
            args: vec![
                dict_term(subject, limits)?,
                dict_term(object_value, limits)?,
            ],
        })
    } else if object.contains_key("source")
        || object.contains_key("source_id")
        || object.contains_key("source_name")
    {
        let source = first(object, &["source", "source_name", "source_id"]);
        let target = first(object, &["target", "target_name", "target_id"]);
        match (source, target) {
            (Some(source), Some(target)) if python_truthy(source) && python_truthy(target) => {
                Some(DatalogFact {
                    predicate: match first(object, &["type", "relation"]) {
                        Some(kind) => dict_term(kind, limits)?,
                        None => "connected_to".to_string(),
                    },
                    args: vec![dict_term(source, limits)?, dict_term(target, limits)?],
                })
            }
            _ => None,
        }
    } else if let (Some(kind), Some(name)) = (object.get("type"), first(object, &["id", "name"])) {
        if python_truthy(name) {
            Some(DatalogFact {
                predicate: dict_term(kind, limits)?,
                args: vec![dict_term(name, limits)?],
            })
        } else {
            None
        }
    } else {
        None
    };
    Ok(fact)
}
fn unify(pattern: &[String], fact: &[String], initial: &Bindings) -> Option<Bindings> {
    if pattern.len() != fact.len() {
        return None;
    }
    let mut bindings = initial.clone();
    for (term, value) in pattern.iter().zip(fact) {
        if is_variable(term) {
            match bindings.get(term) {
                Some(bound) if bound != value => return None,
                Some(_) => {}
                None => {
                    bindings.insert(term.clone(), value.clone());
                }
            }
        } else if term != value {
            return None;
        }
    }
    Some(bindings)
}
fn index_facts(facts: &BTreeSet<DatalogFact>) -> FactIndex {
    let mut index = FactIndex::new();
    for fact in facts {
        index
            .entry(fact.predicate.clone())
            .or_default()
            .insert(fact.clone());
    }
    index
}
fn count_match(stats: &mut DatalogStats, limits: &DatalogLimits) -> Result<()> {
    ensure!(
        stats.match_attempts < limits.max_match_attempts,
        "datalog_match_budget"
    );
    stats.match_attempts += 1;
    Ok(())
}
fn apply_rule(
    rule: &DatalogRule,
    all: &FactIndex,
    delta: &FactIndex,
    limits: &DatalogLimits,
    stats: &mut DatalogStats,
) -> Result<BTreeSet<DatalogFact>> {
    let mut results = BTreeSet::new();
    // Every path anchors one body atom in the last round's delta. Other atoms
    // use the accumulated facts. This supports all variable joins and mutual
    // recursion without re-evaluating tuples having no new input.
    for delta_position in 0..rule.body.len() {
        let mut bindings = BTreeSet::from([Bindings::new()]);
        for (position, atom) in rule.body.iter().enumerate() {
            let source = if position == delta_position {
                delta
            } else {
                all
            };
            let Some(facts) = source.get(&atom.predicate) else {
                bindings.clear();
                break;
            };
            let mut joined = BTreeSet::new();
            for prior in &bindings {
                for fact in facts {
                    count_match(stats, limits)?;
                    if let Some(row) = unify(&atom.args, &fact.args, prior) {
                        if !joined.contains(&row) {
                            ensure!(
                                joined.len() < limits.max_bindings,
                                "datalog_join_binding_budget"
                            );
                            joined.insert(row);
                        }
                    }
                }
            }
            bindings = joined;
            if bindings.is_empty() {
                break;
            }
        }
        for row in bindings {
            let args = rule
                .head_args
                .iter()
                .map(|arg| {
                    if is_variable(arg) {
                        row.get(arg)
                            .cloned()
                            .context("datalog_unbound_head_variable")
                    } else {
                        Ok(arg.clone())
                    }
                })
                .collect::<Result<Vec<_>>>()?;
            let fact = DatalogFact {
                predicate: rule.head_predicate.clone(),
                args,
            };
            if !results.contains(&fact) {
                ensure!(results.len() < limits.max_facts, "datalog_fact_budget");
                results.insert(fact);
            }
        }
    }
    Ok(results)
}
