//! Fallible index adaptation of CPython 3.13.11 Objects/listobject.c list sort.
//!
//! Copyright (c) 2001-2024 Python Software Foundation; All Rights Reserved.
//! The original Python license is retained in ../../LICENSE-CPYTHON.
//! See ../../SOURCE-SORT-PROVENANCE.md for the fixed source and changes.
//! Keys and rows remain owned by the caller. Only index values move here;
//! every comparison follows the original natural-run/Powersort/gallop schedule.
//! Cancellation and allocation refusal are explicit local resource contracts.
use std::marker::PhantomData;

const MIN_GALLOP: usize = 7;
const MAX_MINRUN: usize = 64;
const MAX_PENDING: usize = usize::BITS as usize;

#[derive(Debug)]
pub enum SourceSortError<E> {
    Callback(E),
    Allocation,
    IndexOverflow,
}

type SortResult<T, E> = Result<T, SourceSortError<E>>;

/// This is the same fallible reservation used by actual merging. The argument
/// has Vec's additional-capacity meaning; no numeric/text budget limits rows.
pub(super) fn reserve_sort_scratch<E>(
    scratch: &mut Vec<usize>,
    additional: usize,
) -> SortResult<(), E> {
    scratch
        .try_reserve_exact(additional)
        .map_err(|_| SourceSortError::Allocation)
}

/// Sorts already evaluated key indices. On error the caller must discard the
/// private permutation and publish no sorted rows. E is returned unchanged.
pub fn source_sort_indices<E>(
    indices: &mut [usize],
    descending: bool,
    less: impl FnMut(usize, usize) -> Result<bool, E>,
    check_cancel: impl FnMut() -> Result<(), E>,
) -> SortResult<(), E> {
    let mut state = SortState {
        indices,
        scratch: Vec::new(),
        pending: [Run::EMPTY; MAX_PENDING],
        pending_len: 0,
        min_gallop: MIN_GALLOP,
        less,
        check_cancel,
        error: PhantomData,
    };
    state.check()?;
    // This is the bound of a real usize allocation, not an artificial row cap.
    // It also proves that doubled run midpoints fit usize on every target.
    if state.indices.len() > (isize::MAX as usize) / size_of::<usize>() {
        return Err(SourceSortError::IndexOverflow);
    }
    let n = state.indices.len();
    if n < 2 {
        return Ok(());
    }
    if descending {
        state.reverse(0, n)?;
    }
    state.sort()?;
    if descending {
        state.reverse(0, n)?;
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct Run {
    base: usize,
    len: usize,
    power: usize,
}
impl Run {
    const EMPTY: Self = Self {
        base: 0,
        len: 0,
        power: 0,
    };
}

#[derive(Clone, Copy)]
enum Buffer {
    Rows,
    Scratch,
}

struct SortState<'a, E, L, C> {
    indices: &'a mut [usize],
    scratch: Vec<usize>,
    pending: [Run; MAX_PENDING],
    pending_len: usize,
    min_gallop: usize,
    less: L,
    check_cancel: C,
    error: PhantomData<fn() -> E>,
}

impl<E, L, C> SortState<'_, E, L, C>
where
    L: FnMut(usize, usize) -> Result<bool, E>,
    C: FnMut() -> Result<(), E>,
{
    fn check(&mut self) -> SortResult<(), E> {
        (self.check_cancel)().map_err(SourceSortError::Callback)
    }

    fn lt(&mut self, left: usize, right: usize) -> SortResult<bool, E> {
        self.check()?;
        (self.less)(left, right).map_err(SourceSortError::Callback)
    }

    fn key(&self, buffer: Buffer, at: usize) -> usize {
        match buffer {
            Buffer::Rows => self.indices[at],
            Buffer::Scratch => self.scratch[at],
        }
    }

    fn reverse(&mut self, start: usize, len: usize) -> SortResult<(), E> {
        for i in 0..len / 2 {
            self.check()?;
            self.indices.swap(start + i, start + len - 1 - i);
        }
        Ok(())
    }

    // Directional moves preserve memmove's overlap semantics. Cancellation is
    // checked during long comparison-free moves, with no extra less calls.
    fn move_rows(&mut self, source: usize, dest: usize, len: usize) -> SortResult<(), E> {
        if source < dest {
            for i in (0..len).rev() {
                self.check()?;
                self.indices[dest + i] = self.indices[source + i];
            }
        } else if source > dest {
            for i in 0..len {
                self.check()?;
                self.indices[dest + i] = self.indices[source + i];
            }
        }
        Ok(())
    }

    fn copy_scratch(&mut self, source: usize, dest: usize, len: usize) -> SortResult<(), E> {
        for i in 0..len {
            self.check()?;
            self.indices[dest + i] = self.scratch[source + i];
        }
        Ok(())
    }

    fn fill_scratch(&mut self, source: usize, len: usize) -> SortResult<(), E> {
        self.check()?;
        self.scratch.clear();
        reserve_sort_scratch(&mut self.scratch, len)?;
        for i in 0..len {
            self.check()?;
            // The reservation above makes each push allocation-free.
            self.scratch.push(self.indices[source + i]);
        }
        Ok(())
    }

    fn sort(&mut self) -> SortResult<(), E> {
        let minrun = minrun(self.indices.len());
        let mut base = 0;
        let mut remaining = self.indices.len();
        while remaining != 0 {
            self.check()?;
            let mut n = self.count_run(base, remaining)?;
            if n < minrun {
                let force = remaining.min(minrun);
                self.binarysort(base, force, n)?;
                n = force;
            }
            self.found_new_run(n)?;
            let Some(slot) = self.pending.get_mut(self.pending_len) else {
                return Err(SourceSortError::IndexOverflow);
            };
            *slot = Run {
                base,
                len: n,
                power: 0,
            };
            self.pending_len += 1;
            base += n;
            remaining -= n;
        }
        self.merge_force_collapse()
    }

    fn binarysort(&mut self, base: usize, n: usize, mut sorted: usize) -> SortResult<(), E> {
        if sorted == 0 {
            sorted = 1;
        }
        for at in sorted..n {
            let pivot = self.indices[base + at];
            let (mut left, mut right) = (0, at);
            while left < right {
                let mid = left + ((right - left) >> 1);
                if self.lt(pivot, self.indices[base + mid])? {
                    right = mid;
                } else {
                    left = mid + 1;
                }
            }
            self.move_rows(base + left, base + left + 1, at - left)?;
            self.indices[base + left] = pivot;
        }
        Ok(())
    }

    fn count_run(&mut self, base: usize, remaining: usize) -> SortResult<usize, E> {
        let mut n = 1;
        // CPython tries ascending first, including ties.
        while n < remaining {
            if self.lt(self.indices[base + n], self.indices[base + n - 1])? {
                break;
            }
            n += 1;
        }
        if n == remaining {
            return Ok(n);
        }
        if n > 1 {
            if self.lt(self.indices[base], self.indices[base + n - 1])? {
                return Ok(n);
            }
            self.reverse(base, n)?;
        }
        n += 1;
        let mut equal = 0;
        while n < remaining {
            if self.lt(self.indices[base + n], self.indices[base + n - 1])? {
                if equal != 0 {
                    self.reverse(base + n - equal - 1, equal + 1)?;
                    equal = 0;
                }
            } else if self.lt(self.indices[base + n - 1], self.indices[base + n])? {
                break;
            } else {
                equal += 1;
            }
            n += 1;
        }
        if equal != 0 {
            self.reverse(base + n - equal - 1, equal + 1)?;
        }
        self.reverse(base, n)?;
        // A reversed descending run may join a naturally ascending suffix.
        while n < remaining {
            if self.lt(self.indices[base + n], self.indices[base + n - 1])? {
                break;
            }
            n += 1;
        }
        Ok(n)
    }

    fn gallop_left(
        &mut self,
        key: usize,
        buffer: Buffer,
        base: usize,
        n: usize,
        hint: usize,
    ) -> SortResult<usize, E> {
        let (mut last, mut offset) = (0, 1);
        let (mut low, mut high);
        if self.lt(self.key(buffer, base + hint), key)? {
            let max = n - hint;
            while offset < max {
                if self.lt(self.key(buffer, base + hint + offset), key)? {
                    last = offset;
                    offset = offset
                        .checked_mul(2)
                        .and_then(|x| x.checked_add(1))
                        .ok_or(SourceSortError::IndexOverflow)?;
                } else {
                    break;
                }
            }
            offset = offset.min(max);
            low = last + hint + 1;
            high = offset + hint;
        } else {
            let max = hint + 1;
            while offset < max {
                if self.lt(self.key(buffer, base + hint - offset), key)? {
                    break;
                }
                last = offset;
                offset = offset
                    .checked_mul(2)
                    .and_then(|x| x.checked_add(1))
                    .ok_or(SourceSortError::IndexOverflow)?;
            }
            offset = offset.min(max);
            // Original lastofs can be -1; low is its incremented value.
            low = hint + 1 - offset;
            high = hint - last;
        }
        while low < high {
            let mid = low + ((high - low) >> 1);
            if self.lt(self.key(buffer, base + mid), key)? {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        Ok(high)
    }

    fn gallop_right(
        &mut self,
        key: usize,
        buffer: Buffer,
        base: usize,
        n: usize,
        hint: usize,
    ) -> SortResult<usize, E> {
        let (mut last, mut offset) = (0, 1);
        let (mut low, mut high);
        if self.lt(key, self.key(buffer, base + hint))? {
            let max = hint + 1;
            while offset < max {
                if self.lt(key, self.key(buffer, base + hint - offset))? {
                    last = offset;
                    offset = offset
                        .checked_mul(2)
                        .and_then(|x| x.checked_add(1))
                        .ok_or(SourceSortError::IndexOverflow)?;
                } else {
                    break;
                }
            }
            offset = offset.min(max);
            low = hint + 1 - offset;
            high = hint - last;
        } else {
            let max = n - hint;
            while offset < max {
                if self.lt(key, self.key(buffer, base + hint + offset))? {
                    break;
                }
                last = offset;
                offset = offset
                    .checked_mul(2)
                    .and_then(|x| x.checked_add(1))
                    .ok_or(SourceSortError::IndexOverflow)?;
            }
            offset = offset.min(max);
            low = last + hint + 1;
            high = offset + hint;
        }
        while low < high {
            let mid = low + ((high - low) >> 1);
            if self.lt(key, self.key(buffer, base + mid))? {
                high = mid;
            } else {
                low = mid + 1;
            }
        }
        Ok(high)
    }

    fn found_new_run(&mut self, next_len: usize) -> SortResult<(), E> {
        if self.pending_len != 0 {
            let previous = self.pending[self.pending_len - 1];
            let power = powerloop(previous.base, previous.len, next_len, self.indices.len())
                .ok_or(SourceSortError::IndexOverflow)?;
            while self.pending_len > 1 && self.pending[self.pending_len - 2].power > power {
                self.merge_at(self.pending_len - 2)?;
            }
            self.pending[self.pending_len - 1].power = power;
        }
        Ok(())
    }

    fn merge_force_collapse(&mut self) -> SortResult<(), E> {
        while self.pending_len > 1 {
            self.check()?;
            let mut at = self.pending_len - 2;
            if at > 0 && self.pending[at - 1].len < self.pending[at + 1].len {
                at -= 1;
            }
            self.merge_at(at)?;
        }
        Ok(())
    }

    fn merge_at(&mut self, at: usize) -> SortResult<(), E> {
        let (mut a, b) = (self.pending[at], self.pending[at + 1]);
        self.pending[at].len = a
            .len
            .checked_add(b.len)
            .ok_or(SourceSortError::IndexOverflow)?;
        if at + 3 == self.pending_len {
            self.pending[at + 1] = self.pending[at + 2];
        }
        self.pending_len -= 1;
        let skipped = self.gallop_right(self.indices[b.base], Buffer::Rows, a.base, a.len, 0)?;
        a.base += skipped;
        a.len -= skipped;
        if a.len == 0 {
            return Ok(());
        }
        let nb = self.gallop_left(
            self.indices[a.base + a.len - 1],
            Buffer::Rows,
            b.base,
            b.len,
            b.len - 1,
        )?;
        if nb == 0 {
            return Ok(());
        }
        if a.len <= nb {
            self.merge_lo(a.base, a.len, b.base, nb)
        } else {
            self.merge_hi(a.base, a.len, b.base, nb)
        }
    }

    fn merge_lo(
        &mut self,
        base_a: usize,
        mut na: usize,
        base_b: usize,
        mut nb: usize,
    ) -> SortResult<(), E> {
        self.fill_scratch(base_a, na)?;
        let (mut a, mut b, mut dest) = (0, base_b, base_a);
        self.indices[dest] = self.indices[b];
        dest += 1;
        b += 1;
        nb -= 1;
        if nb == 0 {
            return self.copy_scratch(a, dest, na);
        }
        if na == 1 {
            self.move_rows(b, dest, nb)?;
            self.indices[dest + nb] = self.scratch[a];
            return Ok(());
        }
        let mut min_gallop = self.min_gallop;
        loop {
            let (mut a_count, mut b_count) = (0, 0);
            loop {
                if self.lt(self.indices[b], self.scratch[a])? {
                    self.indices[dest] = self.indices[b];
                    dest += 1;
                    b += 1;
                    b_count += 1;
                    a_count = 0;
                    nb -= 1;
                    if nb == 0 {
                        return self.copy_scratch(a, dest, na);
                    }
                    if b_count >= min_gallop {
                        break;
                    }
                } else {
                    self.indices[dest] = self.scratch[a];
                    dest += 1;
                    a += 1;
                    a_count += 1;
                    b_count = 0;
                    na -= 1;
                    if na == 1 {
                        self.move_rows(b, dest, nb)?;
                        self.indices[dest + nb] = self.scratch[a];
                        return Ok(());
                    }
                    if a_count >= min_gallop {
                        break;
                    }
                }
            }
            min_gallop = min_gallop
                .checked_add(1)
                .ok_or(SourceSortError::IndexOverflow)?;
            loop {
                min_gallop -= usize::from(min_gallop > 1);
                self.min_gallop = min_gallop;
                a_count = self.gallop_right(self.indices[b], Buffer::Scratch, a, na, 0)?;
                if a_count != 0 {
                    self.copy_scratch(a, dest, a_count)?;
                    dest += a_count;
                    a += a_count;
                    na -= a_count;
                    if na == 1 {
                        self.move_rows(b, dest, nb)?;
                        self.indices[dest + nb] = self.scratch[a];
                        return Ok(());
                    }
                    // Original source explicitly permits inconsistent less.
                    if na == 0 {
                        return Ok(());
                    }
                }
                self.indices[dest] = self.indices[b];
                dest += 1;
                b += 1;
                nb -= 1;
                if nb == 0 {
                    return self.copy_scratch(a, dest, na);
                }
                b_count = self.gallop_left(self.scratch[a], Buffer::Rows, b, nb, 0)?;
                if b_count != 0 {
                    self.move_rows(b, dest, b_count)?;
                    dest += b_count;
                    b += b_count;
                    nb -= b_count;
                    if nb == 0 {
                        return self.copy_scratch(a, dest, na);
                    }
                }
                self.indices[dest] = self.scratch[a];
                dest += 1;
                a += 1;
                na -= 1;
                if na == 1 {
                    self.move_rows(b, dest, nb)?;
                    self.indices[dest + nb] = self.scratch[a];
                    return Ok(());
                }
                if a_count < MIN_GALLOP && b_count < MIN_GALLOP {
                    break;
                }
            }
            min_gallop = min_gallop
                .checked_add(1)
                .ok_or(SourceSortError::IndexOverflow)?;
            self.min_gallop = min_gallop;
        }
    }

    fn merge_hi(
        &mut self,
        base_a: usize,
        mut na: usize,
        base_b: usize,
        mut nb: usize,
    ) -> SortResult<(), E> {
        self.fill_scratch(base_b, nb)?;
        // Exclusive cursors avoid C's transient pointers before array start.
        let (mut a, mut b, mut dest) = (base_a + na, nb, base_b + nb);
        dest -= 1;
        a -= 1;
        self.indices[dest] = self.indices[a];
        na -= 1;
        if na == 0 {
            return self.copy_scratch(0, dest - nb, nb);
        }
        if nb == 1 {
            self.move_rows(a - na, dest - na, na)?;
            self.indices[dest - na - 1] = self.scratch[b - 1];
            return Ok(());
        }
        let mut min_gallop = self.min_gallop;
        loop {
            let (mut a_count, mut b_count) = (0, 0);
            loop {
                if self.lt(self.scratch[b - 1], self.indices[a - 1])? {
                    dest -= 1;
                    a -= 1;
                    self.indices[dest] = self.indices[a];
                    a_count += 1;
                    b_count = 0;
                    na -= 1;
                    if na == 0 {
                        return self.copy_scratch(0, dest - nb, nb);
                    }
                    if a_count >= min_gallop {
                        break;
                    }
                } else {
                    dest -= 1;
                    b -= 1;
                    self.indices[dest] = self.scratch[b];
                    b_count += 1;
                    a_count = 0;
                    nb -= 1;
                    if nb == 1 {
                        self.move_rows(a - na, dest - na, na)?;
                        self.indices[dest - na - 1] = self.scratch[b - 1];
                        return Ok(());
                    }
                    if b_count >= min_gallop {
                        break;
                    }
                }
            }
            min_gallop = min_gallop
                .checked_add(1)
                .ok_or(SourceSortError::IndexOverflow)?;
            loop {
                min_gallop -= usize::from(min_gallop > 1);
                self.min_gallop = min_gallop;
                let split =
                    self.gallop_right(self.scratch[b - 1], Buffer::Rows, base_a, na, na - 1)?;
                a_count = na - split;
                if a_count != 0 {
                    a -= a_count;
                    dest -= a_count;
                    self.move_rows(a, dest, a_count)?;
                    na -= a_count;
                    if na == 0 {
                        return self.copy_scratch(0, dest - nb, nb);
                    }
                }
                dest -= 1;
                b -= 1;
                self.indices[dest] = self.scratch[b];
                nb -= 1;
                if nb == 1 {
                    self.move_rows(a - na, dest - na, na)?;
                    self.indices[dest - na - 1] = self.scratch[b - 1];
                    return Ok(());
                }
                let split =
                    self.gallop_left(self.indices[a - 1], Buffer::Scratch, 0, nb, nb - 1)?;
                b_count = nb - split;
                if b_count != 0 {
                    b -= b_count;
                    dest -= b_count;
                    self.copy_scratch(b, dest, b_count)?;
                    nb -= b_count;
                    if nb == 1 {
                        self.move_rows(a - na, dest - na, na)?;
                        self.indices[dest - na - 1] = self.scratch[b - 1];
                        return Ok(());
                    }
                    if nb == 0 {
                        return Ok(());
                    }
                }
                dest -= 1;
                a -= 1;
                self.indices[dest] = self.indices[a];
                na -= 1;
                if na == 0 {
                    return self.copy_scratch(0, dest - nb, nb);
                }
                if a_count < MIN_GALLOP && b_count < MIN_GALLOP {
                    break;
                }
            }
            min_gallop = min_gallop
                .checked_add(1)
                .ok_or(SourceSortError::IndexOverflow)?;
            self.min_gallop = min_gallop;
        }
    }
}

fn minrun(mut n: usize) -> usize {
    let mut remainder = 0;
    while n >= MAX_MINRUN {
        remainder |= n & 1;
        n >>= 1;
    }
    n + remainder
}

fn powerloop(start: usize, first: usize, second: usize, n: usize) -> Option<usize> {
    let mut power = 0;
    let mut a = start.checked_mul(2)?.checked_add(first)?;
    let mut b = a.checked_add(first)?.checked_add(second)?;
    loop {
        power += 1;
        if a >= n {
            a -= n;
            b -= n;
        } else if b >= n {
            return Some(power);
        }
        a = a.checked_mul(2)?;
        b = b.checked_mul(2)?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn source_sort_helpers_minrun_and_powersort_midpoints() {
        assert_eq!(
            [0, 1, 31, 32, 63, 64, 65, 127, 128].map(minrun),
            [0, 1, 31, 32, 63, 32, 33, 64, 32]
        );
        assert_eq!(powerloop(0, 64, 64, 128), Some(1));
        assert_eq!(powerloop(0, 32, 32, 256), Some(3));
        assert_eq!(powerloop(32, 32, 64, 256), Some(2));
    }

    #[test]
    fn source_sort_callback_error_keeps_original_identity() {
        let marker = Rc::new(());
        let mut ids = [0, 1];
        let error = source_sort_indices(&mut ids, false, |_, _| Err(marker.clone()), || Ok(()));
        match error {
            Err(SourceSortError::Callback(actual)) => assert!(Rc::ptr_eq(&marker, &actual)),
            _ => panic!("comparison did not preserve callback error"),
        }
    }

    #[test]
    fn source_sort_cancel_stops_before_next_comparison() {
        let comparisons = Cell::new(0);
        let cancelled = Cell::new(false);
        let mut ids = [7, 2, 8, 1, 9, 0, 4, 3];
        let result = source_sort_indices(
            &mut ids,
            false,
            |a, b| {
                comparisons.set(comparisons.get() + 1);
                if comparisons.get() == 3 {
                    cancelled.set(true);
                }
                Ok(a < b)
            },
            || {
                if cancelled.get() {
                    Err("cancelled")
                } else {
                    Ok(())
                }
            },
        );
        assert!(matches!(
            result,
            Err(SourceSortError::Callback("cancelled"))
        ));
        assert_eq!(comparisons.get(), 3);
    }

    #[test]
    fn source_sort_actual_reservation_rejects_capacity_overflow() {
        let mut scratch = vec![1, 2];
        let result = reserve_sort_scratch::<()>(&mut scratch, usize::MAX);
        assert!(matches!(result, Err(SourceSortError::Allocation)));
        assert_eq!(scratch, [1, 2]);
    }
}
