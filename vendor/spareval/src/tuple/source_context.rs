//! Binding-domain operations from RDFLib 7.6.0 sparql.py. These operate on the
//! existing row carrier; they are not a parser or a second query evaluator.
use super::{InternalTuple, SolutionValue};
use std::cell::Cell;
use std::rc::Rc;

#[derive(Debug, Default)]
struct SourceRowIdentity {
    raw_hash: Cell<Option<i64>>,
}

#[derive(Clone, Debug)]
pub(super) struct SourceTupleMetadata {
    order: Vec<usize>,
    identity: Rc<SourceRowIdentity>,
}
impl SourceTupleMetadata {
    pub(super) fn new() -> Self {
        Self {
            order: Vec::new(),
            identity: Rc::default(),
        }
    }
    pub(super) fn record_set(&mut self, index: usize, present: bool) {
        if !present {
            self.order.push(index);
        }
        // The Rust carrier is mutable; a changed carrier denotes a new frozen
        // object. Its transport siblings keep their original cache/identity.
        self.identity = Rc::default();
    }
    pub(super) fn record_remove(&mut self, index: usize) {
        self.order.retain(|slot| *slot != index);
        self.identity = Rc::default();
    }
}

impl<T> InternalTuple<T> {
    pub(crate) fn source_local_order(&self) -> &[usize] {
        &self.source.order
    }
    pub(crate) fn source_same_object(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.source.identity, &other.source.identity)
    }
    pub(crate) fn source_raw_hash_cache(&self) -> Option<i64> {
        self.source.identity.raw_hash.get()
    }
    /// The callbacks return Python-slot hash words. Preserve the source's
    /// partial cache after a failed component, including the initial zero.
    pub(crate) fn source_hash_by<E>(
        &self,
        mut hash_key: impl FnMut(usize) -> Result<i64, E>,
        mut hash_value: impl FnMut(&SolutionValue<T>) -> Result<i64, E>,
    ) -> Result<i64, E> {
        let cache = &self.source.identity.raw_hash;
        if cache.get().is_none() {
            cache.set(Some(0));
            let mut raw = 0;
            for &slot in &self.source.order {
                // order is maintained by the only tuple mutation methods.
                if let Some(value) = self.cell(slot) {
                    raw ^= normalize(hash_key(slot)?);
                    cache.set(Some(raw));
                    raw ^= normalize(hash_value(value)?);
                    cache.set(Some(raw));
                }
            }
        }
        Ok(normalize(cache.get().unwrap_or(0)))
    }
    pub(crate) fn source_compatible_by<E>(
        &self,
        right: &Self,
        right_initial: &Self,
        mut equal: impl FnMut(&SolutionValue<T>, &SolutionValue<T>) -> Result<bool, E>,
    ) -> Result<bool, E> {
        for &slot in &self.source.order {
            if let (Some(left), Some(other)) = (
                self.cell(slot),
                right.cell(slot).or_else(|| right_initial.cell(slot)),
            ) {
                if !equal(left, other)? {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
    /// Mapping equality is local-only, with left insertion-order comparisons.
    /// Same-object shortcut belongs to the set caller before this operation.
    pub(crate) fn source_mapping_equal_by<E>(
        &self,
        right: &Self,
        mut equal: impl FnMut(&SolutionValue<T>, &SolutionValue<T>) -> Result<bool, E>,
    ) -> Result<bool, E> {
        if self.source.order.len() != right.source.order.len() {
            return Ok(false);
        }
        for &slot in &self.source.order {
            let Some(other) = right.cell(slot) else {
                return Ok(false);
            };
            if let Some(left) = self.cell(slot) {
                if !equal(left, other)? {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}

impl<T: Clone> InternalTuple<T> {
    pub(crate) fn source_fresh_copy(&self) -> Self {
        let mut result = self.clone();
        result.source.identity = Rc::default();
        result
    }
    /// FrozenBindings.project/remember(empty) are empty, in contrast with
    /// QueryContext.solution(empty), which copies every binding.
    pub(crate) fn source_project(&self, columns: &[usize]) -> Self {
        self.source_select(|slot| columns.contains(&slot))
    }
    pub(crate) fn source_solution(&self, columns: Option<&[usize]>) -> Self {
        if let Some(result) = self.source_context_solution(columns) {
            return result;
        }
        match columns {
            Some(columns) if !columns.is_empty() => self.source_project(columns),
            _ => self.source_fresh_copy(),
        }
    }
    fn source_select(&self, mut keep: impl FnMut(usize) -> bool) -> Self {
        let mut result = Self::with_capacity(self.capacity());
        for &slot in &self.source.order {
            if keep(slot) {
                if let Some(value) = self.cell(slot) {
                    result.set_cell(slot, value.clone());
                }
            }
        }
        result.source_context_owner = self.source_context_owner.clone();
        result
    }
    pub(crate) fn source_forget(&self, before: &Self, initial: &Self, except: &[usize]) -> Self {
        let current = before.source_context_bindings();
        let before = current.as_ref().unwrap_or(before);
        self.source_select(|slot| {
            except.contains(&slot)
                || self.source_context_initial_has(slot).unwrap_or_else(|| initial.contains_cell(slot))
                || matches!(before.cell(slot), None | Some(SolutionValue::ExplicitNone))
        })
    }
    /// Right assignments overwrite in place; only new keys are appended.
    pub(crate) fn source_merge(&self, right: &Self) -> Self {
        let mut result = self.source_fresh_copy();
        for &slot in &right.source.order {
            if let Some(value) = right.cell(slot) {
                result.set_cell(slot, value.clone());
            }
        }
        result
    }
    pub(crate) fn source_thaw(&self, frozen: &Self, initial: &Self) -> Self {
        if let Some(result) = self.source_context_thaw(frozen) {
            return result;
        }
        let bindings = if frozen.source.order.is_empty() {
            self
        } else {
            frozen
        };
        bindings.source_merge(initial)
    }
    pub(crate) fn source_push(&self, initial: &Self) -> Self {
        if let Some(result) = self.source_context_clean() {
            return result;
        }
        self.source_merge(initial)
    }
    /// False is the source AlreadyBound branch. Fatal callback errors retain
    /// their exact identity; equal assignments store the incoming value object.
    pub(crate) fn source_assign_by<E>(
        &mut self,
        slot: usize,
        value: SolutionValue<T>,
        mut equal: impl FnMut(&SolutionValue<T>, &SolutionValue<T>) -> Result<bool, E>,
    ) -> Result<bool, E> {
        if let Some(result) = self.source_context_assign_by(slot, value.clone(), &mut equal)? {
            return Ok(result);
        }
        if let Some(previous) = self.cell(slot) {
            if !equal(previous, &value)? {
                return Ok(false);
            }
        }
        self.set_cell(slot, value);
        Ok(true)
    }
}

fn normalize(word: i64) -> i64 {
    crate::source_hash::normalize_hash_word(word)
}

#[cfg(test)]
mod mutation_tests {
    use super::*;

    #[test]
    fn source_context_carrier_mutation_keeps_transport_sibling_identity_and_cache() {
        let mut row = InternalTuple::with_capacity(4);
        row.set(3, 5_i64);
        assert_eq!(
            row.source_hash_by(|_| Ok::<_, ()>(6), |cell| Ok(*cell.rdf().unwrap())),
            Ok(3)
        );
        let sibling = row.clone();
        row.set(3, 7);
        assert!(!row.source_same_object(&sibling));
        assert_eq!(row.source_raw_hash_cache(), None);
        assert_eq!(sibling.source_raw_hash_cache(), Some(3));
        assert_eq!(sibling.get(3), Some(&5));
        assert_eq!(row.source_local_order(), &[3]);
        row.set(1, 9);
        row.remove_cell(3);
        row.set(3, 11);
        assert_eq!(row.source_local_order(), &[1, 3]);
        assert_eq!(sibling.source_local_order(), &[3]);
        assert_eq!(sibling.source_raw_hash_cache(), Some(3));
    }
}
