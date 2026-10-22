//! Entities as the deduplication module passes them around.
//!
//! The original works on Python dicts (`Dict[str, Any]`) and keeps references to the caller's
//! objects: duplicate candidates, groups, clusters and merge operations hold the very dicts that
//! were passed in, and an entity without an `"id"` is told apart by object identity
//! (`id(entity)`). Here an entity is a [`serde_json::Value`] behind an [`EntityRef`]
//! (`Arc<Value>`): cloning the `Arc` keeps the identity and [`py_id`] is what Python's `id()`
//! gives (the object's address).
//!
//! Functions accept any slice of [`AsEntity`] items: plain `Value`s (each call wraps them in new
//! references, so identities are per call) or `EntityRef`s (identities are kept, so results
//! point at the caller's references, as in Python).

use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

/// A shared reference to an entity (a Python object reference to a dict).
pub type EntityRef = Arc<Value>;

/// A value usable as an entity argument.
pub trait AsEntity {
    /// The entity value.
    fn entity(&self) -> &Value;
    /// A reference carrying this entity's identity (a new one for a plain value).
    fn entity_ref(&self) -> EntityRef;
}

impl AsEntity for Value {
    fn entity(&self) -> &Value {
        self
    }

    fn entity_ref(&self) -> EntityRef {
        Arc::new(self.clone())
    }
}

impl AsEntity for EntityRef {
    fn entity(&self) -> &Value {
        self
    }

    fn entity_ref(&self) -> EntityRef {
        Arc::clone(self)
    }
}

impl<T: AsEntity + ?Sized> AsEntity for &T {
    fn entity(&self) -> &Value {
        (**self).entity()
    }

    fn entity_ref(&self) -> EntityRef {
        (**self).entity_ref()
    }
}

/// References for a list of entity arguments (one per item, made once per call).
pub fn entity_refs<E: AsEntity>(entities: &[E]) -> Vec<EntityRef> {
    entities.iter().map(AsEntity::entity_ref).collect()
}

/// `id(entity)`: the identity of the referenced object (its address).
pub fn py_id(entity: &EntityRef) -> u64 {
    Arc::as_ptr(entity) as usize as u64
}

/// `a is b or a == b`, the test Python's `in` and `list.remove` apply to list items.
pub(crate) fn same_or_equal(a: &EntityRef, b: &EntityRef) -> bool {
    Arc::ptr_eq(a, b) || crate::pyops::eq(a, b)
}

/// Serde helpers for `EntityRef` fields (serialized as the plain entity value).
pub(crate) mod serde_ref {
    use super::*;

    pub(crate) fn serialize<S: Serializer>(value: &EntityRef, s: S) -> Result<S::Ok, S::Error> {
        value.as_ref().serialize(s)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<EntityRef, D::Error> {
        Value::deserialize(d).map(Arc::new)
    }
}

/// Serde helpers for `Vec<EntityRef>` fields.
pub(crate) mod serde_refs {
    use super::*;

    pub(crate) fn serialize<S: Serializer>(
        values: &[EntityRef],
        s: S,
    ) -> Result<S::Ok, S::Error> {
        let plain: Vec<&Value> = values.iter().map(|v| v.as_ref()).collect();
        plain.serialize(s)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<Vec<EntityRef>, D::Error> {
        Ok(Vec::<Value>::deserialize(d)?
            .into_iter()
            .map(Arc::new)
            .collect())
    }
}

/// Serde helpers for `Option<EntityRef>` fields.
pub(crate) mod serde_opt_ref {
    use super::*;

    pub(crate) fn serialize<S: Serializer>(
        value: &Option<EntityRef>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        value.as_deref().serialize(s)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<Option<EntityRef>, D::Error> {
        Ok(Option::<Value>::deserialize(d)?.map(Arc::new))
    }
}
