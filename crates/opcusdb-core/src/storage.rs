//! Sparse-set component storage.
//!
//! See `CORE_SPEC.md` §5. A [`SparseSet`] gives O(1) insert/get/remove with a
//! dense, contiguous value array (cache-friendly iteration) while remaining
//! stable across churn. It stores one component type for many entities.
//!
//! Note: the dense order is *insertion/swap-remove* order, not entity order.
//! Deterministic sim iterates entities in ascending index — that ordered view is
//! the query layer's job (a later module), not this storage's.

use crate::entity::EntityId;

/// Sentinel meaning "this slot has no dense entry".
const NONE: u32 = u32::MAX;

/// A sparse set mapping live entities to values of type `T`.
#[derive(Clone, Debug)]
pub struct SparseSet<T> {
    /// `sparse[entity.index]` -> dense slot, or [`NONE`].
    sparse: Vec<u32>,
    /// `dense[slot]` -> the entity index stored there (back-map for swap-remove).
    dense: Vec<u32>,
    /// `gens[slot]` -> the entity generation, to reject stale ids.
    gens: Vec<u32>,
    /// `data[slot]` -> the component value. Parallel to `dense`/`gens`.
    data: Vec<T>,
    /// Bumped on every mutation (change detection for memoized `select`). Reads
    /// don't bump; `get_mut`/`iter_mut` bump conservatively (caller may write).
    version: u64,
}

impl<T> Default for SparseSet<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> SparseSet<T> {
    /// An empty sparse set.
    pub fn new() -> Self {
        Self {
            sparse: Vec::new(),
            dense: Vec::new(),
            gens: Vec::new(),
            data: Vec::new(),
            version: 0,
        }
    }

    /// A monotonically-increasing stamp bumped on every mutation. Two reads with
    /// the same version observed no intervening change.
    #[inline]
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Number of stored components.
    #[inline]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Whether nothing is stored.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Resolve `id` to its dense slot if present and not stale.
    #[inline]
    fn slot_of(&self, id: EntityId) -> Option<usize> {
        let slot = *self.sparse.get(id.index() as usize)?;
        if slot == NONE {
            return None;
        }
        // Validate generation: a freed-then-reused slot must not surface stale data.
        if self.gens[slot as usize] == id.gen() {
            Some(slot as usize)
        } else {
            None
        }
    }

    /// Whether a value is stored for `id`.
    #[inline]
    pub fn contains(&self, id: EntityId) -> bool {
        self.slot_of(id).is_some()
    }

    /// Shared access to the value for `id`, if any.
    #[inline]
    pub fn get(&self, id: EntityId) -> Option<&T> {
        self.slot_of(id).map(|s| &self.data[s])
    }

    /// Mutable access to the value for `id`, if any. Bumps the version (the caller
    /// may write through the returned reference).
    #[inline]
    pub fn get_mut(&mut self, id: EntityId) -> Option<&mut T> {
        let slot = self.slot_of(id)?;
        self.version += 1;
        Some(&mut self.data[slot])
    }

    /// Insert or overwrite the value for `id`. Returns the previous value if one
    /// was present *for this exact id*. A stale entry in the same slot (older
    /// generation) is replaced and reported as `None`.
    pub fn insert(&mut self, id: EntityId, value: T) -> Option<T> {
        self.version += 1;
        let idx = id.index() as usize;
        if idx >= self.sparse.len() {
            self.sparse.resize(idx + 1, NONE);
        }
        let slot = self.sparse[idx];
        if slot != NONE {
            let slot = slot as usize;
            if self.gens[slot] == id.gen() {
                // Same live entity: overwrite in place, return old value.
                return Some(core::mem::replace(&mut self.data[slot], value));
            }
            // Stale occupant of this slot: refresh generation and value.
            self.gens[slot] = id.gen();
            self.data[slot] = value;
            self.dense[slot] = id.index();
            return None;
        }
        // Fresh: push to the dense arrays.
        let slot = self.data.len() as u32;
        self.sparse[idx] = slot;
        self.dense.push(id.index());
        self.gens.push(id.gen());
        self.data.push(value);
        None
    }

    /// Remove and return the value for `id`, if present. Uses swap-remove: the
    /// last dense element is moved into the hole and its sparse pointer patched.
    pub fn remove(&mut self, id: EntityId) -> Option<T> {
        let slot = self.slot_of(id)?;
        self.version += 1;
        let last = self.data.len() - 1;
        // Move the tail element into `slot` (no-op if it already is the tail).
        self.data.swap(slot, last);
        self.dense.swap(slot, last);
        self.gens.swap(slot, last);
        // Patch the sparse pointer of whatever now lives at `slot`.
        let moved_index = self.dense[slot];
        self.sparse[moved_index as usize] = slot as u32;
        // Clear the removed entity's sparse pointer and pop the tail.
        self.sparse[id.index() as usize] = NONE;
        self.gens.pop();
        self.dense.pop();
        self.data.pop()
    }

    /// Iterate `(entity_index, &value)` in dense order. The generation is not
    /// returned here; pair with the owning `World` when a full `EntityId` is
    /// needed. Order is unspecified (dense order) — sort by index for determinism.
    pub fn iter(&self) -> impl Iterator<Item = (u32, &T)> {
        self.dense.iter().copied().zip(self.data.iter())
    }

    /// Iterate `(EntityId, &value)` in dense order, reconstructing the full id
    /// from the stored index+generation. Order is dense (unspecified); the query
    /// layer sorts by id for determinism.
    pub fn iter_full(&self) -> impl Iterator<Item = (EntityId, &T)> {
        self.dense
            .iter()
            .zip(self.gens.iter())
            .zip(self.data.iter())
            .map(|((&idx, &g), v)| (EntityId::from_raw(idx, g), v))
    }

    /// Mutable variant of [`Self::iter`]. Bumps the version.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (u32, &mut T)> {
        self.version += 1;
        self.dense.iter().copied().zip(self.data.iter_mut())
    }

    /// Apply `f` to each `(EntityId, &mut value)` in **ascending entity order**
    /// (deterministic). Safe: each element is accessed by index one at a time, so
    /// there are no overlapping mutable borrows. Bumps the version.
    pub fn for_each_ordered_mut(&mut self, mut f: impl FnMut(EntityId, &mut T)) {
        self.version += 1;
        let mut order: Vec<usize> = (0..self.data.len()).collect();
        order.sort_unstable_by_key(|&s| self.dense[s]);
        for s in order {
            let id = EntityId::from_raw(self.dense[s], self.gens[s]);
            f(id, &mut self.data[s]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::Entities;

    #[test]
    fn insert_get_remove() {
        let mut e = Entities::new();
        let a = e.spawn();
        let b = e.spawn();
        let mut s: SparseSet<i32> = SparseSet::new();

        assert!(s.insert(a, 10).is_none());
        assert!(s.insert(b, 20).is_none());
        assert_eq!(s.len(), 2);
        assert_eq!(s.get(a), Some(&10));
        assert_eq!(s.get(b), Some(&20));

        assert_eq!(s.insert(a, 11), Some(10), "overwrite returns old value");
        assert_eq!(s.get(a), Some(&11));

        assert_eq!(s.remove(a), Some(11));
        assert!(!s.contains(a));
        assert_eq!(s.get(b), Some(&20), "swap-remove keeps other entries valid");
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn slot_reuse_by_insert_invalidates_stale_id() {
        // Contract: a SparseSet does not observe `despawn` (that happens on
        // `Entities`); the `World` is responsible for removing a despawned
        // entity's components. What the set *does* guarantee is that once a slot
        // is reused by an insert with the new generation, the old id stops
        // resolving — protecting against a forgotten cleanup aliasing fresh data.
        let mut e = Entities::new();
        let a = e.spawn();
        let mut s: SparseSet<&str> = SparseSet::new();
        s.insert(a, "old");

        e.despawn(a);
        let b = e.spawn(); // reuses a's slot, higher generation
        assert_eq!(a.index(), b.index());

        // Inserting for the new id refreshes the slot; the stale entry is gone,
        // and the overwrite is reported as `None` (it was not *this* id's value).
        assert!(s.insert(b, "new").is_none());
        assert_eq!(s.get(b), Some(&"new"));
        assert_eq!(s.get(a), None, "stale id no longer resolves after reuse");
        assert_eq!(s.remove(a), None, "stale id is not removable");
    }

    #[test]
    fn swap_remove_patches_back_pointer() {
        let mut e = Entities::new();
        let ids: Vec<_> = (0..5).map(|_| e.spawn()).collect();
        let mut s: SparseSet<u32> = SparseSet::new();
        for (i, id) in ids.iter().enumerate() {
            s.insert(*id, i as u32);
        }
        // Remove a middle element; the tail gets swapped into its place.
        assert_eq!(s.remove(ids[1]), Some(1));
        for (i, id) in ids.iter().enumerate() {
            if i == 1 {
                assert!(!s.contains(*id));
            } else {
                assert_eq!(s.get(*id), Some(&(i as u32)), "id {i} survived intact");
            }
        }
    }

    #[test]
    fn iter_visits_all_values() {
        let mut e = Entities::new();
        let mut s: SparseSet<u32> = SparseSet::new();
        let mut expected = 0u32;
        for v in 0..4u32 {
            s.insert(e.spawn(), v);
            expected += v;
        }
        let sum: u32 = s.iter().map(|(_, v)| *v).sum();
        assert_eq!(sum, expected);
    }
}
