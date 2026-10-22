//! RDFLib760 term.Literal._literal_n3 / _quote_encode (BSD-3-Clause).
//! Exact source spelling and cached-value admission are supplied by the owner.
use super::source_turtle_read::TurtleTerm;
use anyhow::Result;
use spareval::SourceTermInput;
const X: &str = "http://www.w3.org/2001/XMLSchema#";

fn quote(value: &str) -> String {
    let mut text = value.replace('\\', "\\\\");
    if value.contains('\n') {
        text = text.replace("\"\"\"", "\\\"\\\"\\\"");
        if text.ends_with('"') && !text.ends_with("\\\"") {
            text.pop();
            text.push_str("\\\"");
        }
        format!("\"\"\"{}\"\"\"", text.replace('\r', "\\r"))
    } else {
        format!("\"{}\"", text.replace('"', "\\\"").replace('\r', "\\r"))
    }
}
pub(super) fn literal_n3<T: TurtleTerm>(
    term: &T,
    cached: impl FnOnce() -> Result<bool>,
    mut pname: impl FnMut(&str) -> Result<Option<String>>,
) -> Result<String> {
    let SourceTermInput::Literal {
        value,
        datatype,
        language,
    } = term.term()?
    else {
        anyhow::bail!("source_turtle_literal_expected");
    };
    let local = datatype.and_then(|v| v.strip_prefix(X));
    let special = matches!(local, Some("float" | "double" | "decimal"));
    let plain = matches!(local, Some("integer" | "boolean" | "double" | "decimal"))
        || datatype == Some("http://www.w3.org/2002/07/owl#rational");
    // Original float(self) parses the lexical str subclass, not its .value.
    let float =
        || semantica_json::python_builtin_float(&serde_json::Value::String(value.to_owned()));
    if plain && cached()? && (!special || float().is_ok_and(f64::is_finite)) {
        return Ok(match local {
            Some("double") => {
                // CPython's default 'e' precision is six fractional places.
                let number =
                    float().map_err(|e| anyhow::anyhow!("source_turtle_float_lexical:{e:?}"))?;
                let formatted = format!("{number:.6e}");
                let (mantissa, exp) = formatted.split_once('e').expect("finite scientific float");
                let exponent: i32 = exp.parse()?;
                format!(
                    "{}e{exponent:+03}",
                    mantissa.trim_end_matches('0').trim_end_matches('.')
                )
            }
            Some("decimal") if !value.contains(['.', 'e', 'E']) => format!("{value}.0"),
            Some("boolean") => value.to_lowercase(),
            _ => value.to_owned(),
        });
    }
    let mut encoded = quote(value);
    let dt = datatype
        .map(|dt| Ok::<_, anyhow::Error>(pname(dt)?.unwrap_or_else(|| format!("<{dt}>"))))
        .transpose()?;
    if special && let Ok(number) = float() {
        if number.is_infinite() {
            encoded = encoded.replace("inf", "INF").replace("Infinity", "INF");
        }
        if number.is_nan() {
            encoded = encoded.replace("nan", "NaN");
        }
    }
    if let Some(lang) = language.filter(|v| !v.is_empty()) {
        Ok(format!("{encoded}@{lang}"))
    } else if datatype.is_some_and(|v| !v.is_empty()) {
        Ok(format!("{encoded}^^{}", dt.unwrap()))
    } else {
        Ok(encoded)
    }
}
