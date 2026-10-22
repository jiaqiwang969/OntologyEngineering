//! Python's `csv` module as CPython 3.11 implements it: the `_csv` reader state machine
//! (excel dialect with a configurable delimiter), `csv.writer`/`DictWriter` row output with
//! `QUOTE_MINIMAL`, and `csv.Sniffer.sniff` delimiter detection.

use std::fmt;

use serde_json::Value;

use sem_utils::SemanticaError;
use sem_utils::pycompat::fmt::py_str;

/// `_csv.Error`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CsvError(pub String);

impl fmt::Display for CsvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<CsvError> for SemanticaError {
    fn from(e: CsvError) -> Self {
        SemanticaError::Other {
            module: "_csv".to_string(),
            type_name: "Error".to_string(),
            message: e.0,
        }
    }
}

/// `csv.field_size_limit()` default.
const FIELD_SIZE_LIMIT: usize = 131_072;

/// The dialect parameters the parse module varies (everything else is `excel`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Dialect {
    pub delimiter: char,
    pub quotechar: Option<char>,
    pub escapechar: Option<char>,
    pub doublequote: bool,
    pub skipinitialspace: bool,
    pub strict: bool,
    pub lineterminator: String,
}

impl Default for Dialect {
    fn default() -> Self {
        Dialect {
            delimiter: ',',
            quotechar: Some('"'),
            escapechar: None,
            doublequote: true,
            skipinitialspace: false,
            strict: false,
            lineterminator: "\r\n".to_string(),
        }
    }
}

impl Dialect {
    /// The excel dialect with `delimiter=`; like `_csv`, a delimiter must be a single
    /// character (`TypeError: "delimiter" must be a 1-character string`).
    pub fn with_delimiter(delimiter: &str) -> Result<Dialect, SemanticaError> {
        let mut chars = delimiter.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) => Ok(Dialect {
                delimiter: c,
                ..Dialect::default()
            }),
            _ => Err(SemanticaError::Type(
                "\"delimiter\" must be a 1-character string".to_string(),
            )),
        }
    }
}

/// Split text into lines the way a file opened with `newline=""` iterates: each line keeps
/// its terminator (`\n`, `\r` or `\r\n`).
pub(crate) fn split_lines_keep_ends(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        current.push(c);
        if c == '\n' {
            lines.push(std::mem::take(&mut current));
        } else if c == '\r' {
            if chars.peek() == Some(&'\n') {
                current.push('\n');
                chars.next();
            }
            lines.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    StartRecord,
    StartField,
    EscapedChar,
    InField,
    InQuotedField,
    EscapeInQuotedField,
    QuoteInQuotedField,
    EatCrnl,
    AfterEscapedCrnl,
}

/// `csv.reader(f, dialect)` over the lines of a file.
pub(crate) struct Reader {
    lines: Vec<String>,
    pos: usize,
    dialect: Dialect,
    state: State,
    field: String,
    field_len: usize,
    fields: Vec<String>,
    /// `reader.line_num`: lines read so far.
    pub line_num: usize,
}

/// The end-of-line marker the C reader feeds after each line.
const EOL: Option<char> = None;

impl Reader {
    pub fn new(text: &str, dialect: Dialect) -> Reader {
        Reader {
            lines: split_lines_keep_ends(text),
            pos: 0,
            dialect,
            state: State::StartRecord,
            field: String::new(),
            field_len: 0,
            fields: Vec::new(),
            line_num: 0,
        }
    }

    fn save_field(&mut self) {
        self.fields.push(std::mem::take(&mut self.field));
        self.field_len = 0;
    }

    fn add_char(&mut self, c: char) -> Result<(), CsvError> {
        if self.field_len >= FIELD_SIZE_LIMIT {
            return Err(CsvError(format!(
                "field larger than field limit ({FIELD_SIZE_LIMIT})"
            )));
        }
        self.field.push(c);
        self.field_len += 1;
        Ok(())
    }

    fn process(&mut self, c: Option<char>) -> Result<(), CsvError> {
        let d = self.dialect.clone();
        let is_nl = |c: Option<char>| matches!(c, Some('\n') | Some('\r'));
        let mut state = self.state;
        loop {
            match state {
                State::StartRecord => {
                    if c == EOL {
                        return Ok(());
                    }
                    if is_nl(c) {
                        self.state = State::EatCrnl;
                        return Ok(());
                    }
                    self.state = State::StartField;
                    state = State::StartField;
                    continue;
                }
                State::StartField => {
                    if is_nl(c) || c == EOL {
                        self.save_field();
                        self.state = if c == EOL {
                            State::StartRecord
                        } else {
                            State::EatCrnl
                        };
                    } else if c.is_some() && c == d.quotechar {
                        self.state = State::InQuotedField;
                    } else if c.is_some() && c == d.escapechar {
                        self.state = State::EscapedChar;
                    } else if c == Some(' ') && d.skipinitialspace {
                    } else if c == Some(d.delimiter) {
                        self.save_field();
                    } else {
                        self.add_char(c.expect("not EOL"))?;
                        self.state = State::InField;
                    }
                    return Ok(());
                }
                State::EscapedChar => {
                    if is_nl(c) {
                        self.add_char(c.expect("newline"))?;
                        self.state = State::AfterEscapedCrnl;
                        return Ok(());
                    }
                    self.add_char(c.unwrap_or('\n'))?;
                    self.state = State::InField;
                    return Ok(());
                }
                State::AfterEscapedCrnl => {
                    if c == EOL {
                        return Ok(());
                    }
                    state = State::InField;
                    continue;
                }
                State::InField => {
                    if is_nl(c) || c == EOL {
                        self.save_field();
                        self.state = if c == EOL {
                            State::StartRecord
                        } else {
                            State::EatCrnl
                        };
                    } else if c.is_some() && c == d.escapechar {
                        self.state = State::EscapedChar;
                    } else if c == Some(d.delimiter) {
                        self.save_field();
                        self.state = State::StartField;
                    } else {
                        self.add_char(c.expect("not EOL"))?;
                        self.state = State::InField;
                    }
                    return Ok(());
                }
                State::InQuotedField => {
                    if c == EOL {
                    } else if c.is_some() && c == d.escapechar {
                        self.state = State::EscapeInQuotedField;
                    } else if c.is_some() && c == d.quotechar {
                        self.state = if d.doublequote {
                            State::QuoteInQuotedField
                        } else {
                            State::InField
                        };
                    } else {
                        self.add_char(c.expect("not EOL"))?;
                    }
                    return Ok(());
                }
                State::EscapeInQuotedField => {
                    self.add_char(c.unwrap_or('\n'))?;
                    self.state = State::InQuotedField;
                    return Ok(());
                }
                State::QuoteInQuotedField => {
                    if c.is_some() && c == d.quotechar {
                        self.add_char(c.expect("quote"))?;
                        self.state = State::InQuotedField;
                    } else if c == Some(d.delimiter) {
                        self.save_field();
                        self.state = State::StartField;
                    } else if is_nl(c) || c == EOL {
                        self.save_field();
                        self.state = if c == EOL {
                            State::StartRecord
                        } else {
                            State::EatCrnl
                        };
                    } else if !d.strict {
                        self.add_char(c.expect("not EOL"))?;
                        self.state = State::InField;
                    } else {
                        return Err(CsvError(format!(
                            "'{}' expected after '{}'",
                            d.delimiter,
                            d.quotechar.unwrap_or('"')
                        )));
                    }
                    return Ok(());
                }
                State::EatCrnl => {
                    if is_nl(c) {
                    } else if c == EOL {
                        self.state = State::StartRecord;
                    } else {
                        return Err(CsvError(
                            "new-line character seen in unquoted field - do you need to open the file with newline=''?"
                                .to_string(),
                        ));
                    }
                    return Ok(());
                }
            }
        }
    }

    /// `next(reader)`: the next record, `None` at the end of the input.
    pub fn next_record(&mut self) -> Option<Result<Vec<String>, CsvError>> {
        self.fields.clear();
        self.field.clear();
        self.field_len = 0;
        self.state = State::StartRecord;
        loop {
            if self.pos >= self.lines.len() {
                if !self.field.is_empty() || self.state == State::InQuotedField {
                    if self.dialect.strict {
                        return Some(Err(CsvError("unexpected end of data".to_string())));
                    }
                    self.save_field();
                    return Some(Ok(std::mem::take(&mut self.fields)));
                }
                return None;
            }
            let line = self.lines[self.pos].clone();
            self.pos += 1;
            self.line_num += 1;
            for c in line.chars() {
                if let Err(e) = self.process(Some(c)) {
                    return Some(Err(e));
                }
            }
            if let Err(e) = self.process(EOL) {
                return Some(Err(e));
            }
            if self.state == State::StartRecord {
                return Some(Ok(std::mem::take(&mut self.fields)));
            }
        }
    }
}

/// `str(field)` as the csv writer converts it (`None` is the empty string).
fn writer_field(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => py_str(other),
    }
}

/// One row as `csv.writer(..., quoting=QUOTE_MINIMAL)` writes it (with `\r\n`).
pub(crate) fn write_row(fields: &[Value], dialect: &Dialect) -> String {
    let quote = dialect.quotechar.unwrap_or('"');
    let mut out = String::new();
    if fields.len() == 1 && writer_field(&fields[0]).is_empty() {
        // A single empty field record is written quoted.
        out.push(quote);
        out.push(quote);
        out.push_str(&dialect.lineterminator);
        return out;
    }
    for (i, field) in fields.iter().enumerate() {
        if i > 0 {
            out.push(dialect.delimiter);
        }
        let text = writer_field(field);
        let needs_quote = text.chars().any(|c| {
            c == dialect.delimiter
                || Some(c) == dialect.escapechar
                || c == quote
                || dialect.lineterminator.contains(c)
        });
        if needs_quote {
            out.push(quote);
            for c in text.chars() {
                if c == quote {
                    out.push(quote);
                }
                out.push(c);
            }
            out.push(quote);
        } else {
            out.push_str(&text);
        }
    }
    out.push_str(&dialect.lineterminator);
    out
}

/// `csv.writer(f).writerow(row)` for an arbitrary value: lists are rows, strings iterate
/// their characters, other values raise `_csv.Error: iterable expected, not <type>`.
pub(crate) fn write_value_row(row: &Value, dialect: &Dialect) -> Result<String, CsvError> {
    match row {
        Value::Array(items) => Ok(write_row(items, dialect)),
        Value::String(s) => {
            let chars: Vec<Value> = s.chars().map(|c| Value::String(c.to_string())).collect();
            Ok(write_row(&chars, dialect))
        }
        Value::Object(map) => {
            let keys: Vec<Value> = map.keys().map(|k| Value::String(k.clone())).collect();
            Ok(write_row(&keys, dialect))
        }
        other => Err(CsvError(format!(
            "iterable expected, not {}",
            sem_utils::pycompat::fmt::type_name(other)
        ))),
    }
}

// --------------------------------------------------------------------------- Sniffer

/// `csv.Sniffer().sniff(sample).delimiter` (`_csv.Error("Could not determine delimiter")`
/// when nothing qualifies).
pub(crate) fn sniff_delimiter(sample: &str) -> Result<String, CsvError> {
    let (_quote, _dq, delimiter, _skip) = guess_quote_and_delimiter(sample);
    let delimiter = match delimiter {
        Some(d) if !d.is_empty() => d,
        _ => guess_delimiter(sample).0,
    };
    if delimiter.is_empty() {
        return Err(CsvError("Could not determine delimiter".to_string()));
    }
    Ok(delimiter)
}

/// `_guess_quote_and_delimiter(data, None)` -> (quotechar, doublequote, delimiter,
/// skipinitialspace); `delimiter` is `None` when no quoted field was found.
fn guess_quote_and_delimiter(data: &str) -> (String, bool, Option<String>, bool) {
    // Python's patterns, with `(?P=name)` written as `\k<name>` for fancy-regex.
    let patterns = [
        r#"(?sm)(?P<delim>[^\w\n"'])(?P<space> ?)(?P<quote>["']).*?\k<quote>\k<delim>"#,
        r#"(?sm)(?:^|\n)(?P<quote>["']).*?\k<quote>(?P<delim>[^\w\n"'])(?P<space> ?)"#,
        r#"(?sm)(?P<delim>[^\w\n"'])(?P<space> ?)(?P<quote>["']).*?\k<quote>(?:$|\n)"#,
        r#"(?sm)(?:^|\n)(?P<quote>["']).*?\k<quote>(?:$|\n)"#,
    ];
    let mut matches: Vec<(String, Option<String>, Option<String>)> = Vec::new();
    for pat in patterns {
        let re = fancy_regex::Regex::new(pat).expect("valid sniffer regex");
        let has_delim = pat.contains("<delim>[");
        let has_space = pat.contains("<space>");
        let mut found = Vec::new();
        let mut start = 0;
        while start <= data.len() {
            let Ok(Some(caps)) = re.captures_from_pos(data, start) else {
                break;
            };
            let whole = caps.get(0).expect("match");
            let get = |name: &str| {
                caps.name(name)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default()
            };
            found.push((
                get("quote"),
                has_delim.then(|| get("delim")),
                has_space.then(|| get("space")),
            ));
            start = if whole.end() > whole.start() {
                whole.end()
            } else {
                // Empty match: advance one character.
                whole.end() + data[whole.end()..].chars().next().map_or(1, char::len_utf8)
            };
        }
        if !found.is_empty() {
            matches = found;
            break;
        }
    }
    if matches.is_empty() {
        return (String::new(), false, None, false);
    }
    let mut quotes: indexmap::IndexMap<String, usize> = indexmap::IndexMap::new();
    let mut delims: indexmap::IndexMap<String, usize> = indexmap::IndexMap::new();
    let mut spaces = 0;
    for (quote, delim, space) in &matches {
        if !quote.is_empty() {
            *quotes.entry(quote.clone()).or_insert(0) += 1;
        }
        let Some(delim) = delim else { continue };
        if !delim.is_empty() {
            *delims.entry(delim.clone()).or_insert(0) += 1;
        }
        let Some(space) = space else { continue };
        if !space.is_empty() {
            spaces += 1;
        }
    }
    let quotechar = first_max(&quotes).unwrap_or_default();
    let (delim, skipinitialspace) = if !delims.is_empty() {
        let delim = first_max(&delims).unwrap_or_default();
        let skip = delims[&delim] == spaces;
        (if delim == "\n" { String::new() } else { delim }, skip)
    } else {
        (String::new(), false)
    };
    let esc = regex::escape(&delim);
    let q = regex::escape(&quotechar);
    let dq = format!(r"(?m)(({esc})|^)\W*{q}[^{esc}\n]*{q}[^{esc}\n]*{q}\W*(({esc})|$)");
    let doublequote = regex::Regex::new(&dq)
        .map(|re| re.is_match(data))
        .unwrap_or(false);
    (quotechar, doublequote, Some(delim), skipinitialspace)
}

/// `max(d, key=d.get)`: the first key with the largest value.
fn first_max(d: &indexmap::IndexMap<String, usize>) -> Option<String> {
    let mut best: Option<(&String, usize)> = None;
    for (k, &v) in d {
        if best.is_none_or(|(_, b)| v > b) {
            best = Some((k, v));
        }
    }
    best.map(|(k, _)| k.clone())
}

/// `_guess_delimiter(data, None)` -> (delimiter, skipinitialspace).
fn guess_delimiter(data: &str) -> (String, bool) {
    let lines: Vec<&str> = data.split('\n').filter(|l| !l.is_empty()).collect();
    let ascii: Vec<char> = (0u8..127).map(char::from).collect();
    let chunk_length = lines.len().min(10);
    let mut iteration = 0usize;
    // char -> (frequency -> count), both in first-insertion order.
    let mut char_frequency: indexmap::IndexMap<char, indexmap::IndexMap<usize, usize>> =
        indexmap::IndexMap::new();
    let mut modes: indexmap::IndexMap<char, (usize, i64)> = indexmap::IndexMap::new();
    let mut delims: indexmap::IndexMap<char, (usize, i64)> = indexmap::IndexMap::new();
    let (mut start, mut end) = (0usize, chunk_length);
    let skip_for = |d: char| {
        let first = lines.first().copied().unwrap_or("");
        first.matches(d).count() == first.matches(&format!("{d} ")).count()
    };
    while start < lines.len() {
        iteration += 1;
        for line in &lines[start..end.min(lines.len())] {
            for &ch in &ascii {
                let freq = line.matches(ch).count();
                *char_frequency
                    .entry(ch)
                    .or_default()
                    .entry(freq)
                    .or_insert(0) += 1;
            }
        }
        for (&ch, meta) in &char_frequency {
            let items: Vec<(usize, usize)> = meta.iter().map(|(&f, &c)| (f, c)).collect();
            if items.len() == 1 && items[0].0 == 0 {
                continue;
            }
            if items.len() > 1 {
                let mut best = items[0];
                for &it in &items[1..] {
                    if it.1 > best.1 {
                        best = it;
                    }
                }
                let pos = items.iter().position(|&it| it == best).expect("present");
                let rest: usize = items
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != pos)
                    .map(|(_, it)| it.1)
                    .sum();
                modes.insert(ch, (best.0, best.1 as i64 - rest as i64));
            } else {
                modes.insert(ch, (items[0].0, items[0].1 as i64));
            }
        }
        let total = (chunk_length * iteration).min(lines.len()) as f64;
        let mut consistency = 1.0f64;
        let threshold = 0.9;
        while delims.is_empty() && consistency >= threshold {
            for (&k, &v) in &modes {
                if v.0 > 0 && v.1 > 0 && (v.1 as f64 / total) >= consistency {
                    delims.insert(k, v);
                }
            }
            consistency -= 0.01;
        }
        if delims.len() == 1 {
            let delim = *delims.keys().next().expect("one");
            return (delim.to_string(), skip_for(delim));
        }
        start = end;
        end += chunk_length;
    }
    if delims.is_empty() {
        return (String::new(), false);
    }
    if delims.len() > 1 {
        for d in [',', '\t', ';', ' ', ':'] {
            if delims.contains_key(&d) {
                return (d.to_string(), skip_for(d));
            }
        }
    }
    let mut items: Vec<((usize, i64), char)> = delims.iter().map(|(&k, &v)| (v, k)).collect();
    items.sort();
    let delim = items.last().expect("non-empty").1;
    (delim.to_string(), skip_for(delim))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn records(text: &str) -> Vec<Vec<String>> {
        let mut r = Reader::new(text, Dialect::default());
        let mut out = Vec::new();
        while let Some(rec) = r.next_record() {
            out.push(rec.unwrap());
        }
        out
    }

    #[test]
    fn reader_matches_csv_module() {
        assert_eq!(
            records("a,b\r\n\"x,1\",\"he said \"\"hi\"\"\"\n\nlast"),
            vec![
                vec!["a", "b"],
                vec!["x,1", "he said \"hi\""],
                vec![],
                vec!["last"]
            ]
        );
        assert_eq!(
            records("\"multi\nline\",2\n"),
            vec![vec!["multi\nline", "2"]]
        );
        assert_eq!(records("a\"b\",c\n"), vec![vec!["a\"b\"", "c"]]);
        assert_eq!(records("\"ab\"cd,e"), vec![vec!["abcd", "e"]]);
        assert_eq!(records("x,\n"), vec![vec!["x", ""]]);
        assert_eq!(records("\"open"), vec![vec!["open"]]);
    }

    #[test]
    fn writer_quotes_minimally() {
        let d = Dialect::default();
        assert_eq!(
            write_row(&[json!("a"), json!("b,c"), json!("q\"x")], &d),
            "a,\"b,c\",\"q\"\"x\"\r\n"
        );
        assert_eq!(write_row(&[json!("")], &d), "\"\"\r\n");
        assert_eq!(
            write_row(&[json!(1), json!(2.5), json!(null), json!(true)], &d),
            "1,2.5,,True\r\n"
        );
    }

    #[test]
    fn sniffer_finds_delimiters() {
        assert_eq!(sniff_delimiter("a;b;c\n1;2;3\n4;5;6\n").unwrap(), ";");
        assert_eq!(sniff_delimiter("a\tb\n1\t2\n").unwrap(), "\t");
        assert_eq!(sniff_delimiter("'a','b'\n'c','d'\n").unwrap(), ",");
        // Expected values from CPython 3.11's csv.Sniffer.
        assert_eq!(sniff_delimiter("abc\n").unwrap(), "c");
        assert_eq!(sniff_delimiter("x\n").unwrap(), "x");
        assert_eq!(sniff_delimiter("name age\nbob 3\n").unwrap(), " ");
        assert_eq!(
            sniff_delimiter("a,b\nc\n").unwrap_err().to_string(),
            "Could not determine delimiter"
        );
    }
}
