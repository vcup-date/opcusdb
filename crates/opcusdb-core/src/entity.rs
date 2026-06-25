//! Generational entity identifiers and the entity allocator.
//!
//! See `CORE_SPEC.md` §4. An [`EntityId`] is an `(index, gen)` pair: `index`
//! locates the slot, `gen` distinguishes successive entities that reuse the same
//! slot, so a stale id never aliases a live one. Allocation is fully determined
//! by the call sequence (LIFO free-list), which keeps snapshots reproducible.

use core::fmt;

/// A handle to an entity. `Copy` and cheap (8 bytes). Ordered by `(index, gen)`
/// so containers iterate in a stable, deterministic order.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityId {
    index: u32,
    gen: u32,
}

impl EntityId {
    /// The slot this entity occupies. Valid only together with [`Self::gen`].
    #[inline]
    pub const fn index(self) -> u32 {
        self.index
    }

    /// The generation stamp distinguishing reuses of the same slot.
    #[inline]
    pub const fn gen(self) -> u32 {
        self.gen
    }

    /// Construct an id from raw parts. Intended for deserialization/tests; normal
    /// code obtains ids from [`Entities::spawn`].
    #[inline]
    pub const fn from_raw(index: u32, gen: u32) -> Self {
        Self { index, gen }
    }
}

impl fmt::Debug for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "E{}v{}", self.index, self.gen)
    }
}

/// The entity allocator: tracks the live generation of every slot and recycles
/// freed slots LIFO. This is part of the `World`'s serializable state.
#[derive(Clone, Debug, Default)]
pub struct Entities {
    /// `generations[index]` is the generation currently live in that slot.
    generations: Vec<u32>,
    /// Recycled slot indices, popped LIFO so allocation is deterministic.
    free: Vec<u32>,
    /// Count of currently-live entities (cheap `len`/`is_empty`).
    live: u32,
}

impl Entities {
    /// A fresh allocator with no entities.
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocate a new entity. Reuses a freed slot if one is available, otherwise
    /// grows the slot table. The returned id is live until [`Self::despawn`].
    pub fn spawn(&mut self) -> EntityId {
        self.live += 1;
        if let Some(index) = self.free.pop() {
            // Reused slot: its generation was already bumped at despawn time, so
            // the live generation is whatever is stored now.
            EntityId {
                index,
                gen: self.generations[index as usize],
            }
        } else {
            let index = self.generations.len() as u32;
            // Generations start at 1 so that a zeroed `EntityId` (index 0, gen 0)
            // is never a live entity — useful as a niche/"null" sentinel.
            self.generations.push(1);
            EntityId { index, gen: 1 }
        }
    }

    /// Free an entity. Returns `false` (and does nothing) if `id` is already dead
    /// or out of range, so double-despawn is safe and observable.
    pub fn despawn(&mut self, id: EntityId) -> bool {
        if !self.is_alive(id) {
            return false;
        }
        // Bump the generation so the old id can never be confused with the next
        // occupant of this slot, then mark the slot free.
        self.generations[id.index as usize] = id.gen.wrapping_add(1);
        self.free.push(id.index);
        self.live -= 1;
        true
    }

    /// Whether `id` refers to a currently-live entity.
    #[inline]
    pub fn is_alive(&self, id: EntityId) -> bool {
        self.generations
            .get(id.index as usize)
            .is_some_and(|&g| g == id.gen)
    }

    /// Number of currently-live entities.
    #[inline]
    pub fn len(&self) -> u32 {
        self.live
    }

    /// Whether there are no live entities.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.live == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_yields_distinct_live_ids() {
        let mut e = Entities::new();
        let a = e.spawn();
        let b = e.spawn();
        assert_ne!(a, b);
        assert!(e.is_alive(a) && e.is_alive(b));
        assert_eq!(e.len(), 2);
    }

    #[test]
    fn despawn_invalidates_id() {
        let mut e = Entities::new();
        let a = e.spawn();
        assert!(e.despawn(a));
        assert!(!e.is_alive(a));
        assert!(!e.despawn(a), "double despawn is a no-op");
        assert_eq!(e.len(), 0);
    }

    #[test]
    fn slot_reuse_bumps_generation() {
        let mut e = Entities::new();
        let a = e.spawn();
        e.despawn(a);
        let b = e.spawn();
        // Same slot reused...
        assert_eq!(a.index(), b.index());
        // ...but a higher generation, so the stale id stays dead.
        assert!(b.gen() > a.gen());
        assert!(!e.is_alive(a));
        assert!(e.is_alive(b));
    }

    #[test]
    fn free_list_is_lifo_deterministic() {
        let mut e = Entities::new();
        let a = e.spawn();
        let b = e.spawn();
        let c = e.spawn();
        e.despawn(a);
        e.despawn(b);
        e.despawn(c);
        // LIFO: c freed last -> reused first.
        assert_eq!(e.spawn().index(), c.index());
        assert_eq!(e.spawn().index(), b.index());
        assert_eq!(e.spawn().index(), a.index());
    }

    #[test]
    fn zeroed_id_is_never_live() {
        let e = Entities::new();
        // index 0, gen 0 sentinel is never alive (generations start at 1).
        assert!(!e.is_alive(EntityId::from_raw(0, 0)));
    }
}
