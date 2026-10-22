//! Explorer read projection from the existing session. This candidate does not
//! expose HTTP or claim RDFLib parser, deadline, or complete term-domain parity.
use crate::dataset::DatasetSession;
use anyhow::{Context, Result, ensure};
use oxigraph::{
    model::{Dataset, GraphName, Literal, NamedNode, Quad, Term},
    sparql::{QueryResults, SparqlEvaluator},
};
use serde_json::{Value, json};
const ENTITY: &str = "http://semantica.local/entity/";
const PROPERTY: &str = "http://semantica.local/prop/";
const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";

fn literal(value: &Value) -> Result<Literal> {
    let lexical = semantica_json::python_str(value)?;
    let datatype = match value {
        Value::Bool(_) => Some("boolean"),
        // Arbitrary-precision JSON integers are not bounded by native i64/u64.
        Value::Number(n) if !n.to_string().contains(['.', 'e', 'E']) => Some("integer"),
        Value::Number(_) => Some("double"),
        _ => None,
    };
    Ok(match datatype {
        Some("boolean") => Literal::from(value.as_bool().unwrap()),
        Some(kind) => Literal::new_typed_literal(
            lexical,
            NamedNode::new(format!("http://www.w3.org/2001/XMLSchema#{kind}"))?,
        ),
        None => Literal::new_simple_literal(lexical),
    })
}

/// A disposable default-graph view, never published into the session. The
/// caller keeps revision/source checks and execution/resource authority.
pub fn project(session: &DatasetSession) -> Result<Dataset> {
    let records = crate::context::records(session)?;
    ensure!(
        records.nodes.len() <= 50_000 && records.edges.len() <= 50_000,
        "explorer_sparql_graph_capacity"
    );
    let mut out = Dataset::new();
    let mut insert = |subject: NamedNode, predicate: NamedNode, object: Term| {
        out.insert(&Quad::new(
            subject,
            predicate,
            object,
            GraphName::DefaultGraph,
        ));
    };
    for node in records.nodes {
        let subject = NamedNode::new(format!(
            "{ENTITY}{}",
            node["id"].as_str().context("explorer_node_id")?
        ))?;
        let kind = node.get("type").and_then(Value::as_str).unwrap_or("Entity");
        insert(
            subject.clone(),
            NamedNode::new(TYPE)?,
            NamedNode::new(format!("{ENTITY}{kind}"))?.into(),
        );
        if let Some(content) = node
            .get("content")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            insert(
                subject.clone(),
                NamedNode::new(LABEL)?,
                Literal::new_simple_literal(content).into(),
            );
        }
        if let Some(properties) = node.get("properties") {
            for (key, value) in properties.as_object().context("explorer_node_properties")? {
                if ["content", "valid_from", "valid_until"].contains(&key.as_str()) {
                    continue;
                }
                insert(
                    subject.clone(),
                    NamedNode::new(format!("{PROPERTY}{key}"))?,
                    literal(value)?.into(),
                );
            }
        }
    }
    for edge in records.edges {
        let source = edge["source"].as_str().context("explorer_edge_source")?;
        let target = edge["target"].as_str().context("explorer_edge_target")?;
        let kind = edge
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("relatedTo");
        insert(
            NamedNode::new(format!("{ENTITY}{source}"))?,
            NamedNode::new(format!("{PROPERTY}{kind}"))?,
            NamedNode::new(format!("{ENTITY}{target}"))?.into(),
        );
    }
    Ok(out)
}
fn lexical(term: &Term) -> String {
    match term {
        Term::NamedNode(n) => n.as_str().into(),
        Term::BlankNode(n) => n.as_str().into(),
        Term::Literal(l) => l.value().into(),
    }
}
/// Bounded result materialization for offline migration comparison only.
/// No wall-time cancellation is claimed; the service must supply that owner
/// before this algorithm can be exposed as the original HTTP route.
pub fn query_projection(session: &DatasetSession, query: &str, row_limit: usize) -> Result<Value> {
    ensure!(
        query.chars().count() <= 10_000,
        "explorer_sparql_query_length"
    );
    ensure!(row_limit <= 5_000, "explorer_sparql_row_limit");
    ensure!(
        read_only_query(query)?,
        "Only SELECT, ASK, CONSTRUCT, and DESCRIBE queries are permitted."
    );
    crate::graph::local_query(query)?;
    let dataset = project(session)?;
    std::thread::scope(|scope| {
        std::thread::Builder::new().name("explorer-sparql-comparison".into())
            .stack_size(crate::graph::QUERY_STACK_BYTES).spawn_scoped(scope, || -> Result<Value> {
                let result = SparqlEvaluator::new().parse_query(query)?.on_queryable_dataset(&dataset).execute()?;
                let mut output = Vec::new(); let mut truncated = false;
                let columns: Vec<String> = match result {
                    QueryResults::Boolean(value) => {
                        output.push(json!({"result":if value {"true"} else {"false"}}));
                        vec!["result".into()]
                    }
                    QueryResults::Solutions(rows) => {
                        let columns = rows.variables().iter().map(|v| v.as_str().to_owned()).collect::<Vec<_>>();
                        for row in rows {
                            if output.len() >= row_limit { truncated = true; break; }
                            let row = row?;
                            output.push(Value::Object(columns.iter().map(|key| (key.clone(), row.get(key.as_str()).map(|v| Value::String(lexical(v))).unwrap_or(Value::Null))).collect()));
                        }
                        columns
                    }
                    QueryResults::Graph(rows) => {
                        for row in rows {
                            if output.len() >= row_limit { truncated = true; break; }
                            let triple = row?;
                            output.push(json!({"subject": match triple.subject { oxigraph::model::NamedOrBlankNode::NamedNode(n)=>n.as_str().to_owned(), oxigraph::model::NamedOrBlankNode::BlankNode(n)=>n.as_str().to_owned() },"predicate":triple.predicate.as_str(),"object":lexical(&triple.object)}));
                        }
                        vec!["subject".into(),"predicate".into(),"object".into()]
                    }
                };
                Ok(json!({"columns":columns,"total":output.len(),"rows":output,"truncated":truncated,"error":null,"error_line":null,"error_column":null}))
            })?.join().map_err(|_| anyhow::anyhow!("explorer_query_panicked"))?
    })
}
/// Preserve the original lexical admission before touching the graph. This is
/// intentionally stricter than parsing SPARQL alone (including words in literals).
pub fn read_only_query(query: &str) -> Result<bool> {
    let mut cleaned = String::new();
    let mut previous = None;
    let mut comment = false;
    for c in query.chars() {
        if comment {
            if c == '\n' {
                comment = false;
                cleaned.push(c);
            }
            previous = Some(c);
            continue;
        }
        let whitespace =
            previous.is_none_or(|p: char| semantica_json::python_strip(&p.to_string()).is_empty());
        if c == '#' && whitespace {
            comment = true;
            previous = Some(c);
            continue;
        }
        cleaned.push(c);
        previous = Some(c);
    }
    let prefix = regex::Regex::new(
        r"(?m)^[ \t]*(?:[pP][rR][eE][fF][iIİı][xX][ \t]+[^\s\x1c-\x1f]+|[bB][aA][sSſ][eE])[ \t]*<[^>\r\n]*>[ \t]*",
    )?;
    let cleaned = prefix.replace_all(&cleaned, "");
    let cleaned = semantica_json::python_strip(&cleaned);
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let first = cleaned.chars().take_while(|c| word(*c)).collect::<String>();
    if !["SELECT", "ASK", "CONSTRUCT", "DESCRIBE"]
        .iter()
        .any(|s| python_keyword(&first, s))
    {
        return Ok(false);
    }
    Ok(!cleaned.split(|c| !word(c)).any(|token| {
        [
            "INSERT", "DELETE", "DROP", "LOAD", "CLEAR", "CREATE", "COPY", "MOVE", "ADD",
        ]
        .iter()
        .any(|v| python_keyword(token, v))
    }))
}

fn python_keyword(value: &str, expected: &str) -> bool {
    value
        .chars()
        .map(|c| match c {
            'İ' | 'ı' => 'i',
            'ſ' => 's',
            'K' => 'k',
            _ => c.to_ascii_lowercase(),
        })
        .eq(expected.chars().map(|c| c.to_ascii_lowercase()))
}

/// Preserve native parser diagnostics in the Explorer response envelope. Only a
/// typed syntax failure is a query response; graph/source/policy/process failures
/// remain errors. Native syntax messages do not claim RDFLib text compatibility.
pub fn query_projection_response(
    session: &DatasetSession,
    query: &str,
    row_limit: usize,
) -> Result<Value> {
    match query_projection(session, query, row_limit) {
        Ok(result) => Ok(result),
        Err(error) => {
            let Some(syntax) = error.downcast_ref::<oxigraph::sparql::SparqlSyntaxError>() else {
                return Err(error);
            };
            let message = syntax.to_string();
            // Locked peg-runtime Display: "error at LINE:COLUMN: expected ...".
            // Other typed syntax variants have no position and retain nulls.
            let position = regex::Regex::new(r"^error at ([0-9]+):([0-9]+):")?;
            let captures = position.captures(&message);
            let line = captures.as_ref().and_then(|c| c[1].parse::<u64>().ok());
            let column = captures.as_ref().and_then(|c| c[2].parse::<u64>().ok());
            Ok(
                json!({"columns":[],"rows":[],"total":0,"truncated":false,"error":message,"error_line":line,"error_column":column}),
            )
        }
    }
}
