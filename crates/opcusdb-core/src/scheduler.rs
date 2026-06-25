//! The system scheduler (`CORE_SPEC.md` §8).
//!
//! Systems declare which components they **read** and **write**. From those
//! declarations the scheduler builds a conflict graph and assigns each system a
//! *stage*: systems in the same stage are pairwise independent and could run in
//! parallel; conflicting systems land in later stages, preserving their relative
//! order.
//!
//! This module does the *analysis* and *deterministic serial execution*. Actual
//! multi-threaded execution within a stage needs to hand each thread a disjoint
//! slice of the `World` (split by component access), which requires encapsulated
//! `unsafe`; that is deferred. What's here is the correctness foundation, and the
//! key safety property is verified in tests: **reordering independent systems does
//! not change the result** (so running them concurrently is sound).

use crate::world::World;
use core::any::TypeId;
use std::collections::HashSet;

type SystemFn = Box<dyn Fn(&mut World)>;

struct System {
    name: &'static str,
    reads: HashSet<TypeId>,
    writes: HashSet<TypeId>,
    run: SystemFn,
}

impl System {
    /// Two systems conflict if either one writes a component the other touches
    /// (reads or writes). Independent (non-conflicting) systems may run together.
    fn conflicts_with(&self, other: &System) -> bool {
        self.writes.iter().any(|w| other.reads.contains(w) || other.writes.contains(w))
            || other.writes.iter().any(|w| self.reads.contains(w) || self.writes.contains(w))
    }
}

/// An ordered set of systems with declared component access.
#[derive(Default)]
pub struct Schedule {
    systems: Vec<System>,
}

impl Schedule {
    /// An empty schedule.
    pub fn new() -> Self {
        Self::default()
    }

    /// Begin declaring a system named `name`. Chain `.reads::<T>()` /
    /// `.writes::<T>()` then `.build(run)` to add it.
    pub fn system(&mut self, name: &'static str) -> SystemBuilder<'_> {
        SystemBuilder {
            sched: self,
            name,
            reads: HashSet::new(),
            writes: HashSet::new(),
        }
    }

    /// Number of registered systems.
    pub fn len(&self) -> usize {
        self.systems.len()
    }

    /// Whether no systems are registered.
    pub fn is_empty(&self) -> bool {
        self.systems.is_empty()
    }

    /// Run every system once, in registration order, against `world`.
    /// Deterministic: a fixed order plus the world's deterministic operations.
    pub fn run(&self, world: &mut World) {
        for s in &self.systems {
            (s.run)(world);
        }
    }

    /// Run the systems in an explicit index order (used to verify that reordering
    /// independent systems is equivalent). Panics on an out-of-range index.
    pub fn run_order(&self, order: &[usize], world: &mut World) {
        for &i in order {
            (self.systems[i].run)(world);
        }
    }

    /// A human-readable execution plan: each system's name paired with the stage
    /// it runs in. Useful for inspecting the parallelism the scheduler found.
    pub fn plan(&self) -> Vec<(&'static str, usize)> {
        let mut out = vec![("", 0usize); self.systems.len()];
        for (stage, idxs) in self.stages().iter().enumerate() {
            for &i in idxs {
                out[i] = (self.systems[i].name, stage);
            }
        }
        out
    }

    /// Compute execution stages: `stages()[k]` is the indices of systems that can
    /// run together in stage `k`. Systems in one stage are pairwise independent;
    /// conflicting systems are placed in later stages preserving their order.
    pub fn stages(&self) -> Vec<Vec<usize>> {
        let n = self.systems.len();
        let mut level = vec![0usize; n];
        for i in 0..n {
            let mut lv = 0;
            // Conflicting earlier systems push this one to a later stage.
            for (sys_j, &lvl_j) in self.systems[..i].iter().zip(&level[..i]) {
                if self.systems[i].conflicts_with(sys_j) {
                    lv = lv.max(lvl_j + 1);
                }
            }
            level[i] = lv;
        }
        let stage_count = level.iter().copied().max().map_or(0, |m| m + 1);
        let mut stages = vec![Vec::new(); stage_count];
        for (i, &lv) in level.iter().enumerate() {
            stages[lv].push(i);
        }
        stages
    }
}

/// Builder for one system's declared access. Finish with [`build`](Self::build).
pub struct SystemBuilder<'s> {
    sched: &'s mut Schedule,
    name: &'static str,
    reads: HashSet<TypeId>,
    writes: HashSet<TypeId>,
}

impl SystemBuilder<'_> {
    /// Declare that the system reads component `T`.
    pub fn reads<T: 'static>(mut self) -> Self {
        self.reads.insert(TypeId::of::<T>());
        self
    }

    /// Declare that the system writes component `T`.
    pub fn writes<T: 'static>(mut self) -> Self {
        self.writes.insert(TypeId::of::<T>());
        self
    }

    /// Register the system with its run closure.
    pub fn build(self, run: impl Fn(&mut World) + 'static) {
        self.sched.systems.push(System {
            name: self.name,
            reads: self.reads,
            writes: self.writes,
            run: Box::new(run),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    struct Pos(i64);
    #[derive(Clone, Debug, PartialEq)]
    struct Vel(i64);
    #[derive(Clone, Debug, PartialEq)]
    struct Health(i64);

    fn world_with_one() -> (World, crate::EntityId) {
        let mut w = World::new();
        let e = w.spawn();
        w.insert(e, Pos(0));
        w.insert(e, Vel(2));
        w.insert(e, Health(10));
        (w, e)
    }

    #[test]
    fn independent_systems_share_a_stage() {
        // "move" writes Pos (reads Vel); "regen" writes Health. Disjoint -> 1 stage.
        let mut s = Schedule::new();
        s.system("move").reads::<Vel>().writes::<Pos>().build(|_w| {});
        s.system("regen").writes::<Health>().build(|_w| {});
        let stages = s.stages();
        assert_eq!(stages.len(), 1, "independent systems run together");
        assert_eq!(stages[0], vec![0, 1]);
    }

    #[test]
    fn conflicting_systems_are_serialized() {
        // Both write Pos -> must be in separate, ordered stages.
        let mut s = Schedule::new();
        s.system("a").writes::<Pos>().build(|_w| {});
        s.system("b").writes::<Pos>().build(|_w| {});
        let stages = s.stages();
        assert_eq!(stages.len(), 2);
        assert_eq!(stages[0], vec![0]);
        assert_eq!(stages[1], vec![1]);
    }

    #[test]
    fn plan_names_systems_and_stages() {
        let mut s = Schedule::new();
        s.system("move").reads::<Vel>().writes::<Pos>().build(|_w| {});
        s.system("regen").writes::<Health>().build(|_w| {});
        s.system("collide").writes::<Pos>().build(|_w| {}); // conflicts with "move"
        let plan = s.plan();
        assert_eq!(plan[0], ("move", 0));
        assert_eq!(plan[1], ("regen", 0)); // independent of move -> stage 0
        assert_eq!(plan[2], ("collide", 1)); // shares Pos with move -> stage 1
    }

    #[test]
    fn read_write_on_same_component_conflicts() {
        // "a" writes Pos, "b" reads Pos -> conflict (b must see a's writes).
        let mut s = Schedule::new();
        s.system("a").writes::<Pos>().build(|_w| {});
        s.system("b").reads::<Pos>().build(|_w| {});
        assert_eq!(s.stages().len(), 2);
    }

    #[test]
    fn run_executes_all_systems() {
        let (mut w, e) = world_with_one();
        let mut s = Schedule::new();
        s.system("move").reads::<Vel>().writes::<Pos>().build(|w| {
            for id in w.matching::<(Pos, Vel)>() {
                let v = w.get::<Vel>(id).unwrap().0;
                w.get_mut::<Pos>(id).unwrap().0 += v;
            }
        });
        s.system("regen").writes::<Health>().build(|w| {
            w.for_each_mut::<Health>(|_id, h| h.0 += 1);
        });
        s.run(&mut w);
        assert_eq!(w.get::<Pos>(e), Some(&Pos(2)));
        assert_eq!(w.get::<Health>(e), Some(&Health(11)));
    }

    #[test]
    fn reordering_independent_systems_is_equivalent() {
        // THE parallel-safety property: independent systems commute, so running
        // them in any order yields the same world (hence concurrency is sound).
        let build = || {
            let mut s = Schedule::new();
            s.system("move").reads::<Vel>().writes::<Pos>().build(|w| {
                for id in w.matching::<(Pos, Vel)>() {
                    let v = w.get::<Vel>(id).unwrap().0;
                    w.get_mut::<Pos>(id).unwrap().0 += v;
                }
            });
            s.system("regen").writes::<Health>().build(|w| {
                w.for_each_mut::<Health>(|_id, h| h.0 += 1);
            });
            s
        };

        let (mut w_fwd, e) = world_with_one();
        build().run_order(&[0, 1], &mut w_fwd);

        let (mut w_rev, _) = world_with_one();
        build().run_order(&[1, 0], &mut w_rev);

        // Same result regardless of order -> the systems are truly independent.
        assert_eq!(w_fwd.get::<Pos>(e), w_rev.get::<Pos>(e));
        assert_eq!(w_fwd.get::<Health>(e), w_rev.get::<Health>(e));
    }
}
