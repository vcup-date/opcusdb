//! The [`World`]: the container that ties entities, component stores, and
//! resources together.
//!
//! See `CORE_SPEC.md` §5. The world owns the entity allocator, one type-erased
//! [`SparseSet`] per registered component type, and a set of singleton
//! *resources*. It is the unit that gets snapshotted (§9), everything mutable
//! sim state lives here, nothing in statics.
//!
//! Determinism: component stores live in a `Vec` indexed by registration-order
//! [`ComponentId`], so iterating them (e.g. on `despawn`) is deterministic. The
//! `TypeId -> ComponentId` map is used only for lookup, never iterated in sim.

use crate::component::{ComponentId, ErasedResource, ErasedStore};
use crate::entity::{Entities, EntityId};
use crate::storage::SparseSet;
use core::any::{Any, TypeId};
use std::collections::HashMap;

/// The ECS world. Holds all entities, components, and resources.
///
/// Components and resources must be `Clone`, which makes the whole `World`
/// deep-cloneable, that is what lets it be snapshotted and rolled back by the
/// Timeline (`CORE_SPEC.md` §9).
#[derive(Default)]
pub struct World {
    entities: Entities,
    /// Lookup-only: maps a component's `TypeId` to its dense store index.
    type_to_id: HashMap<TypeId, ComponentId>,
    /// One store per component type, indexed by `ComponentId.0`.
    stores: Vec<Box<dyn ErasedStore>>,
    /// Singleton resources keyed by type (config, clock, rng, ...).
    resources: HashMap<TypeId, Box<dyn ErasedResource>>,
}

impl Clone for World {
    fn clone(&self) -> Self {
        Self {
            entities: self.entities.clone(),
            type_to_id: self.type_to_id.clone(),
            stores: self.stores.iter().map(|s| s.dyn_clone()).collect(),
            resources: self
                .resources
                .iter()
                // Deref to the `dyn` so the blanket impl doesn't resolve on `&Box`.
                .map(|(k, v)| (*k, (**v).res_clone()))
                .collect(),
        }
    }
}

impl World {
    /// A fresh, empty world.
    pub fn new() -> Self {
        Self::default()
    }

    // --- entities ---------------------------------------------------------

    /// Allocate a new live entity.
    pub fn spawn(&mut self) -> EntityId {
        self.entities.spawn()
    }

    /// Despawn an entity, removing all of its components first. Returns `false`
    /// if it was already dead. This is where the "a despawned entity's components
    /// are cleaned up" contract (see `storage` tests) is actually fulfilled.
    pub fn despawn(&mut self, id: EntityId) -> bool {
        if !self.entities.is_alive(id) {
            return false;
        }
        for store in &mut self.stores {
            store.erased_remove(id);
        }
        self.entities.despawn(id)
    }

    /// Whether `id` is a currently-live entity.
    #[inline]
    pub fn is_alive(&self, id: EntityId) -> bool {
        self.entities.is_alive(id)
    }

    /// Number of live entities.
    #[inline]
    pub fn entity_count(&self) -> u32 {
        self.entities.len()
    }

    /// Number of distinct component types that have been registered.
    #[inline]
    pub fn component_type_count(&self) -> usize {
        self.stores.len()
    }

    /// The mutation version of component `T`'s store (0 if never registered).
    /// Bumped on any mutation, the basis of memoized [`Select`](crate::select::Select).
    pub fn component_version<T: 'static>(&self) -> u64 {
        self.version_of(TypeId::of::<T>())
    }

    /// Version by `TypeId` (used by `Select` whose deps are type-erased).
    pub(crate) fn version_of(&self, ty: TypeId) -> u64 {
        self.type_to_id
            .get(&ty)
            .map_or(0, |id| self.stores[id.0 as usize].version())
    }

    // --- components -------------------------------------------------------

    /// Ensure a store exists for `T`, returning its id. Registration order is
    /// deterministic given a deterministic sequence of first-insertions.
    fn register<T: Clone + 'static>(&mut self) -> ComponentId {
        let ty = TypeId::of::<T>();
        if let Some(id) = self.type_to_id.get(&ty) {
            return *id;
        }
        let id = ComponentId(self.stores.len() as u32);
        self.stores.push(Box::new(SparseSet::<T>::new()));
        self.type_to_id.insert(ty, id);
        id
    }

    /// Mutable access to the store for `T`, registering it if needed.
    fn store_mut<T: Clone + 'static>(&mut self) -> &mut SparseSet<T> {
        let id = self.register::<T>();
        self.stores[id.0 as usize]
            .as_any_mut()
            .downcast_mut::<SparseSet<T>>()
            .expect("store type matches its ComponentId")
    }

    /// Shared access to the store for `T`, if it has ever been registered.
    pub(crate) fn store<T: 'static>(&self) -> Option<&SparseSet<T>> {
        let id = self.type_to_id.get(&TypeId::of::<T>())?;
        Some(
            self.stores[id.0 as usize]
                .as_any()
                .downcast_ref::<SparseSet<T>>()
                .expect("store type matches its ComponentId"),
        )
    }

    /// Insert (or overwrite) component `T` on `id`, returning the previous value
    /// for that id if any. The entity must be live.
    pub fn insert<T: Clone + 'static>(&mut self, id: EntityId, value: T) -> Option<T> {
        debug_assert!(
            self.entities.is_alive(id),
            "insert on a dead entity {id:?}"
        );
        self.store_mut::<T>().insert(id, value)
    }

    /// Whether `id` has a component of type `T`.
    pub fn has<T: 'static>(&self, id: EntityId) -> bool {
        self.store::<T>().is_some_and(|s| s.contains(id))
    }

    /// Shared access to `id`'s component `T`.
    pub fn get<T: 'static>(&self, id: EntityId) -> Option<&T> {
        self.store::<T>()?.get(id)
    }

    /// Mutable access to `id`'s component `T`.
    pub fn get_mut<T: Clone + 'static>(&mut self, id: EntityId) -> Option<&mut T> {
        // Avoid registering a store just to fail a lookup.
        if !self.type_to_id.contains_key(&TypeId::of::<T>()) {
            return None;
        }
        self.store_mut::<T>().get_mut(id)
    }

    /// Remove and return `id`'s component `T`, if present.
    pub fn remove<T: Clone + 'static>(&mut self, id: EntityId) -> Option<T> {
        if !self.type_to_id.contains_key(&TypeId::of::<T>()) {
            return None;
        }
        self.store_mut::<T>().remove(id)
    }

    /// Apply `f` to every `(id, &mut T)` in ascending entity order, a safe,
    /// deterministic mutable iteration over a single component (no `get_mut`
    /// dance). No-op if `T` was never registered.
    pub fn for_each_mut<T: Clone + 'static>(&mut self, f: impl FnMut(EntityId, &mut T)) {
        if !self.type_to_id.contains_key(&TypeId::of::<T>()) {
            return;
        }
        self.store_mut::<T>().for_each_ordered_mut(f);
    }

    // --- resources --------------------------------------------------------

    /// Insert a singleton resource, returning the previous one if present.
    pub fn insert_resource<R: Clone + 'static>(&mut self, value: R) -> Option<R> {
        let prev = self.resources.insert(TypeId::of::<R>(), Box::new(value))?;
        // Upcast dyn ErasedResource -> dyn Any, then downcast out by value.
        (prev as Box<dyn Any>).downcast::<R>().ok().map(|b| *b)
    }

    /// Shared access to resource `R`.
    pub fn resource<R: 'static>(&self) -> Option<&R> {
        let b = self.resources.get(&TypeId::of::<R>())?;
        (b.as_ref() as &dyn Any).downcast_ref::<R>()
    }

    /// Mutable access to resource `R`.
    pub fn resource_mut<R: 'static>(&mut self) -> Option<&mut R> {
        let b = self.resources.get_mut(&TypeId::of::<R>())?;
        (b.as_mut() as &mut dyn Any).downcast_mut::<R>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    struct Position {
        x: i32,
        y: i32,
    }
    #[derive(Clone, Debug, PartialEq)]
    struct Health(i32);

    #[test]
    fn insert_get_remove_components() {
        let mut w = World::new();
        let e = w.spawn();
        assert!(w.insert(e, Position { x: 1, y: 2 }).is_none());
        assert!(w.insert(e, Health(100)).is_none());
        assert_eq!(w.get::<Position>(e), Some(&Position { x: 1, y: 2 }));
        assert_eq!(w.get::<Health>(e), Some(&Health(100)));
        assert_eq!(w.component_type_count(), 2);

        *w.get_mut::<Health>(e).unwrap() = Health(50);
        assert_eq!(w.get::<Health>(e), Some(&Health(50)));

        assert_eq!(w.remove::<Health>(e), Some(Health(50)));
        assert!(!w.has::<Health>(e));
        assert!(w.has::<Position>(e));
    }

    #[test]
    fn despawn_clears_all_components() {
        let mut w = World::new();
        let e = w.spawn();
        w.insert(e, Position { x: 9, y: 9 });
        w.insert(e, Health(1));
        assert!(w.despawn(e));

        assert!(!w.is_alive(e));
        // The previously-flagged contract gap: components are gone after despawn.
        assert_eq!(w.get::<Position>(e), None);
        assert_eq!(w.get::<Health>(e), None);
        assert_eq!(w.entity_count(), 0);
    }

    #[test]
    fn reused_slot_starts_clean() {
        let mut w = World::new();
        let a = w.spawn();
        w.insert(a, Health(7));
        w.despawn(a);
        let b = w.spawn(); // reuses a's slot
        assert_eq!(a.index(), b.index());
        // No leakage from the previous occupant.
        assert!(!w.has::<Health>(b));
        assert_eq!(w.get::<Health>(a), None);
    }

    #[test]
    fn get_missing_type_does_not_register() {
        let mut w = World::new();
        let e = w.spawn();
        // Querying a never-inserted type must not create a store.
        assert_eq!(w.get::<Position>(e), None);
        assert_eq!(w.get_mut::<Position>(e), None);
        assert_eq!(w.remove::<Position>(e), None);
        assert_eq!(w.component_type_count(), 0);
    }

    #[test]
    fn resources_roundtrip() {
        #[derive(Clone)]
        struct Config {
            tick_hz: u32,
        }
        let mut w = World::new();
        assert!(w.resource::<Config>().is_none());
        assert!(w.insert_resource(Config { tick_hz: 20 }).is_none());
        assert_eq!(w.resource::<Config>().unwrap().tick_hz, 20);
        w.resource_mut::<Config>().unwrap().tick_hz = 60;
        assert_eq!(w.resource::<Config>().unwrap().tick_hz, 60);
    }

    #[test]
    fn snapshot_is_a_deep_independent_copy() {
        #[derive(Clone)]
        struct Tick(u32);
        let mut w = World::new();
        let e = w.spawn();
        w.insert(e, Position { x: 1, y: 2 });
        w.insert(e, Health(100));
        w.insert_resource(Tick(5));

        // Take a snapshot, then mutate the original heavily.
        let snap = w.clone();
        w.get_mut::<Health>(e).unwrap().0 = 0;
        w.get_mut::<Position>(e).unwrap().x = 999;
        w.resource_mut::<Tick>().unwrap().0 = 42;
        let e2 = w.spawn();
        w.insert(e2, Health(7));

        // The snapshot is untouched by changes to the original.
        assert_eq!(snap.get::<Health>(e), Some(&Health(100)));
        assert_eq!(snap.get::<Position>(e), Some(&Position { x: 1, y: 2 }));
        assert_eq!(snap.resource::<Tick>().unwrap().0, 5);
        assert_eq!(snap.entity_count(), 1, "the later spawn isn't in the snapshot");
        // And the original kept its changes.
        assert_eq!(w.get::<Health>(e), Some(&Health(0)));
        assert_eq!(w.entity_count(), 2);
    }
}
