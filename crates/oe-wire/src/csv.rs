//! Strict bounded CSV fields shared by historical wheel intake and drawing inventory.
//! Original RECORD diagnostics remain unchanged for compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CsvError(pub &'static str);
impl std::fmt::Display for CsvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for CsvError {}
type Result<T> = std::result::Result<T, CsvError>;
pub fn rows(text: &str) -> Result<Vec<Vec<String>>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut closed = false;
    let mut started = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                    closed = true;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        if c == '"' {
            if started || closed {
                return Err(CsvError("invalid RECORD CSV quote"));
            }
            quoted = true;
            started = true;
        } else if c == ',' {
            row.push(std::mem::take(&mut field));
            started = false;
            closed = false;
        } else if c == '\n' || c == '\r' {
            if c == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            row.push(std::mem::take(&mut field));
            rows.push(std::mem::take(&mut row));
            if rows.len() > u16::MAX as usize {
                return Err(CsvError("RECORD exceeds row limit"));
            }
            started = false;
            closed = false;
        } else {
            if closed {
                return Err(CsvError("invalid text after RECORD CSV quote"));
            }
            field.push(c);
            started = true;
        }
    }
    if quoted {
        return Err(CsvError("unterminated RECORD CSV quote"));
    }
    if started || closed || !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    Ok(rows)
}
