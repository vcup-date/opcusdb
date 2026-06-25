//! Component identity and type-erased storage plumbing.
//!
//! See `CORE_SPEC.md` §5. A [`ComponentId`] is a dense, registration-order index
//! the [`World`](crate::world::World) assigns to each component type, so it can
//! keep stores in a `Vec` and iterate them deterministically. [`ErasedStore`]
//! lets the world hold differently-typed `SparseSet<T>`s behind one trait object
//! while still recovering the concrete type via `Any` downcasting (no `unsafe`).

use crate::entity::EntityId;
use crate::storage::SparseSet;
use core::any::Any;

/// A dense identifier for a component type, assigned in registration order.
/// Stable within a `World`'s lifetime and used to index its store vector.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ComponentId(pub(crate) u32);

impl ComponentId {
    /// The raw index (its position in the world's store vector).
    #[inline]
    pub fn index(self) -> u32 {
        self.0
    }
}

/// Type-erased view over a `SparseSet<T>` so the world can store, drop-on-despawn,
/// clone, and downcast stores without naming `T`. `pub(crate)` — an implementation
/// detail. Components must be `Clone` so the `World` can be snapshotted (§9).
pub(crate) trait ErasedStore: Any {
    /// Remove this entity's value if present; returns whether something was removed.
    fn erased_remove(&mut self, id: EntityId) -> bool;
    /// The store's mutation version (change detection).
    fn version(&self) -> u64;
    /// Deep-clone the store behind the trait object (for `World` snapshots).
    fn dyn_clone(&self) -> Box<dyn ErasedStore>;
    /// Upcast for downcasting back to the concrete `SparseSet<T>`.
    fn as_any(&self) -> &dyn Any;
    /// Mutable upcast.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T: Clone + 'static> ErasedStore for SparseSet<T> {
    #[inline]
    fn erased_remove(&mut self, id: EntityId) -> bool {
        self.remove(id).is_some()
    }
    #[inline]
    fn version(&self) -> u64 {
        SparseSet::version(self)
    }
    #[inline]
    fn dyn_clone(&self) -> Box<dyn ErasedStore> {
        Box::new(self.clone())
    }
    #[inline]
    fn as_any(&self) -> &dyn Any {
        self
    }
    #[inline]
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Type-erased, cloneable singleton resource. Resources must be `Clone` so the
/// `World` snapshot captures them too. The `Any` supertrait lets callers upcast
/// `dyn ErasedResource -> dyn Any` for downcasting (stable trait upcasting).
pub(crate) trait ErasedResource: Any {
    /// Deep-clone the resource behind the trait object.
    fn res_clone(&self) -> Box<dyn ErasedResource>;
}

impl<T: Clone + 'static> ErasedResource for T {
    #[inline]
    fn res_clone(&self) -> Box<dyn ErasedResource> {
        Box::new(self.clone())
    }
}
