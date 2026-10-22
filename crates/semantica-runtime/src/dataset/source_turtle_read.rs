//! One writer and one source sort schedule; receiving supplies read observations.
use anyhow::{Result, ensure};
use spareval::{SourceTermInput, SourceTurtleTerm, SourceTurtleView};

pub(super) trait TurtleTerm: Clone {
    fn term(&self) -> Result<SourceTermInput<'_>>;
}
impl TurtleTerm for SourceTurtleTerm {
    fn term(&self) -> Result<SourceTermInput<'_>> {
        Ok(self.term()?)
    }
}
impl TurtleTerm for semantica_contracts::SemanticSourceStorageTerm3 {
    fn term(&self) -> Result<SourceTermInput<'_>> {
        super::source_storage_witness::borrowed(self)
    }
}
pub(super) trait TurtleRead {
    type Term: TurtleTerm;
    fn charge(&self, amount: usize) -> Result<()>;
    fn iri(&self, value: &str) -> Result<Self::Term>;
    fn scan(&self, pattern: [Option<&Self::Term>; 3]) -> Result<Vec<[Self::Term; 3]>>;
    fn truth(&self, value: &Self::Term) -> Result<bool>;
    fn cached(&self, value: &Self::Term) -> Result<bool>;
    fn less(&self, left: &Self::Term, right: &Self::Term) -> Result<bool>;

    fn sort(&self, values: &mut Vec<Self::Term>, prefixes: Option<&[(bool, usize)]>) -> Result<()> {
        sort(self, values, prefixes)
    }
}
/// Actual shared source-sort adapter; visible to the existing source mapper.
pub(super) fn sort<G: TurtleRead + ?Sized>(
    graph: &G,
    values: &mut Vec<G::Term>,
    prefixes: Option<&[(bool, usize)]>,
) -> Result<()> {
    ensure!(
        prefixes.is_none_or(|p| p.len() == values.len()),
        "source_turtle_sort_prefix_length"
    );
    graph.charge(values.len())?;
    let mut order: Vec<_> = (0..values.len()).collect();
    spareval::source_sort_indices(
        &mut order,
        false,
        |a, b| {
            if let Some(p) = prefixes
                && p[a] != p[b]
            {
                return Ok(p[a] < p[b]);
            }
            graph.less(&values[a], &values[b])
        },
        || graph.charge(1),
    )
    .map_err(|e| match e {
        spareval::SourceSortError::Callback(e) => e,
        spareval::SourceSortError::Allocation => anyhow::anyhow!("source_turtle_sort_allocation"),
        spareval::SourceSortError::IndexOverflow => anyhow::anyhow!("source_turtle_sort_index"),
    })?;
    let mut sorted = Vec::new();
    sorted.try_reserve_exact(values.len())?;
    sorted.extend(order.into_iter().map(|i| values[i].clone()));
    *values = sorted;
    Ok(())
}

impl TurtleRead for SourceTurtleView {
    type Term = SourceTurtleTerm;
    fn charge(&self, amount: usize) -> Result<()> {
        Ok(self.charge(amount)?)
    }
    fn iri(&self, value: &str) -> Result<Self::Term> {
        Ok(self.iri(value)?)
    }
    fn scan(&self, pattern: [Option<&Self::Term>; 3]) -> Result<Vec<[Self::Term; 3]>> {
        Ok(self.scan(pattern)?)
    }
    fn truth(&self, value: &Self::Term) -> Result<bool> {
        Ok(self.truth(value)?)
    }
    fn cached(&self, value: &Self::Term) -> Result<bool> {
        Ok(value.has_cached_value()?)
    }
    fn less(&self, left: &Self::Term, right: &Self::Term) -> Result<bool> {
        Ok(self.less(left, right)?)
    }
}
