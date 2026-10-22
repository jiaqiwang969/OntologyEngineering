//! Python's `csv` module: the C reader state machine (`_csv.c`, dialect options included),
//! `csv.Sniffer` and `csv.DictReader`.

use fancy_regex::Regex as PyRegex;
use indexmap::IndexMap;
use serde_json::Value;

/// `csv.QUOTE_*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quoting {
    /// `QUOTE_MINIMAL`.
    Minimal,
    /// `QUOTE_ALL`.
    All,
    /// `QUOTE_NONNUMERIC`.
    NonNumeric,
    /// `QUOTE_NONE`.
    None,
}

/// A `csv.Dialect`.
#[derive(Debug, Clone, PartialEq)]
pub struct Dialect {
    /// `delimiter`.
    pub delimiter: char,
    /// `quotechar`.
    pub quotechar: Option<char>,
    /// `escapechar`.
    pub escapechar: Option<char>,
    /// `doublequote`.
    pub doublequote: bool,
    /// `skipinitialspace`.
    pub skipinitialspace: bool,
    /// `quoting`.
    pub quoting: Quoting,
    /// `strict`.
    pub strict: bool,
}

impl Default for Dialect {
    /// `csv.excel`.
    fn default() -> Self {
        Dialect {
            delimiter: ',',
            quotechar: Some('"'),
            escapechar: None,
            doublequote: true,
            skipinitialspace: false,
            quoting: Quoting::Minimal,
            strict: false,
        }
    }
}

/// `_csv.Error`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsvError {
    /// The message.
    pub message: String,
    /// `reader.line_num` when it was raised.
    pub line_num: usize,
}

impl std::fmt::Display for CsvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
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

/// Splits text into lines the way iterating a text stream does (keeping `\n` endings).
pub fn split_lines_keepends(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut start = 0;
    for (i, c) in text.char_indices() {
        if c == '\n' {
            lines.push(text[start..=i].to_string());
            start = i + 1;
        }
    }
    if start < text.len() {
        lines.push(text[start..].to_string());
    }
    lines
}

/// A `csv.reader` over a list of input lines.
pub struct Reader {
    lines: Vec<String>,
    next_line: usize,
    dialect: Dialect,
    state: State,
    fields: Vec<String>,
    field: String,
    field_started: bool,
    /// `reader.line_num`: lines read so far.
    pub line_num: usize,
    done: bool,
}

impl Reader {
    /// `csv.reader(lines, dialect)`.
    pub fn new(lines: Vec<String>, dialect: Dialect) -> Reader {
        Reader {
            lines,
            next_line: 0,
            dialect,
            state: State::StartRecord,
            fields: Vec::new(),
            field: String::new(),
            field_started: false,
            line_num: 0,
            done: false,
        }
    }

    /// `csv.reader(io.StringIO(text), dialect)`.
    pub fn from_text(text: &str, dialect: Dialect) -> Reader {
        Reader::new(split_lines_keepends(text), dialect)
    }

    fn save_field(&mut self) {
        self.fields.push(std::mem::take(&mut self.field));
        self.field_started = false;
    }

    fn add_char(&mut self, c: char) {
        self.field.push(c);
        self.field_started = true;
    }

    fn error(&self, message: &str) -> CsvError {
        CsvError {
            message: message.to_string(),
            line_num: self.line_num,
        }
    }

    /// Feeds one character (`None` = end of line).
    fn process(&mut self, c: Option<char>) -> Result<(), CsvError> {
        let d = self.dialect.clone();
        let is_quote = |c: char| d.quoting != Quoting::None && Some(c) == d.quotechar;
        let is_escape = |c: char| Some(c) == d.escapechar;
        let mut state = self.state;
        loop {
            match state {
                State::StartRecord => {
                    match c {
                        None => {
                            self.state = State::StartRecord;
                            return Ok(());
                        }
                        Some('\n' | '\r') => {
                            self.state = State::EatCrnl;
                            return Ok(());
                        }
                        _ => {
                            state = State::StartField;
                            continue;
                        }
                    }
                }
                State::StartField => {
                    match c {
                        None | Some('\n' | '\r') => {
                            self.save_field();
                            self.state = if c.is_none() {
                                State::StartRecord
                            } else {
                                State::EatCrnl
                            };
                        }
                        Some(ch) if is_quote(ch) => self.state = State::InQuotedField,
                        Some(ch) if is_escape(ch) => self.state = State::EscapedChar,
                        Some(' ') if d.skipinitialspace => self.state = State::StartField,
                        Some(ch) if ch == d.delimiter => {
                            self.save_field();
                            self.state = State::StartField;
                        }
                        Some(ch) => {
                            self.add_char(ch);
                            self.state = State::InField;
                        }
                    }
                    return Ok(());
                }
                State::EscapedChar => {
                    match c {
                        Some(ch @ ('\n' | '\r')) => {
                            self.add_char(ch);
                            self.state = State::AfterEscapedCrnl;
                        }
                        None => {
                            self.add_char('\n');
                            self.state = State::InField;
                        }
                        Some(ch) => {
                            self.add_char(ch);
                            self.state = State::InField;
                        }
                    }
                    return Ok(());
                }
                State::AfterEscapedCrnl => {
                    if c.is_none() {
                        self.state = State::AfterEscapedCrnl;
                        return Ok(());
                    }
                    state = State::InField;
                    continue;
                }
                State::InField => {
                    match c {
                        None | Some('\n' | '\r') => {
                            self.save_field();
                            self.state = if c.is_none() {
                                State::StartRecord
                            } else {
                                State::EatCrnl
                            };
                        }
                        Some(ch) if is_escape(ch) => self.state = State::EscapedChar,
                        Some(ch) if ch == d.delimiter => {
                            self.save_field();
                            self.state = State::StartField;
                        }
                        Some(ch) => {
                            self.add_char(ch);
                            self.state = State::InField;
                        }
                    }
                    return Ok(());
                }
                State::InQuotedField => {
                    match c {
                        None => self.state = State::InQuotedField,
                        Some(ch) if is_escape(ch) => self.state = State::EscapeInQuotedField,
                        Some(ch) if is_quote(ch) => {
                            self.state = if d.doublequote {
                                State::QuoteInQuotedField
                            } else {
                                State::InField
                            };
                        }
                        Some(ch) => {
                            self.add_char(ch);
                            self.state = State::InQuotedField;
                        }
                    }
                    return Ok(());
                }
                State::EscapeInQuotedField => {
                    self.add_char(c.unwrap_or('\n'));
                    self.state = State::InQuotedField;
                    return Ok(());
                }
                State::QuoteInQuotedField => {
                    match c {
                        Some(ch) if is_quote(ch) => {
                            self.add_char(ch);
                            self.state = State::InQuotedField;
                        }
                        Some(ch) if ch == d.delimiter => {
                            self.save_field();
                            self.state = State::StartField;
                        }
                        None | Some('\n' | '\r') => {
                            self.save_field();
                            self.state = if c.is_none() {
                                State::StartRecord
                            } else {
                                State::EatCrnl
                            };
                        }
                        Some(ch) if !d.strict => {
                            self.add_char(ch);
                            self.state = State::InField;
                        }
                        Some(_) => {
                            return Err(self.error(&format!(
                                "'{}' expected after '{}'",
                                d.delimiter,
                                d.quotechar.unwrap_or('"')
                            )));
                        }
                    }
                    return Ok(());
                }
                State::EatCrnl => {
                    match c {
                        Some('\n' | '\r') => self.state = State::EatCrnl,
                        None => self.state = State::StartRecord,
                        Some(_) => {
                            return Err(self.error(
                                "new-line character seen in unquoted field - do you need to open the file with newline=''?",
                            ));
                        }
                    }
                    return Ok(());
                }
            }
        }
    }

    /// `next(reader)`: the next record, `None` at the end.
    pub fn next_record(&mut self) -> Option<Result<Vec<String>, CsvError>> {
        if self.done {
            return None;
        }
        self.fields.clear();
        self.field.clear();
        self.field_started = false;
        loop {
            if self.next_line >= self.lines.len() {
                self.done = true;
                if !self.field.is_empty() || self.state == State::InQuotedField {
                    if self.dialect.strict {
                        return Some(Err(self.error("unexpected end of data")));
                    }
                    self.save_field();
                    return Some(Ok(std::mem::take(&mut self.fields)));
                }
                return None;
            }
            let line = self.lines[self.next_line].clone();
            self.next_line += 1;
            self.line_num += 1;
            for c in line.chars() {
                if c == '\0' {
                    self.state = State::StartRecord;
                    return Some(Err(self.error("line contains NUL")));
                }
                if let Err(e) = self.process(Some(c)) {
                    self.state = State::StartRecord;
                    return Some(Err(e));
                }
            }
            if let Err(e) = self.process(None) {
                self.state = State::StartRecord;
                return Some(Err(e));
            }
            if self.state == State::StartRecord {
                return Some(Ok(std::mem::take(&mut self.fields)));
            }
        }
    }
}

impl Iterator for Reader {
    type Item = Result<Vec<String>, CsvError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_record()
    }
}

/// `list(csv.DictReader(io.StringIO(text)))` with the default dialect: values are strings,
/// missing trailing values are `None`, surplus values are collected under the key `null`
/// (Python's `None` restkey).
pub fn dict_reader(text: &str) -> Result<Vec<IndexMap<Option<String>, Value>>, CsvError> {
    let mut reader = Reader::from_text(text, Dialect::default());
    let mut fieldnames: Option<Vec<String>> = None;
    let mut rows = Vec::new();
    while let Some(record) = reader.next_record() {
        let record = record?;
        if fieldnames.is_none() {
            fieldnames = Some(record);
            continue;
        }
        if record.is_empty() {
            continue;
        }
        let names = fieldnames.as_ref().expect("fieldnames read");
        let mut row: IndexMap<Option<String>, Value> = IndexMap::new();
        for (i, name) in names.iter().enumerate() {
            row.insert(
                Some(name.clone()),
                record.get(i).map(|v| Value::String(v.clone())).unwrap_or(Value::Null),
            );
        }
        if record.len() > names.len() {
            row.insert(
                None,
                Value::Array(
                    record[names.len()..]
                        .iter()
                        .map(|v| Value::String(v.clone()))
                        .collect(),
                ),
            );
        }
        rows.push(row);
    }
    Ok(rows)
}

/// `csv.Sniffer().sniff(sample, delimiters)`.
pub fn sniff(sample: &str, delimiters: Option<&[char]>) -> Result<Dialect, CsvError> {
    let (quotechar, doublequote, delimiter, skipinitialspace) =
        guess_quote_and_delimiter(sample, delimiters);
    let (delimiter, skipinitialspace) = match delimiter {
        Some(d) => (Some(d), skipinitialspace),
        None => guess_delimiter(sample, delimiters),
    };
    let Some(delimiter) = delimiter else {
        return Err(CsvError {
            message: "Could not determine delimiter".to_string(),
            line_num: 0,
        });
    };
    Ok(Dialect {
        delimiter,
        quotechar: Some(quotechar.unwrap_or('"')),
        escapechar: None,
        doublequote,
        skipinitialspace,
        quoting: Quoting::Minimal,
        strict: false,
    })
}

fn guess_quote_and_delimiter(
    data: &str,
    delimiters: Option<&[char]>,
) -> (Option<char>, bool, Option<char>, bool) {
    let patterns = [
        r#"(?sm)(?P<delim>[^\w\n"'])(?P<space> ?)(?P<quote>["']).*?(?P=quote)(?P=delim)"#,
        r#"(?sm)(?:^|\n)(?P<quote>["']).*?(?P=quote)(?P<delim>[^\w\n"'])(?P<space> ?)"#,
        r#"(?sm)(?P<delim>[^\w\n"'])(?P<space> ?)(?P<quote>["']).*?(?P=quote)(?:$|\n)"#,
        r#"(?sm)(?:^|\n)(?P<quote>["']).*?(?P=quote)(?:$|\n)"#,
    ];
    let mut found: Vec<(Option<String>, Option<String>, Option<String>)> = Vec::new();
    let mut has_delim_group = false;
    let mut has_space_group = false;
    for (i, pattern) in patterns.iter().enumerate() {
        let Ok(re) = PyRegex::new(pattern) else { continue };
        let mut matches = Vec::new();
        let mut pos = 0;
        while pos <= data.len() {
            let Ok(Some(caps)) = re.captures_from_pos(data, pos) else { break };
            let whole = caps.get(0).expect("group 0");
            matches.push((
                caps.name("quote").map(|m| m.as_str().to_string()),
                caps.name("delim").map(|m| m.as_str().to_string()),
                caps.name("space").map(|m| m.as_str().to_string()),
            ));
            pos = if whole.end() > whole.start() {
                whole.end()
            } else {
                whole.end() + data[whole.end()..].chars().next().map_or(1, char::len_utf8)
            };
        }
        if !matches.is_empty() {
            found = matches;
            has_delim_group = i < 3;
            has_space_group = i < 3;
            break;
        }
    }
    if found.is_empty() {
        return (None, false, None, false);
    }
    let mut quotes: IndexMap<String, usize> = IndexMap::new();
    let mut delims: IndexMap<String, usize> = IndexMap::new();
    let mut spaces = 0usize;
    for (quote, delim, space) in &found {
        if let Some(q) = quote.as_ref().filter(|q| !q.is_empty()) {
            *quotes.entry(q.clone()).or_insert(0) += 1;
        }
        if !has_delim_group {
            continue;
        }
        if let Some(d) = delim.as_ref().filter(|d| !d.is_empty())
            && delimiters.is_none_or(|ds| d.chars().next().is_some_and(|c| ds.contains(&c)))
        {
            *delims.entry(d.clone()).or_insert(0) += 1;
        }
        if !has_space_group {
            continue;
        }
        if space.as_ref().is_some_and(|s| !s.is_empty()) {
            spaces += 1;
        }
    }
    let quotechar = max_by_count(&quotes).and_then(|q| q.chars().next());
    let (delim, skipinitialspace) = match max_by_count(&delims) {
        Some(d) => {
            let skip = delims[&d] == spaces;
            if d == "\n" { (None, skip) } else { (d.chars().next(), skip) }
        }
        None => (None, false),
    };
    let delim_text = delim.map(|c| c.to_string()).unwrap_or_default();
    let quote_text = quotechar.map(|c| c.to_string()).unwrap_or_default();
    let dq_pattern = format!(
        r"(?m)(({d})|^)\W*{q}[^{d}\n]*{q}[^{d}\n]*{q}\W*(({d})|$)",
        d = fancy_escape(&delim_text),
        q = quote_text
    );
    let doublequote = PyRegex::new(&dq_pattern)
        .ok()
        .and_then(|re| re.is_match(data).ok())
        .unwrap_or(false);
    (quotechar, doublequote, delim, skipinitialspace)
}

fn fancy_escape(s: &str) -> String {
    regex::escape(s)
}

fn max_by_count(counts: &IndexMap<String, usize>) -> Option<String> {
    let mut best: Option<(&String, usize)> = None;
    for (k, &v) in counts {
        if best.is_none_or(|(_, bv)| v > bv) {
            best = Some((k, v));
        }
    }
    best.map(|(k, _)| k.clone())
}

fn guess_delimiter(data: &str, delimiters: Option<&[char]>) -> (Option<char>, bool) {
    let lines: Vec<&str> = data.split('\n').filter(|l| !l.is_empty()).collect();
    let ascii: Vec<char> = (0u8..127).map(char::from).collect();
    let chunk_length = lines.len().min(10);
    let mut iteration = 0usize;
    // char -> (frequency -> count), insertion ordered.
    let mut char_frequency: IndexMap<char, IndexMap<usize, usize>> = IndexMap::new();
    let mut modes: IndexMap<char, (usize, i64)> = IndexMap::new();
    let mut delims: IndexMap<char, (usize, i64)> = IndexMap::new();
    let (mut start, mut end) = (0usize, chunk_length);
    let skip_for = |d: char| -> bool {
        let first = lines.first().copied().unwrap_or("");
        first.matches(d).count() == first.matches(&format!("{d} ")).count()
    };
    while start < lines.len() {
        iteration += 1;
        for line in &lines[start..end.min(lines.len())] {
            for &ch in &ascii {
                let freq = line.chars().filter(|&c| c == ch).count();
                *char_frequency.entry(ch).or_default().entry(freq).or_insert(0) += 1;
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
                let rest: usize = items
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| {
                        let first_max = items.iter().position(|x| *x == best).unwrap_or(0);
                        *i != first_max
                    })
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
                if v.0 > 0
                    && v.1 > 0
                    && (v.1 as f64 / total) >= consistency
                    && delimiters.is_none_or(|ds| ds.contains(&k))
                {
                    delims.insert(k, v);
                }
            }
            consistency -= 0.01;
        }
        if delims.len() == 1 {
            let delim = *delims.keys().next().expect("one delimiter");
            return (Some(delim), skip_for(delim));
        }
        start = end;
        end += chunk_length;
        if chunk_length == 0 {
            break;
        }
    }
    if delims.is_empty() {
        return (None, false);
    }
    if delims.len() > 1 {
        for d in [',', '\t', ';', ' ', ':'] {
            if delims.contains_key(&d) {
                return (Some(d), skip_for(d));
            }
        }
    }
    let mut items: Vec<((usize, i64), char)> = delims.iter().map(|(&k, &v)| (v, k)).collect();
    items.sort();
    let delim = items.last().map(|x| x.1).expect("non-empty");
    (Some(delim), skip_for(delim))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reader_quotes_and_multiline() {
        let rows: Vec<Vec<String>> = Reader::from_text("id,notes\n1,\"line1\nline2\"\n2,\"a,b\"\n", Dialect::default())
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(rows[1], vec!["1".to_string(), "line1\nline2".to_string()]);
        assert_eq!(rows[2], vec!["2".to_string(), "a,b".to_string()]);
    }

    #[test]
    fn sniffing() {
        assert_eq!(sniff("a;b\n1;2\n", Some(&[',', ';', '\t', '|'])).unwrap().delimiter, ';');
        assert_eq!(sniff("a|b\n1|2\n", Some(&[',', ';', '\t', '|'])).unwrap().delimiter, '|');
        assert_eq!(
            sniff("company,revenue\n\"Acme, Inc.\",100\n", Some(&[',', ';', '\t', '|']))
                .unwrap()
                .delimiter,
            ','
        );
    }
}
