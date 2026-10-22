//! Source algebra.triples sequence reduction for the admitted builtin row
//! containers. This is a parse-to-plan operation, never a runtime interpreter.
use std::fmt;

#[doc(hidden)]
#[derive(Debug)]
pub enum SourceTripleRow<'a, T> {
    List(&'a [T]),
    Tuple(&'a [T]),
}
impl<T> SourceTripleRow<'_, T> {
    fn cells(&self) -> &[T] {
        match self {
            Self::List(cells) | Self::Tuple(cells) => cells,
        }
    }
}

#[doc(hidden)]
#[derive(Debug, Eq, PartialEq)]
pub enum SourceTripleRowsError {
    EmptyReduction,
    ListPlusTuple,
    TuplePlusList,
    InvalidLength,
    ResourceLimit,
}
impl SourceTripleRowsError {
    /// Original source exceptions are distinct from Rust resource policy.
    pub fn source_exception(&self) -> Option<(&'static str, &'static str, &'static str)> {
        let (class, argument) = match self {
            Self::EmptyReduction => (
                "TypeError",
                "reduce() of empty iterable with no initial value",
            ),
            Self::ListPlusTuple => (
                "TypeError",
                "can only concatenate list (not \"tuple\") to list",
            ),
            Self::TuplePlusList => (
                "TypeError",
                "can only concatenate tuple (not \"list\") to tuple",
            ),
            Self::InvalidLength => ("Exception", "these aint triples"),
            Self::ResourceLimit => return None,
        };
        Some(("builtins", class, argument))
    }
}
impl fmt::Display for SourceTripleRowsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            self.source_exception()
                .map_or("source triple rows resource limit", |(_, _, text)| text),
        )
    }
}
impl std::error::Error for SourceTripleRowsError {}

/// Concatenate in source order, then group the total into triples. Individual
/// row lengths are unrestricted. Borrowed inputs and their term identities are
/// preserved; the caller applies the existing source reorderTriples owner.
#[doc(hidden)]
pub fn source_group_triple_rows<T: Clone>(
    rows: &[SourceTripleRow<'_, T>],
) -> Result<Vec<[T; 3]>, SourceTripleRowsError> {
    let first = rows.first().ok_or(SourceTripleRowsError::EmptyReduction)?;
    let mut length = first.cells().len();
    for row in &rows[1..] {
        match (first, row) {
            (SourceTripleRow::List(_), SourceTripleRow::Tuple(_)) => {
                return Err(SourceTripleRowsError::ListPlusTuple);
            }
            (SourceTripleRow::Tuple(_), SourceTripleRow::List(_)) => {
                return Err(SourceTripleRowsError::TuplePlusList);
            }
            _ => {}
        }
        length = length
            .checked_add(row.cells().len())
            .ok_or(SourceTripleRowsError::ResourceLimit)?;
    }
    if length % 3 != 0 {
        return Err(SourceTripleRowsError::InvalidLength);
    }
    let mut result = Vec::new();
    result
        .try_reserve_exact(length / 3)
        .map_err(|_| SourceTripleRowsError::ResourceLimit)?;
    let mut terms = rows.iter().flat_map(|row| row.cells().iter());
    for _ in 0..length / 3 {
        // The checked total above is the exact length of this borrowed iterator.
        result.push(std::array::from_fn(|_| terms.next().unwrap().clone()));
    }
    Ok(result)
}
