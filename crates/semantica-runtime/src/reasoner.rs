//! Original high-level Semantica Reasoner string-rule forward chaining.
//! This is not the separate Datalog engine, RDF reasoning, or model inference.
use anyhow::{Result, ensure};
use regex::Regex;
use serde_json::Value;
use std::{collections::BTreeSet, fmt::Write};

// Reuse the already frozen CPython Unicode 15 printability reference data.
#[path = "providers/python_repr_data.rs"]
mod python_repr_data;

pub const PROFILE: &str = "semantica.reasoner-string-rules-default-50-bounded/1";
pub const MAX_PASSES: usize = 50;
pub const MAX_INPUT_BYTES: usize = 1024 * 1024;
pub const MAX_FACTS: usize = 4096;
pub const MAX_RULES: usize = 256;
pub const MAX_CONDITIONS: usize = 32;
pub const MAX_TEXT_BYTES: usize = 4096;
pub const MAX_PATTERN_SEGMENTS: usize = 128;
pub const MAX_BINDINGS: usize = 8192;
pub const MAX_BINDING_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_WORKING_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_MATCH_STEPS: usize = 1_000_000;

const SPACE: &str = r"[\t\n\x0b\x0c\r\x1c-\x20\x85\u{a0}\u{1680}\u{2000}-\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}]";

fn python_space(character: char) -> bool {
    matches!(character, '\u{9}'..='\u{d}' | '\u{1c}'..='\u{20}' | '\u{85}' | '\u{a0}' |
        '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}')
}

fn text_limit(text: &str) -> Result<()> {
    ensure!(
        text.len() <= MAX_TEXT_BYTES,
        "reasoner_text_limit_exceeded: {PROFILE}"
    );
    Ok(())
}

fn string_repr(text: &str, output: &mut String) {
    let quote = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    output.push(quote);
    for character in text.chars() {
        match character {
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            value if value == quote => {
                output.push('\\');
                output.push(value);
            }
            value => {
                let codepoint = value as u32;
                let ranges = python_repr_data::NONPRINTABLE;
                let index = ranges.partition_point(|(start, _)| *start <= codepoint);
                if index == 0 || ranges[index - 1].1 < codepoint {
                    output.push(value);
                } else if codepoint <= 0xff {
                    write!(output, "\\x{codepoint:02x}").expect("String write");
                } else if codepoint <= 0xffff {
                    write!(output, "\\u{codepoint:04x}").expect("String write");
                } else {
                    write!(output, "\\U{codepoint:08x}").expect("String write");
                }
            }
        }
    }
    output.push(quote);
}

// f-string coercion for finite JSON values: strings are raw at the top level,
// while list/dict members use Python repr and preserve dictionary input order.
fn python_string(value: &Value, quoted: bool, depth: usize, output: &mut String) -> Result<()> {
    ensure!(depth <= 128, "reasoner_json_depth_uncovered: {PROFILE}");
    match value {
        Value::Null => output.push_str("None"),
        Value::Bool(true) => output.push_str("True"),
        Value::Bool(false) => output.push_str("False"),
        Value::String(text) if quoted => string_repr(text, output),
        Value::String(text) => output.push_str(text),
        Value::Number(_) => {
            let bytes = semantica_contracts::json::canonical_json(value)
                .map_err(|_| anyhow::anyhow!("reasoner_nonfinite_json_uncovered: {PROFILE}"))?;
            output.push_str(std::str::from_utf8(&bytes).expect("numeric ASCII"));
        }
        Value::Array(values) => {
            output.push('[');
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    output.push_str(", ");
                }
                python_string(value, true, depth + 1, output)?;
            }
            output.push(']');
        }
        Value::Object(values) => {
            output.push('{');
            for (index, (key, value)) in values.iter().enumerate() {
                if index > 0 {
                    output.push_str(", ");
                }
                string_repr(key, output);
                output.push_str(": ");
                python_string(value, true, depth + 1, output)?;
            }
            output.push('}');
        }
    }
    text_limit(output)
}

fn fact_string(value: &Value) -> Result<Option<String>> {
    if let Value::String(text) = value {
        let text = text.trim_matches(python_space).to_owned();
        text_limit(&text)?;
        return Ok(Some(text));
    }
    let Some(fact) = value.as_object() else {
        return Ok(None);
    };
    let mut result = String::new();
    let none = Value::Null;
    if fact.contains_key("type") && (fact.contains_key("name") || fact.contains_key("id")) {
        python_string(fact.get("type").unwrap_or(&none), false, 0, &mut result)?;
        result.push('(');
        python_string(
            fact.get("name").or_else(|| fact.get("id")).unwrap_or(&none),
            false,
            0,
            &mut result,
        )?;
        result.push(')');
    } else if fact.contains_key("source_id") || fact.contains_key("source_name") {
        python_string(
            fact.get("type")
                .unwrap_or(&Value::String("Relationship".into())),
            false,
            0,
            &mut result,
        )?;
        result.push('(');
        python_string(
            fact.get("source_name")
                .or_else(|| fact.get("source_id"))
                .unwrap_or(&none),
            false,
            0,
            &mut result,
        )?;
        result.push_str(", ");
        python_string(
            fact.get("target_name")
                .or_else(|| fact.get("target_id"))
                .unwrap_or(&none),
            false,
            0,
            &mut result,
        )?;
        result.push(')');
    } else {
        return Ok(None);
    }
    text_limit(&result)?;
    Ok(Some(result))
}

#[derive(Clone, PartialEq, Eq)]
enum Segment {
    Literal(String),
    Variable(String),
}

#[derive(Clone, PartialEq, Eq)]
struct Pattern {
    source: String,
    segments: Vec<Segment>,
    valid: bool,
}

impl Pattern {
    fn parse(source: &str) -> Result<Self> {
        text_limit(source)?;
        let mut segments = Vec::new();
        let mut valid = true;
        let mut literal_start = 0;
        let mut cursor = 0;
        while cursor < source.len() {
            let character = source[cursor..].chars().next().expect("character boundary");
            if character != '?' {
                cursor += character.len_utf8();
                continue;
            }
            let mut end = cursor + 1;
            while let Some(character) = source[end..].chars().next() {
                if character != '_' && !character.is_alphanumeric() {
                    break;
                }
                ensure!(
                    character.is_ascii(),
                    "reasoner_unicode_variable_uncovered: {PROFILE}"
                );
                end += character.len_utf8();
            }
            if end == cursor + 1 {
                cursor += 1;
                continue;
            }
            // Original re.split keeps literal segments. Its seg.startswith('?')
            // check makes a leading unmatched '?' an invalid group too.
            let literal = &source[literal_start..cursor];
            if literal.starts_with('?') {
                valid = false;
            }
            segments.push(Segment::Literal(literal.into()));
            let name = &source[cursor + 1..end];
            if !name
                .starts_with(|character: char| character.is_ascii_alphabetic() || character == '_')
            {
                valid = false;
            }
            segments.push(Segment::Variable(name.into()));
            literal_start = end;
            cursor = end;
        }
        let literal = &source[literal_start..];
        if literal.starts_with('?') {
            valid = false;
        }
        segments.push(Segment::Literal(literal.into()));
        ensure!(
            segments.len() <= MAX_PATTERN_SEGMENTS,
            "reasoner_pattern_limit_exceeded: {PROFILE}"
        );
        Ok(Self {
            source: source.into(),
            segments,
            valid,
        })
    }

    fn match_fact(
        &self,
        fact: &str,
        initial: &Bindings,
        work: &mut MatchLimit,
    ) -> Result<Option<Bindings>> {
        if !self.valid {
            return Ok(None);
        }
        let mut bindings = initial.clone();
        Ok(self
            .match_at(0, fact, 0, &mut bindings, work)?
            .then_some(bindings))
    }

    // Literal matching plus lazy, nonempty, non-newline capture and exact
    // backreferences reproduces the original escaped Python pattern grammar.
    fn match_at(
        &self,
        index: usize,
        fact: &str,
        offset: usize,
        bindings: &mut Bindings,
        work: &mut MatchLimit,
    ) -> Result<bool> {
        work.step()?;
        let Some(segment) = self.segments.get(index) else {
            return Ok(offset == fact.len() || &fact[offset..] == "\n"); // Python '$'
        };
        match segment {
            Segment::Literal(literal) => {
                if !fact[offset..].starts_with(literal) {
                    return Ok(false);
                }
                self.match_at(index + 1, fact, offset + literal.len(), bindings, work)
            }
            Segment::Variable(name) => {
                if let Some((_, value)) = bindings.iter().find(|(variable, _)| variable == name) {
                    if !fact[offset..].starts_with(value) {
                        return Ok(false);
                    }
                    let end = offset + value.len();
                    return self.match_at(index + 1, fact, end, bindings, work);
                }
                for (relative, character) in fact[offset..].char_indices() {
                    if character == '\n' {
                        break;
                    }
                    let end = offset + relative + character.len_utf8();
                    work.step()?;
                    bindings.push((name.clone(), fact[offset..end].into()));
                    if self.match_at(index + 1, fact, end, bindings, work)? {
                        return Ok(true);
                    }
                    bindings.pop();
                }
                Ok(false)
            }
        }
    }
}

type Bindings = Vec<(String, String)>; // Original dictionary insertion order.
struct MatchLimit {
    steps: usize,
}
impl MatchLimit {
    fn step(&mut self) -> Result<()> {
        self.steps += 1;
        ensure!(
            self.steps <= MAX_MATCH_STEPS,
            "reasoner_match_limit_exceeded: {PROFILE}"
        );
        Ok(())
    }
}

#[derive(PartialEq, Eq)]
struct Rule {
    conditions: Vec<Pattern>,
    conclusion: String,
}

fn parse_rules(values: &Value) -> Result<Vec<Rule>> {
    if values.is_null() {
        return Ok(Vec::new());
    }
    let values = values
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("reasoner_rules_shape_uncovered: {PROFILE}"))?;
    ensure!(
        values.len() <= MAX_RULES,
        "reasoner_rule_limit_exceeded: {PROFILE}"
    );
    let definition = Regex::new(&format!(
        r"(?s)^[Iiİı][Ff]{SPACE}+(.+?){SPACE}+[Tt][Hh][Ee][Nn]{SPACE}+(.+)$"
    ))
    .expect("fixed original parser grammar");
    let conjunction =
        Regex::new(&format!(r"{SPACE}+[Aa][Nn][Dd]{SPACE}+")).expect("fixed original AND grammar");
    let mut rules = Vec::new();
    for value in values {
        let text = value
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("reasoner_rule_kind_uncovered: {PROFILE}"))?;
        text_limit(text)?;
        let text = text.trim_matches(python_space);
        let Some(captures) = definition.captures(text) else {
            continue;
        }; // Empty fallback conditions do not execute.
        let conditions = conjunction
            .split(&captures[1])
            .map(|condition| Pattern::parse(condition.trim_matches(python_space)))
            .collect::<Result<Vec<_>>>()?;
        ensure!(
            conditions.len() <= MAX_CONDITIONS,
            "reasoner_condition_limit_exceeded: {PROFILE}"
        );
        let rule = Rule {
            conditions,
            conclusion: captures[2].trim_matches(python_space).into(),
        };
        if !rules.contains(&rule) {
            rules.push(rule);
        }
    }
    // Original parsed Rule defaults are priority 0/confidence 1.0. Equal
    // priorities are stable, and duplicate rules retain the original record.
    Ok(rules)
}

fn match_rule(
    rule: &Rule,
    facts: &BTreeSet<String>,
    work: &mut MatchLimit,
) -> Result<Vec<Bindings>> {
    if rule.conditions.is_empty() {
        return Ok(Vec::new());
    }
    let mut bindings = vec![Vec::new()];
    for condition in &rule.conditions {
        let mut next = Vec::new();
        let mut bytes = 0;
        for initial in &bindings {
            for fact in facts {
                work.step()?;
                if let Some(binding) = condition.match_fact(fact, initial, work)? {
                    bytes += binding
                        .iter()
                        .map(|(name, value)| name.len() + value.len())
                        .sum::<usize>();
                    ensure!(
                        next.len() < MAX_BINDINGS && bytes <= MAX_BINDING_BYTES,
                        "reasoner_binding_limit_exceeded: {PROFILE}"
                    );
                    next.push(binding);
                }
            }
        }
        bindings = next;
        if bindings.is_empty() {
            break;
        }
    }
    Ok(bindings)
}

/// Preserve the original sequential global replace, including prefix collisions
/// and replacements introduced by values. Token-safe substitution changes it.
fn substitute(pattern: &str, bindings: &Bindings) -> Result<String> {
    let mut result = pattern.to_owned();
    for (variable, value) in bindings {
        result = result.replace(&format!("?{variable}"), value);
        text_limit(&result)?;
    }
    Ok(result)
}

/// Original Reasoner.infer_facts output for finite JSON facts and string rules.
/// Limits are explicit in PROFILE; non-ASCII variable identifiers, Python Rule
/// objects, custom handlers/config, backward chaining and unbounded resources
/// remain uncovered. No DatasetSession, call budget or model is constructed.
pub fn infer_facts(facts: &Value, rules: &Value) -> Result<Value> {
    let input_bytes = [facts, rules]
        .into_iter()
        .try_fold(0usize, |count, value| {
            let bytes = semantica_contracts::json::ordered_json(value)
                .map_err(|_| anyhow::anyhow!("reasoner_input_json_uncovered: {PROFILE}"))?;
            Ok::<_, anyhow::Error>(count + bytes.len())
        })?;
    ensure!(
        input_bytes <= MAX_INPUT_BYTES,
        "reasoner_input_limit_exceeded: {PROFILE}"
    );
    let rules = parse_rules(rules)?;
    let mut memory = BTreeSet::new();
    let mut memory_bytes = 0;
    let mut add = |value: &Value| -> Result<()> {
        if let Some(fact) = fact_string(value)?
            && memory.insert(fact.clone())
        {
            memory_bytes += fact.len();
            ensure!(
                memory.len() <= MAX_FACTS && memory_bytes <= MAX_WORKING_BYTES,
                "reasoner_fact_limit_exceeded: {PROFILE}"
            );
        }
        Ok(())
    };
    if let Value::Array(facts) = facts {
        ensure!(
            facts.len() <= MAX_FACTS,
            "reasoner_fact_limit_exceeded: {PROFILE}"
        );
        for fact in facts {
            add(fact)?;
        }
    } else {
        add(facts)?;
    }
    let mut results = Vec::new();
    let mut work = MatchLimit { steps: 0 };
    for _ in 0..MAX_PASSES {
        let mut added = false;
        for rule in &rules {
            // Facts are sorted once for each rule. All matches are computed
            // before its conclusions are inserted, while later rules in the
            // same pass immediately see earlier rules' new facts.
            for bindings in match_rule(rule, &memory, &mut work)? {
                let conclusion = substitute(&rule.conclusion, &bindings)?;
                if memory.insert(conclusion.clone()) {
                    memory_bytes += conclusion.len();
                    ensure!(
                        memory.len() <= MAX_FACTS && memory_bytes <= MAX_WORKING_BYTES,
                        "reasoner_fact_limit_exceeded: {PROFILE}"
                    );
                    results.push(Value::String(conclusion));
                    added = true;
                }
            }
        }
        if !added {
            return Ok(Value::Array(results));
        }
    }
    // The original default is exactly 50 passes, even when another pass could
    // derive more facts. This return is an original limit, not convergence.
    Ok(Value::Array(results))
}

/// Original SemanticRuntime.reason string-asset adapter. Keep its existing
/// BookRunner result and errors while sharing the structured rule implementation.
pub(crate) fn forward_chain_text(facts: &str, rules: &str, max_iterations: u64) -> Result<Value> {
    ensure!(
        max_iterations > 0 && max_iterations <= 1000,
        "runtime_reason_iteration_uncovered"
    );
    ensure!(
        facts.len() + rules.len() <= MAX_INPUT_BYTES,
        "runtime_reason_input_limit"
    );
    forward_chain_impl(
        &Value::String(facts.into()),
        &Value::String(rules.into()),
        max_iterations,
    )
}

fn forward_lines(text: &str) -> Vec<String> {
    text.split([
        '\n', '\r', '\u{b}', '\u{c}', '\u{1c}', '\u{1d}', '\u{1e}', '\u{85}', '\u{2028}',
        '\u{2029}',
    ])
    .map(|line| line.trim_matches(python_space))
    .filter(|line| !line.is_empty() && !line.starts_with('#'))
    .map(str::to_owned)
    .collect()
}

fn one_forward_line(value: &Value, kind: &str) -> Result<String> {
    let text = value
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("runtime_reason_input: {kind} must be text"))?;
    let lines = forward_lines(text);
    ensure!(
        lines.len() == 1,
        "runtime_reason_input: each {kind} item must contain exactly one line"
    );
    Ok(lines.into_iter().next().expect("one line"))
}

/// JSON wire adaptation of original ordered strings / ForwardRuleDTO inputs.
/// Objects in the wire denote DTOs; the original Python API did not accept dicts.
/// Uses the existing matcher and bounded convergence probe, not a second engine.
pub fn forward_chain_values(facts: &Value, rules: &Value, max_iterations: u64) -> Result<Value> {
    let input_bytes = [facts, rules]
        .into_iter()
        .try_fold(0usize, |total, value| {
            let bytes = semantica_contracts::json::ordered_json(value)?;
            Ok::<_, anyhow::Error>(total + bytes.len())
        })?;
    ensure!(input_bytes <= MAX_INPUT_BYTES, "runtime_reason_input_limit");
    forward_chain_impl(facts, rules, max_iterations)
}

fn forward_chain_impl(facts: &Value, rules: &Value, max_iterations: u64) -> Result<Value> {
    use num_bigint::BigInt;
    use semantica_contracts::SemanticForwardRule;
    use serde_json::json;
    ensure!(
        max_iterations > 0 && max_iterations <= 1000,
        "runtime_reason_iteration_uncovered"
    );
    let fact_lines = match facts {
        Value::String(text) => forward_lines(text),
        Value::Array(values) => {
            ensure!(values.len() <= MAX_FACTS, "runtime_reason_count_limit");
            values
                .iter()
                .map(|value| one_forward_line(value, "fact"))
                .collect::<Result<Vec<_>>>()?
        }
        _ => anyhow::bail!("runtime_reason_input: facts must be text or ordered array"),
    };
    let raw_rules = match rules {
        Value::String(text) => forward_lines(text).into_iter().map(Value::String).collect(),
        Value::Array(values) => values.clone(),
        _ => anyhow::bail!("runtime_reason_input: rules must be text or ordered array"),
    };
    ensure!(
        !fact_lines.is_empty() && !raw_rules.is_empty(),
        "runtime_reason_input: facts and rules are required"
    );
    ensure!(
        fact_lines.len() <= MAX_FACTS && raw_rules.len() <= MAX_RULES,
        "runtime_reason_count_limit"
    );
    let starts_if = Regex::new(&format!(r"^[Iiİı][Ff]{SPACE}+")).expect("fixed fact grammar");
    let mut memory = BTreeSet::new();
    for fact in fact_lines {
        text_limit(&fact)?;
        ensure!(
            !fact.contains('?') && !starts_if.is_match(&fact),
            "runtime_reason_input: ground fact required"
        );
        memory.insert(fact);
    }
    let input_facts: Vec<_> = memory.iter().cloned().collect();
    let mut normalized = Vec::new();
    let mut next_id = 1;
    for value in raw_rules {
        let mut contract = if value.is_string() {
            let text = one_forward_line(&value, "rule")?;
            let contract = SemanticForwardRule {
                rule_id: format!("rule_{next_id}"),
                text,
                confidence: 1.0,
                priority: json!(0),
            };
            next_id += 1;
            contract
        } else if let Value::Object(mut object) = value {
            object.entry("confidence").or_insert(json!(1.0));
            object.entry("priority").or_insert(json!(0));
            serde_json::from_value::<SemanticForwardRule>(Value::Object(object)).map_err(|_| {
                anyhow::anyhow!("runtime_reason_input: invalid ForwardRuleDTO wire fields")
            })?
        } else {
            anyhow::bail!("runtime_reason_input: rule must be text or ForwardRuleDTO wire object")
        };
        text_limit(&contract.rule_id)?;
        ensure!(
            !contract.rule_id.is_empty()
                && contract.rule_id == contract.rule_id.trim_matches(python_space),
            "runtime_reason_input: invalid rule ID"
        );
        ensure!(
            contract.confidence.is_finite() && (0.0..=1.0).contains(&contract.confidence),
            "runtime_reason_input: confidence must be finite in [0,1]"
        );
        // Python ints are not restricted to i64. Preserve the original JSON
        // integer, compare with BigInt, and reject floats (including 1.0)/bools.
        let priority_text = contract
            .priority
            .as_number()
            .map(ToString::to_string)
            .ok_or_else(|| anyhow::anyhow!("runtime_reason_input: priority must be an integer"))?;
        ensure!(
            !priority_text.contains(['.', 'e', 'E']),
            "runtime_reason_input: priority must be an integer"
        );
        let priority: BigInt = priority_text
            .parse()
            .map_err(|_| anyhow::anyhow!("runtime_reason_input: priority must be an integer"))?;
        contract.text = contract.text.trim_matches(python_space).to_owned();
        let text = &contract.text;
        ensure!(
            !text.contains(['\n', '\r']),
            "runtime_reason_input: each rule must occupy one line"
        );
        ensure!(
            !text.to_lowercase().contains("swrlb:") && !text.contains(['→', '∧']),
            "runtime_reason_input: SWRL outside profile"
        );
        let mut single = parse_rules(&json!([text]))?;
        ensure!(
            single.len() == 1
                && single[0].conditions.iter().all(|v| !v.source.is_empty())
                && !single[0].conclusion.is_empty(),
            "runtime_reason_input: invalid rule syntax"
        );
        let rule = single.remove(0);
        let variables = |pattern: &Pattern| -> BTreeSet<String> {
            pattern
                .segments
                .iter()
                .filter_map(|segment| match segment {
                    Segment::Variable(name)
                        if name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') =>
                    {
                        Some(name.clone())
                    }
                    _ => None,
                })
                .collect()
        };
        let bound: BTreeSet<String> = rule.conditions.iter().flat_map(variables).collect();
        let conclusion = variables(&Pattern::parse(&rule.conclusion)?);
        ensure!(
            conclusion.is_subset(&bound),
            "runtime_reason_input: unbound conclusion variable"
        );
        normalized.push((priority, contract, rule));
    }
    // Original normalization completes every DTO before checking duplicates.
    let mut seen_ids = BTreeSet::new();
    for (index, (_, contract, rule)) in normalized.iter().enumerate() {
        ensure!(
            seen_ids.insert(contract.rule_id.clone()),
            "runtime_reason_input: duplicate rule ID"
        );
        ensure!(
            !normalized[..index]
                .iter()
                .any(|(_, _, existing)| existing == rule),
            "runtime_reason_input: semantic duplicate"
        );
    }
    // Stable descending priority; identical priority retains caller order.
    normalized.sort_by(|left, right| right.0.cmp(&left.0));
    let (rule_contracts, parsed): (Vec<_>, Vec<_>) = normalized
        .into_iter()
        .map(|(_, contract, rule)| (contract, rule))
        .unzip();
    struct Detail {
        conclusion: String,
        rule: usize,
        premises: Vec<String>,
    }
    fn matched(
        rule: &Rule,
        facts: &BTreeSet<String>,
        work: &mut MatchLimit,
    ) -> Result<Vec<(Bindings, Vec<String>)>> {
        let mut bindings = vec![(Vec::new(), Vec::<String>::new())];
        for condition in &rule.conditions {
            let mut next = Vec::new();
            let mut bytes = 0;
            for (initial, premises) in &bindings {
                for fact in facts {
                    work.step()?;
                    if let Some(binding) = condition.match_fact(fact, initial, work)? {
                        let mut premises = premises.clone();
                        premises.push(fact.clone());
                        bytes += binding
                            .iter()
                            .map(|(name, value)| name.len() + value.len())
                            .sum::<usize>()
                            + premises.iter().map(String::len).sum::<usize>();
                        ensure!(
                            next.len() < MAX_BINDINGS && bytes <= MAX_BINDING_BYTES,
                            "runtime_reason_binding_limit"
                        );
                        next.push((binding, premises));
                    }
                }
            }
            bindings = next;
            if bindings.is_empty() {
                break;
            }
        }
        Ok(bindings)
    }
    fn chain(
        memory: &mut BTreeSet<String>,
        rules: &[Rule],
        max: u64,
        work: &mut MatchLimit,
    ) -> Result<Vec<Detail>> {
        let mut results: Vec<Detail> = Vec::new();
        for _ in 0..max {
            let before = memory.clone();
            let mut pass_results = std::collections::BTreeMap::<String, usize>::new();
            let mut added = false;
            for (index, rule) in rules.iter().enumerate() {
                // Original _match_rule prepares all matches before mutation.
                for (bindings, premises) in matched(rule, memory, work)? {
                    let conclusion = substitute(&rule.conclusion, &bindings)?;
                    if let Some(existing) = pass_results.get(&conclusion) {
                        for premise in premises {
                            if !results[*existing].premises.contains(&premise) {
                                results[*existing].premises.push(premise);
                            }
                        }
                        continue;
                    }
                    if before.contains(&conclusion) {
                        continue;
                    }
                    memory.insert(conclusion.clone());
                    ensure!(
                        memory.len() <= MAX_FACTS
                            && memory.iter().map(String::len).sum::<usize>() <= MAX_WORKING_BYTES,
                        "runtime_reason_fact_limit"
                    );
                    pass_results.insert(conclusion.clone(), results.len());
                    results.push(Detail {
                        conclusion,
                        rule: index,
                        premises,
                    });
                    added = true;
                }
            }
            if !added {
                break;
            }
        }
        Ok(results)
    }
    let mut work = MatchLimit { steps: 0 };
    let inferences = chain(&mut memory, &parsed, max_iterations, &mut work)?;
    ensure!(
        chain(&mut memory, &parsed, max_iterations, &mut work)?.is_empty(),
        "runtime_reason_convergence: forward reasoning did not converge within max_iterations={max_iterations}"
    );
    let inferences:Vec<Value>=inferences.into_iter().map(|inference| {
        let contract = &rule_contracts[inference.rule];
        let rule_id = &contract.rule_id;
        let explanation=format!("Given the premises: {}, we conclude: {} using rule '{}'.",inference.premises.join(", "),inference.conclusion,rule_id);
        json!({"conclusion":inference.conclusion,"rule_id":rule_id,"rule_text":contract.text,"premises":inference.premises,"confidence":contract.confidence,"explanation":explanation})
    }).collect();
    Ok(
        json!({"input_facts":input_facts,"rules":rule_contracts,"inferences":inferences,"all_facts":memory,"max_iterations":max_iterations,"semantics":"semantica.textual-forward-chain.v1","complete":true}),
    )
}
