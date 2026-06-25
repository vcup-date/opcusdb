//! `select` — memoized **derived views** over the World (`DESIGN.md` §4,
//! `CORE_SPEC.md` §7). The last of the five sync primitives (`reduce`, `merge`,
//! `select`, `query`, `fold`).
//!
//! A [`Select`] computes a value from the World with a pure function and caches
//! it, recomputing only when one of its declared component **dependencies**
//! actually changes (tracked by [`World::component_version`]). This is how derived
//! state (a computed stat, an aggregate, a spatial summary) stays correct without
//! manual invalidation — and the same change-detection underpins reactive
//! subscriptions ("a subscription is a query that streams its delta").
//!
//! ```ignore
//! let mut threat = Select::new(deps::<Threat, _>(...));   // see `Select::new`
//! let v = threat.get(&world);   // recomputes only when deps changed
//! ```

use crate::world::World;
use core::any::TypeId;

type ComputeFn<T> = Box<dyn Fn(&World) -> T>;

/// A memoized derived value. Construct with the component types it depends on and
/// a pure closure; [`get`](Self::get) returns the cached value, recomputing only
/// when a dependency's version changed.
pub struct Select<T> {
    deps: Vec<TypeId>,
    compute: ComputeFn<T>,
    cache: Option<(Vec<u64>, T)>,
}

impl<T> Select<T> {
    /// Create a derived view depending on the component `TypeId`s in `deps`,
    /// computed by `compute`. Use [`dep`] / [`deps2`] / [`deps3`] for ergonomics.
    pub fn new(deps: Vec<TypeId>, compute: impl Fn(&World) -> T + 'static) -> Self {
        Self {
            deps,
            compute: Box::new(compute),
            cache: None,
        }
    }

    /// The current derived value, recomputing iff a dependency changed since the
    /// last call.
    pub fn get(&mut self, world: &World) -> &T {
        let versions: Vec<u64> = self.deps.iter().map(|&t| world.version_of(t)).collect();
        let stale = match &self.cache {
            Some((cached, _)) => *cached != versions,
            None => true,
        };
        if stale {
            let value = (self.compute)(world);
            self.cache = Some((versions, value));
        }
        &self.cache.as_ref().expect("just populated").1
    }

    /// Drop the cached value, forcing a recompute on the next [`get`](Self::get).
    pub fn invalidate(&mut self) {
        self.cache = None;
    }
}

/// Dependency list for a single component type.
pub fn dep<T: 'static>() -> Vec<TypeId> {
    vec![TypeId::of::<T>()]
}

/// Dependency list for two component types.
pub fn deps2<A: 'static, B: 'static>() -> Vec<TypeId> {
    vec![TypeId::of::<A>(), TypeId::of::<B>()]
}

/// Dependency list for three component types.
pub fn deps3<A: 'static, B: 'static, C: 'static>() -> Vec<TypeId> {
    vec![TypeId::of::<A>(), TypeId::of::<B>(), TypeId::of::<C>()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    #[derive(Clone, Debug, PartialEq)]
    struct Hp(i32);
    #[derive(Clone, Debug, PartialEq)]
    struct Armor(i32);

    #[test]
    fn memoizes_until_a_dependency_changes() {
        let mut w = World::new();
        let a = w.spawn();
        w.insert(a, Hp(30));
        let b = w.spawn();
        w.insert(b, Hp(70));

        let calls = Rc::new(Cell::new(0u32));
        let c = calls.clone();
        // Derived: total HP across the world.
        let mut total = Select::new(dep::<Hp>(), move |w: &World| {
            c.set(c.get() + 1);
            w.query::<Hp>().map(|(_, h)| h.0).sum::<i32>()
        });

        assert_eq!(*total.get(&w), 100);
        assert_eq!(calls.get(), 1);
        // Repeated reads with no change -> served from cache.
        assert_eq!(*total.get(&w), 100);
        assert_eq!(*total.get(&w), 100);
        assert_eq!(calls.get(), 1, "memoized: no recompute without a change");

        // Mutate a dependency -> recompute on next get.
        w.get_mut::<Hp>(a).unwrap().0 = 50;
        assert_eq!(*total.get(&w), 120);
        assert_eq!(calls.get(), 2, "recomputed after the dependency changed");
    }

    #[test]
    fn unrelated_change_does_not_invalidate() {
        let mut w = World::new();
        let e = w.spawn();
        w.insert(e, Hp(10));
        w.insert(e, Armor(5));

        let calls = Rc::new(Cell::new(0u32));
        let c = calls.clone();
        // Depends only on Hp.
        let mut sel = Select::new(dep::<Hp>(), move |w: &World| {
            c.set(c.get() + 1);
            w.query::<Hp>().map(|(_, h)| h.0).sum::<i32>()
        });
        assert_eq!(*sel.get(&w), 10);
        assert_eq!(calls.get(), 1);

        // Changing Armor (not a dependency) must NOT invalidate the Hp select.
        w.get_mut::<Armor>(e).unwrap().0 = 99;
        assert_eq!(*sel.get(&w), 10);
        assert_eq!(calls.get(), 1, "unrelated change did not trigger recompute");

        // Changing Hp does.
        w.get_mut::<Hp>(e).unwrap().0 = 20;
        assert_eq!(*sel.get(&w), 20);
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn invalidate_forces_recompute() {
        let mut w = World::new();
        let e = w.spawn();
        w.insert(e, Hp(1));
        let calls = Rc::new(Cell::new(0u32));
        let c = calls.clone();
        let mut sel = Select::new(dep::<Hp>(), move |w: &World| {
            c.set(c.get() + 1);
            w.query::<Hp>().count()
        });
        sel.get(&w);
        sel.get(&w);
        assert_eq!(calls.get(), 1);
        sel.invalidate();
        sel.get(&w);
        assert_eq!(calls.get(), 2);
    }
}
