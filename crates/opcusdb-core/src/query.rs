//! Deterministic, read-only multi-component queries.
//!
//! See `CORE_SPEC.md` §8. A query yields the entities that have *all* of a set of
//! components, **in ascending `EntityId` order** so iteration is deterministic
//! (the determinism contract, §2). It collects matching ids from one component's
//! store, filters by the rest, and sorts, then the typed `query*` helpers on
//! [`World`] hand back component references via O(1) lookups.
//!
//! Scope note: these are read-only joins. Simultaneous *mutable* multi-component
//! access (the classic `&mut A, &B` join) needs either disjoint-borrow proof or
//! encapsulated `unsafe`; with `unsafe_code = warn` we defer it. For now, mutate
//! by iterating ids and using `get`/`get_mut` sequentially (copy reads out first).
//! Exclusion filters (`without::<T>`) are also a later addition.

use crate::entity::EntityId;
use crate::world::World;

/// A conjunctive component filter: "has all of these types". Implemented for
/// tuples of component types. Produces a sorted (ascending) list of matching ids.
pub trait Filter {
    /// All live entities that have every component in this filter, ascending by id.
    fn matching(world: &World) -> Vec<EntityId>;
}

impl<A: 'static> Filter for (A,) {
    fn matching(world: &World) -> Vec<EntityId> {
        let Some(store) = world.store::<A>() else {
            return Vec::new();
        };
        let mut ids: Vec<EntityId> = store.iter_full().map(|(id, _)| id).collect();
        ids.sort_unstable();
        ids
    }
}

impl<A: 'static, B: 'static> Filter for (A, B) {
    fn matching(world: &World) -> Vec<EntityId> {
        let (Some(sa), Some(sb)) = (world.store::<A>(), world.store::<B>()) else {
            return Vec::new();
        };
        // Pick the smaller store as the candidate source, filter by the other,         // O(min) candidates instead of O(|A|). Result is still sorted ascending.
        let mut ids: Vec<EntityId> = if sa.len() <= sb.len() {
            sa.iter_full().map(|(id, _)| id).filter(|&id| world.has::<B>(id)).collect()
        } else {
            sb.iter_full().map(|(id, _)| id).filter(|&id| world.has::<A>(id)).collect()
        };
        ids.sort_unstable();
        ids
    }
}

impl<A: 'static, B: 'static, C: 'static> Filter for (A, B, C) {
    fn matching(world: &World) -> Vec<EntityId> {
        let (Some(sa), Some(sb), Some(sc)) =
            (world.store::<A>(), world.store::<B>(), world.store::<C>())
        else {
            return Vec::new();
        };
        let (la, lb, lc) = (sa.len(), sb.len(), sc.len());
        // Iterate the smallest of the three stores.
        let mut ids: Vec<EntityId> = if la <= lb && la <= lc {
            sa.iter_full()
                .map(|(id, _)| id)
                .filter(|&id| world.has::<B>(id) && world.has::<C>(id))
                .collect()
        } else if lb <= lc {
            sb.iter_full()
                .map(|(id, _)| id)
                .filter(|&id| world.has::<A>(id) && world.has::<C>(id))
                .collect()
        } else {
            sc.iter_full()
                .map(|(id, _)| id)
                .filter(|&id| world.has::<A>(id) && world.has::<B>(id))
                .collect()
        };
        ids.sort_unstable();
        ids
    }
}

impl World {
    /// Ids of all entities matching filter `F` (a tuple of component types),
    /// ascending. Use when you need to then mutate via `get_mut` in a loop.
    pub fn matching<F: Filter>(&self) -> Vec<EntityId> {
        F::matching(self)
    }

    /// Like [`matching`](Self::matching) but excluding entities that also have
    /// component `X` (e.g. "all `Position` *without* `Dead`"). Order preserved.
    pub fn matching_without<F: Filter, X: 'static>(&self) -> Vec<EntityId> {
        let mut ids = F::matching(self);
        ids.retain(|&id| !self.has::<X>(id));
        ids
    }

    /// Iterate `(id, &A)` for every entity with an `A`, ascending by id.
    pub fn query<A: 'static>(&self) -> impl Iterator<Item = (EntityId, &A)> {
        self.matching::<(A,)>()
            .into_iter()
            .map(move |id| (id, self.get::<A>(id).expect("matched entity has A")))
    }

    /// Iterate `(id, &A, &B)` for every entity with both `A` and `B`, ascending.
    pub fn query2<A: 'static, B: 'static>(&self) -> impl Iterator<Item = (EntityId, &A, &B)> {
        self.matching::<(A, B)>().into_iter().map(move |id| {
            (
                id,
                self.get::<A>(id).expect("matched entity has A"),
                self.get::<B>(id).expect("matched entity has B"),
            )
        })
    }

    /// Iterate `(id, &A, &B, &C)` for every entity with all three, ascending.
    pub fn query3<A: 'static, B: 'static, C: 'static>(
        &self,
    ) -> impl Iterator<Item = (EntityId, &A, &B, &C)> {
        self.matching::<(A, B, C)>().into_iter().map(move |id| {
            (
                id,
                self.get::<A>(id).expect("matched entity has A"),
                self.get::<B>(id).expect("matched entity has B"),
                self.get::<C>(id).expect("matched entity has C"),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::world::World;

    #[derive(Clone, Debug, PartialEq)]
    struct Pos(i32);
    #[derive(Clone, Debug, PartialEq)]
    struct Vel(i32);
    #[derive(Clone, Debug, PartialEq)]
    struct Tag;

    #[test]
    fn join_yields_only_entities_with_all_components() {
        let mut w = World::new();
        let a = w.spawn();
        w.insert(a, Pos(1));
        w.insert(a, Vel(10));
        let b = w.spawn();
        w.insert(b, Pos(2)); // no Vel -> excluded from the 2-join
        let c = w.spawn();
        w.insert(c, Pos(3));
        w.insert(c, Vel(30));

        let got: Vec<_> = w.query2::<Pos, Vel>().map(|(id, p, v)| (id, p.0, v.0)).collect();
        assert_eq!(got, vec![(a, 1, 10), (c, 3, 30)]);
    }

    #[test]
    fn results_are_ascending_regardless_of_insertion_order() {
        let mut w = World::new();
        // Spawn three, insert components in a scrambled order.
        let e0 = w.spawn();
        let e1 = w.spawn();
        let e2 = w.spawn();
        w.insert(e2, Pos(2));
        w.insert(e0, Pos(0));
        w.insert(e1, Pos(1));

        let ids: Vec<_> = w.query::<Pos>().map(|(id, _)| id).collect();
        assert_eq!(ids, vec![e0, e1, e2], "deterministic ascending order");
    }

    #[test]
    fn three_way_join() {
        let mut w = World::new();
        let a = w.spawn();
        w.insert(a, Pos(1));
        w.insert(a, Vel(2));
        w.insert(a, Tag);
        let b = w.spawn();
        w.insert(b, Pos(9));
        w.insert(b, Vel(9)); // missing Tag

        let ids: Vec<_> = w.query3::<Pos, Vel, Tag>().map(|(id, ..)| id).collect();
        assert_eq!(ids, vec![a]);
    }

    #[test]
    fn missing_type_yields_empty() {
        let w = World::new();
        assert_eq!(w.query::<Pos>().count(), 0);
        assert_eq!(w.query2::<Pos, Vel>().count(), 0);
    }

    #[test]
    fn mutate_via_ids_then_get_mut() {
        // The documented pattern for read-one-write-another joins.
        let mut w = World::new();
        let a = w.spawn();
        w.insert(a, Pos(0));
        w.insert(a, Vel(5));
        for id in w.matching::<(Pos, Vel)>() {
            let v = w.get::<Vel>(id).unwrap().0; // copy read out
            w.get_mut::<Pos>(id).unwrap().0 += v; // then mutate
        }
        assert_eq!(w.get::<Pos>(a), Some(&Pos(5)));
    }

    #[test]
    fn pick_smallest_store_join_is_correct() {
        // A huge Pos population, a tiny Vel population: the join must still be
        // exactly the intersection (the optimization picks the small store).
        let mut w = World::new();
        let mut with_both = Vec::new();
        for i in 0..1000 {
            let e = w.spawn();
            w.insert(e, Pos(i));
            if i % 250 == 0 {
                w.insert(e, Vel(i));
                with_both.push(e);
            }
        }
        let got: Vec<_> = w.matching::<(Pos, Vel)>();
        assert_eq!(got, with_both, "join == intersection regardless of sizes");
        // Symmetric order: (Vel, Pos) yields the same set.
        assert_eq!(w.matching::<(Vel, Pos)>(), with_both);
    }

    #[test]
    fn matching_without_excludes() {
        let mut w = World::new();
        let alive = w.spawn();
        w.insert(alive, Pos(1));
        let dead = w.spawn();
        w.insert(dead, Pos(2));
        w.insert(dead, Tag); // Tag marks "dead"

        let got = w.matching_without::<(Pos,), Tag>();
        assert_eq!(got, vec![alive], "the tagged entity is excluded");
    }

    #[test]
    fn for_each_mut_visits_in_order() {
        let mut w = World::new();
        let e0 = w.spawn();
        let e1 = w.spawn();
        let e2 = w.spawn();
        // scrambled insert order
        w.insert(e2, Pos(2));
        w.insert(e0, Pos(0));
        w.insert(e1, Pos(1));

        let mut seen = Vec::new();
        w.for_each_mut::<Pos>(|id, p| {
            seen.push(id);
            p.0 += 100;
        });
        assert_eq!(seen, vec![e0, e1, e2], "ascending order");
        assert_eq!(w.get::<Pos>(e1), Some(&Pos(101)), "mutation applied");
    }
}
