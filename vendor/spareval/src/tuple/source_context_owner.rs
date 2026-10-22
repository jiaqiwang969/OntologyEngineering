//! The binding portion of the real QueryContext retained by FrozenBindings.
//! Local cells stay a snapshot. The owner has separate live bindings and shares
//! the immutable initial mapping. Graph/time/bnode state is not supplied here.
use super::{InternalTuple, SolutionValue};
use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

pub(super) struct SourceContextOwner<T> {
    // Both stored tuples are ownerless: a context never owns itself through a
    // frozen row. Cell values retain their existing RDF/cache/error objects.
    bindings: RefCell<InternalTuple<T>>,
    initial: Rc<InternalTuple<T>>,
}

impl<T> fmt::Debug for SourceContextOwner<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SourceContextOwner")
            .field(
                "binding_count",
                &self.bindings.borrow().source_local_order().len(),
            )
            .field("initial_count", &self.initial.source_local_order().len())
            .finish_non_exhaustive()
    }
}

impl<T> InternalTuple<T> {
    pub(crate) fn source_context_owner_same(&self, other: &Self) -> bool {
        match (&self.source_context_owner, &other.source_context_owner) {
            (Some(left), Some(right)) => Rc::ptr_eq(left, right),
            _ => false,
        }
    }

    pub(crate) fn source_context_initial_shared(&self, other: &Self) -> bool {
        match (&self.source_context_owner, &other.source_context_owner) {
            (Some(left), Some(right)) => Rc::ptr_eq(&left.initial, &right.initial),
            _ => false,
        }
    }

    pub(crate) fn source_has_context_owner(&self) -> bool {
        self.source_context_owner.is_some()
    }

    pub(super) fn source_context_initial_has(&self, slot: usize) -> Option<bool> {
        self.source_context_owner
            .as_ref()
            .map(|owner| owner.initial.contains_cell(slot))
    }
}

impl<T: Clone> InternalTuple<T> {
    fn source_without_context(&self) -> Self {
        let mut result = self.source_fresh_copy();
        result.source_context_owner = None;
        result
    }

    /// QueryContext(bindings, initBindings), including unconditional initial
    /// overwrite. This is not QueryContext.__setitem__'s AlreadyBound check.
    pub(crate) fn source_context_from_bindings(&self, initial: Rc<Self>) -> Self {
        let initial = if initial.source_context_owner.is_some() {
            Rc::new(initial.source_without_context())
        } else {
            initial
        };
        let mut bindings = self.source_merge(&initial);
        bindings.source_context_owner = None;
        let owner = Rc::new(SourceContextOwner {
            bindings: RefCell::new(bindings.source_without_context()),
            initial,
        });
        bindings.source_context_owner = Some(owner);
        bindings
    }

    /// FrozenBindings(ctx, local): local cells do not become ctx.bindings.
    pub(crate) fn source_with_context_of(&self, context: &Self) -> Self {
        let mut result = self.source_fresh_copy();
        result.source_context_owner = context.source_context_owner.clone();
        result
    }

    pub(crate) fn source_context_bindings(&self) -> Option<Self> {
        self.source_context_owner
            .as_ref()
            .map(|owner| owner.bindings.borrow().clone())
    }

    /// FrozenBindings.__getitem__: the owner's live bindings are deliberately
    /// absent from this lookup. ExplicitNone/ReturnedError mask initial cells.
    pub(crate) fn source_context_lookup(&self, slot: usize) -> Option<SolutionValue<T>> {
        self.cell(slot).cloned().or_else(|| {
            self.source_context_owner
                .as_ref()
                .and_then(|owner| owner.initial.cell(slot).cloned())
        })
    }

    /// Builtin_EXISTS uses the frozen row's owner; join/OPTIONAL use their
    /// explicit call context instead (source_thaw). Missing provenance remains
    /// missing instead of reconstructing an owner from the row's local cells.
    pub(crate) fn source_thaw_owned(&self) -> Option<Self> {
        let owner = self.source_context_owner.as_ref()?;
        let bindings = if self.source_local_order().is_empty() {
            owner.bindings.borrow().clone()
        } else {
            self.source_without_context()
        };
        Some(bindings.source_context_from_bindings(Rc::clone(&owner.initial)))
    }

    /// QueryContext.clean calls clone([]); the false empty argument falls back
    /// to that context's current bindings, just as clone/thaw do.
    pub(crate) fn source_context_clean(&self) -> Option<Self> {
        let owner = self.source_context_owner.as_ref()?;
        let bindings = owner.bindings.borrow().clone();
        Some(bindings.source_context_from_bindings(Rc::clone(&owner.initial)))
    }

    pub(super) fn source_context_solution(&self, columns: Option<&[usize]>) -> Option<Self> {
        let owner = self.source_context_owner.as_ref()?;
        let bindings = owner.bindings.borrow();
        let result = match columns {
            Some(columns) if !columns.is_empty() => bindings.source_project(columns),
            _ => bindings.source_fresh_copy(),
        };
        Some(result.source_with_context_of(self))
    }

    pub(super) fn source_context_thaw(&self, frozen: &Self) -> Option<Self> {
        let owner = self.source_context_owner.as_ref()?;
        let bindings = if frozen.source_local_order().is_empty() {
            owner.bindings.borrow().clone()
        } else {
            frozen.source_without_context()
        };
        Some(bindings.source_context_from_bindings(Rc::clone(&owner.initial)))
    }

    pub(super) fn source_context_assign_by<E>(
        &mut self,
        slot: usize,
        value: SolutionValue<T>,
        mut equal: impl FnMut(&SolutionValue<T>, &SolutionValue<T>) -> Result<bool, E>,
    ) -> Result<Option<bool>, E> {
        let Some(owner) = self.source_context_owner.clone() else {
            return Ok(None);
        };
        // Do not hold a RefCell borrow while invoking the real equality/codec
        // callback. A callback failure leaves both local and owner unchanged.
        let previous = owner.bindings.borrow().cell(slot).cloned();
        if let Some(previous) = previous {
            if !equal(&previous, &value)? {
                return Ok(Some(false));
            }
        }
        owner.bindings.borrow_mut().set_cell(slot, value.clone());
        self.set_cell(slot, value);
        Ok(Some(true))
    }
}
