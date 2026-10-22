//! CPython 3.13.11 mutable sets with immutable caller-owned row objects.
//!
//! Copyright (c) 2001-2024 Python Software Foundation; All Rights Reserved.
//! License: ../../LICENSE-CPYTHON; sources: ../../SOURCE-HASH-SET-PROVENANCE.md.
//! No Rust Hash/Eq, numeric comparison, source row hashing, or AST evaluation
//! takes place here. Caller objects stay immutable and callbacks cannot reenter
//! the table. A failed lookup/equality leaves that single operation unchanged.
//! Allocation, overflow or cancellation during a rebuild can leave a partial
//! table; its owner must discard it instead of publishing a successful state.
use crate::source_hash::normalize_hash_word;

const MIN_SIZE: usize = 8;
const LINEAR_PROBES: usize = 9;
const PERTURB_SHIFT: u32 = 5;

#[derive(Debug)]
pub(crate) enum SourceSetError<E> {
    Callback(E),
    Allocation,
    IndexOverflow,
}

type SetResult<T, E> = Result<T, SourceSetError<E>>;

#[derive(Clone)]
struct Entry<T> {
    hash: i64,
    // None inside an occupied table slot is the deletion marker. A vacant
    // outer slot still terminates lookup. No caller value is a sentinel.
    value: Option<T>,
}

#[derive(Clone)]
enum Table<T> {
    Small([Option<Entry<T>>; MIN_SIZE]),
    Heap(Vec<Option<Entry<T>>>),
}

impl<T> Table<T> {
    fn slots(&self) -> &[Option<Entry<T>>] {
        match self {
            Self::Small(slots) => slots,
            Self::Heap(slots) => slots,
        }
    }

    fn slots_mut(&mut self) -> &mut [Option<Entry<T>>] {
        match self {
            Self::Small(slots) => slots,
            Self::Heap(slots) => slots,
        }
    }
}

/// Shared by real table growth and the explicit capacity-refusal control.
/// Vec's additional-capacity contract; no fixed numeric/output row budget.
pub(crate) fn reserve_set_table<T, E>(table: &mut Vec<T>, additional: usize) -> SetResult<(), E> {
    table
        .try_reserve_exact(additional)
        .map_err(|_| SourceSetError::Allocation)
}

/// Host snapshots copy physical slots exactly; this is not Python set.copy().
#[derive(Clone)]
pub(crate) struct SourceSet<T> {
    table: Table<T>,
    used: usize,
    fill: usize,
}

impl<T> SourceSet<T> {
    pub(crate) fn new() -> Self {
        Self {
            table: Table::Small(std::array::from_fn(|_| None)),
            used: 0,
            fill: 0,
        }
    }

    pub(crate) fn physical_slots(&self) -> usize { self.table.slots().len() }
    pub(crate) fn len(&self) -> usize {
        self.used
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &T> {
        self.table
            .slots()
            .iter()
            .filter_map(|slot| slot.as_ref().and_then(|entry| entry.value.as_ref()))
    }

    /// CPython set_merge, including tables with deletion markers. Copy/union
    /// retain stored hashes and physical slot order, unlike set(generator).
    pub(crate) fn merge_hashed<E>(
        &mut self,
        other: &Self,
        same_object: &mut impl FnMut(&T, &T) -> bool,
        equals: &mut impl FnMut(&T, &T) -> Result<bool, E>,
        check_cancel: &mut impl FnMut() -> Result<(), E>,
    ) -> SetResult<(), E>
    where
        T: Clone,
    {
        check_cancel().map_err(SourceSetError::Callback)?;
        if other.used == 0 {
            return Ok(());
        }
        let combined = self
            .used
            .checked_add(other.used)
            .ok_or(SourceSetError::IndexOverflow)?;
        let fill5 = self
            .fill
            .checked_add(other.used)
            .ok_or(SourceSetError::IndexOverflow)?
            .checked_mul(5)
            .ok_or(SourceSetError::IndexOverflow)?;
        let mask3 = (self.table.slots().len() - 1)
            .checked_mul(3)
            .ok_or(SourceSetError::IndexOverflow)?;
        if fill5 >= mask3 {
            self.resize(
                combined
                    .checked_mul(2)
                    .ok_or(SourceSetError::IndexOverflow)?,
                check_cancel,
            )?;
        }
        if self.fill == 0
            && self.table.slots().len() == other.table.slots().len()
            && other.fill == other.used
        {
            for (target, source) in self.table.slots_mut().iter_mut().zip(other.table.slots()) {
                check_cancel().map_err(SourceSetError::Callback)?;
                if let Some(entry) = source {
                    *target = Some(Entry {
                        hash: entry.hash,
                        value: entry.value.clone(),
                    });
                }
            }
            self.used = other.used;
            self.fill = other.fill;
        } else if self.fill == 0 {
            for entry in other.table.slots().iter().flatten() {
                if entry.value.is_none() {
                    continue;
                }
                self.insert_clean(
                    Entry {
                        hash: entry.hash,
                        value: entry.value.clone(),
                    },
                    check_cancel,
                )?;
                self.used += 1;
                self.fill += 1;
            }
        } else {
            for entry in other.table.slots().iter().flatten() {
                let Some(value) = &entry.value else {
                    continue;
                };
                self.insert_hashed(value.clone(), entry.hash, same_object, equals, check_cancel)?;
            }
        }
        Ok(())
    }

    pub(crate) fn copy_hashed<E>(
        &self,
        same_object: &mut impl FnMut(&T, &T) -> bool,
        equals: &mut impl FnMut(&T, &T) -> Result<bool, E>,
        check_cancel: &mut impl FnMut() -> Result<(), E>,
    ) -> SetResult<Self, E>
    where
        T: Clone,
    {
        let mut copy = Self::new();
        copy.merge_hashed(self, same_object, equals, check_cancel)?;
        Ok(copy)
    }

    /// Read-only set_lookkey for immutable caller objects.
    /// Hash computation belongs to the caller, including on an empty set.
    pub(crate) fn contains_hashed<E>(
        &self,
        value: &T,
        hash: i64,
        same_object: &mut impl FnMut(&T, &T) -> bool,
        equals: &mut impl FnMut(&T, &T) -> Result<bool, E>,
        check_cancel: &mut impl FnMut() -> Result<(), E>,
    ) -> SetResult<bool, E> {
        Ok(self
            .lookup_hashed(value, hash, same_object, equals, check_cancel)?
            .is_some())
    }

    /// Returns whether an existing representative was removed. Missing keys
    /// still perform the normal lookup; callback errors leave this set intact.
    pub(crate) fn discard_hashed<E>(
        &mut self,
        value: &T,
        hash: i64,
        same_object: &mut impl FnMut(&T, &T) -> bool,
        equals: &mut impl FnMut(&T, &T) -> Result<bool, E>,
        check_cancel: &mut impl FnMut() -> Result<(), E>,
    ) -> SetResult<bool, E> {
        let Some(at) = self.lookup_hashed(value, hash, same_object, equals, check_cancel)? else {
            return Ok(false);
        };
        self.table.slots_mut()[at] = Some(Entry {
            hash: -1,
            value: None,
        });
        self.used -= 1;
        Ok(true)
    }

    pub(crate) fn clear(&mut self) {
        self.table = Table::Small(std::array::from_fn(|_| None));
        self.used = 0;
        self.fill = 0;
    }

    fn lookup_hashed<E>(
        &self,
        value: &T,
        hash: i64,
        same_object: &mut impl FnMut(&T, &T) -> bool,
        equals: &mut impl FnMut(&T, &T) -> Result<bool, E>,
        check_cancel: &mut impl FnMut() -> Result<(), E>,
    ) -> SetResult<Option<usize>, E> {
        check_cancel().map_err(SourceSetError::Callback)?;
        let hash = normalize_hash_word(hash);
        let mask = self.table.slots().len() - 1;
        let mut perturb = hash as u64;
        let mut index = (perturb & mask as u64) as usize;
        loop {
            check_cancel().map_err(SourceSetError::Callback)?;
            let probes = if mask >= LINEAR_PROBES && index <= mask - LINEAR_PROBES {
                LINEAR_PROBES
            } else {
                0
            };
            for offset in 0..=probes {
                let Some(existing) = &self.table.slots()[index + offset] else {
                    return Ok(None);
                };
                if existing.hash == hash {
                    let Some(existing_value) = &existing.value else {
                        continue;
                    };
                    if same_object(existing_value, value) {
                        return Ok(Some(index + offset));
                    }
                    check_cancel().map_err(SourceSetError::Callback)?;
                    if equals(existing_value, value).map_err(SourceSetError::Callback)? {
                        return Ok(Some(index + offset));
                    }
                }
            }
            perturb >>= PERTURB_SHIFT;
            index = next_index(index, perturb, mask);
        }
    }

    /// Resume physical slot iteration without storing a self-borrowing iterator
    /// or rebuilding a second table. Callers reset the cursor for each left row.
    pub(crate) fn next_occupied_from<'s, E>(
        &'s self,
        cursor: &mut usize,
        check_cancel: &mut impl FnMut() -> Result<(), E>,
    ) -> SetResult<Option<&'s T>, E> {
        while *cursor < self.table.slots().len() {
            check_cancel().map_err(SourceSetError::Callback)?;
            let at = *cursor;
            *cursor += 1; // at < a live slice length, so this cannot overflow.
            if let Some(entry) = &self.table.slots()[at] {
                if let Some(value) = &entry.value {
                    return Ok(Some(value));
                }
            }
        }
        Ok(None)
    }

    /// The caller evaluates the incoming object's hash even for reinsertion
    /// of the same object. `equals` receives existing first, incoming second.
    /// Success retains the complete first representative, not incoming fields.
    pub(crate) fn insert_hashed<E>(
        &mut self,
        value: T,
        hash: i64,
        same_object: &mut impl FnMut(&T, &T) -> bool,
        equals: &mut impl FnMut(&T, &T) -> Result<bool, E>,
        check_cancel: &mut impl FnMut() -> Result<(), E>,
    ) -> SetResult<(), E> {
        check_cancel().map_err(SourceSetError::Callback)?;
        // Idempotent guard of the already-normalized caller contract. -1 is
        // CPython's empty/deleted/error sentinel, never an occupied hash.
        let hash = normalize_hash_word(hash);
        let mask = self.table.slots().len() - 1;
        let mut perturb = hash as u64;
        let mut index = (perturb & mask as u64) as usize;
        let mut free_slot = None;
        loop {
            check_cancel().map_err(SourceSetError::Callback)?;
            let probes = if index <= mask.saturating_sub(LINEAR_PROBES) && mask >= LINEAR_PROBES {
                LINEAR_PROBES
            } else {
                0
            };
            for offset in 0..=probes {
                let at = index + offset;
                if let Some(existing) = &self.table.slots()[at] {
                    if existing.hash == hash {
                        let Some(existing_value) = &existing.value else {
                            continue;
                        };
                        if same_object(existing_value, &value) {
                            return Ok(());
                        }
                        check_cancel().map_err(SourceSetError::Callback)?;
                        if equals(existing_value, &value).map_err(SourceSetError::Callback)? {
                            return Ok(());
                        }
                    } else if existing.value.is_none() {
                        // CPython retains the last visited dummy, not the
                        // first one. This affects subsequent iteration order.
                        free_slot = Some(at);
                    }
                } else {
                    let used = self
                        .used
                        .checked_add(1)
                        .ok_or(SourceSetError::IndexOverflow)?;
                    if let Some(free) = free_slot {
                        self.table.slots_mut()[free] = Some(Entry {
                            hash,
                            value: Some(value),
                        });
                        self.used = used;
                        return Ok(());
                    }
                    let fill = self
                        .fill
                        .checked_add(1)
                        .ok_or(SourceSetError::IndexOverflow)?;
                    self.table.slots_mut()[at] = Some(Entry {
                        hash,
                        value: Some(value),
                    });
                    self.used = used;
                    self.fill = fill;
                    let fill5 = fill.checked_mul(5).ok_or(SourceSetError::IndexOverflow)?;
                    let mask3 = mask.checked_mul(3).ok_or(SourceSetError::IndexOverflow)?;
                    if fill5 >= mask3 {
                        let minused = used
                            .checked_mul(if used > 50_000 { 2 } else { 4 })
                            .ok_or(SourceSetError::IndexOverflow)?;
                        self.resize(minused, check_cancel)?;
                    }
                    return Ok(());
                }
            }
            perturb >>= PERTURB_SHIFT;
            index = next_index(index, perturb, mask);
        }
    }

    fn resize<E>(
        &mut self,
        minused: usize,
        check_cancel: &mut impl FnMut() -> Result<(), E>,
    ) -> SetResult<(), E> {
        let mut size = MIN_SIZE;
        while size <= minused {
            size = size.checked_mul(2).ok_or(SourceSetError::IndexOverflow)?;
        }
        let bytes = size
            .checked_mul(size_of::<Option<Entry<T>>>())
            .ok_or(SourceSetError::IndexOverflow)?;
        if bytes > isize::MAX as usize {
            return Err(SourceSetError::IndexOverflow);
        }
        check_cancel().map_err(SourceSetError::Callback)?;
        if size == MIN_SIZE && matches!(self.table, Table::Small(_)) && self.fill == self.used {
            return Ok(());
        }
        let table = if size == MIN_SIZE {
            Table::Small(std::array::from_fn(|_| None))
        } else {
            let mut slots = Vec::new();
            reserve_set_table(&mut slots, size)?;
            for _ in 0..size {
                check_cancel().map_err(SourceSetError::Callback)?;
                slots.push(None);
            }
            Table::Heap(slots)
        };
        let mut old = std::mem::replace(&mut self.table, table);
        self.used = 0;
        self.fill = 0;
        // Reinsert in old physical slot order with stored hashes, invoking
        // neither object hash nor equality. On cancellation the caller drops
        // the private partial table; no partial query rows are published.
        for slot in old.slots_mut() {
            check_cancel().map_err(SourceSetError::Callback)?;
            if let Some(entry) = slot.take() {
                if entry.value.is_none() {
                    continue;
                }
                self.insert_clean(entry, check_cancel)?;
                self.used += 1;
                self.fill += 1;
            }
        }
        Ok(())
    }

    fn insert_clean<E>(
        &mut self,
        entry: Entry<T>,
        check_cancel: &mut impl FnMut() -> Result<(), E>,
    ) -> SetResult<(), E> {
        let mask = self.table.slots().len() - 1;
        let mut perturb = entry.hash as u64;
        let mut index = (perturb & mask as u64) as usize;
        loop {
            check_cancel().map_err(SourceSetError::Callback)?;
            let probes = if mask >= LINEAR_PROBES && index <= mask - LINEAR_PROBES {
                LINEAR_PROBES
            } else {
                0
            };
            for offset in 0..=probes {
                if self.table.slots()[index + offset].is_none() {
                    self.table.slots_mut()[index + offset] = Some(entry);
                    return Ok(());
                }
            }
            perturb >>= PERTURB_SHIFT;
            index = next_index(index, perturb, mask);
        }
    }
}

fn next_index(index: usize, perturb: u64, mask: usize) -> usize {
    ((index as u64)
        .wrapping_mul(5)
        .wrapping_add(1)
        .wrapping_add(perturb)
        & mask as u64) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn source_set_allocation_refusal_keeps_scratch_unchanged() {
        let mut scratch = vec![7_u8];
        let result = reserve_set_table::<_, ()>(&mut scratch, usize::MAX);
        assert!(matches!(result, Err(SourceSetError::Allocation)));
        assert_eq!(scratch, [7]);
    }

    #[test]
    fn source_set_cancellation_preserves_callback_identity_and_stops_equality() {
        let marker = Rc::new(());
        let cancelled = Cell::new(false);
        let calls = Cell::new(0);
        let mut set = SourceSet::new();
        let mut same = |a: &usize, b: &usize| a == b;
        let mut equal = |_: &usize, _: &usize| {
            calls.set(calls.get() + 1);
            if calls.get() == 3 {
                cancelled.set(true);
            }
            Ok(false)
        };
        let mut cancel = || {
            if cancelled.get() {
                Err(Rc::clone(&marker))
            } else {
                Ok(())
            }
        };
        let result = (0..10)
            .try_for_each(|value| set.insert_hashed(value, 7, &mut same, &mut equal, &mut cancel));
        let Err(SourceSetError::Callback(actual)) = result else {
            panic!("expected cancellation")
        };
        assert!(Rc::ptr_eq(&actual, &marker));
        assert_eq!(calls.get(), 3);
    }
}
