//! Run an ECS [`World`] as a [`Timeline`](opcusdb_time::Timeline)-driven
//! simulation, so ECS games get rollback, scrubbing, and replay for free
//! (`CORE_SPEC.md` §9), exactly like the hand-written sims.
//!
//! The trick is keeping state and logic separate so the *state* can be cloned:
//! - the **state** is the `World` (now deep-`Clone`),
//! - the **logic** is an [`EcsLogic`] implemented on a *zero-sized marker type*
//!   via associated functions (no data, so it doesn't need to be cloned).
//!
//! [`EcsWorld<G>`] bundles them: it holds the `World` plus a `PhantomData<G>`, is
//! `Clone` (World deep-copies; the marker is free), and implements
//! [`Sim`](opcusdb_time::Sim) by delegating each tick to `G::step`. Drop it into a
//! `Timeline<EcsWorld<G>>` and rollback/replay just work.

use core::marker::PhantomData;
use opcusdb_core::World;
use opcusdb_time::{Sim, Tick};

/// The logic of an ECS simulation, defined on a zero-sized marker type so it
/// carries no state. Determinism is the implementor's responsibility (use
/// `World` queries, which iterate in ascending entity order, and keep any RNG as
/// a resource so it is snapshotted with the world).
pub trait EcsLogic: 'static {
    /// Per-tick input applied to the world.
    type Input;

    /// Build the initial world: spawn entities, insert resources. Must be
    /// deterministic so a fresh `EcsWorld` is a valid replay baseline.
    fn setup(world: &mut World);

    /// Advance the world exactly one tick given this tick's inputs. Deterministic.
    fn step(world: &mut World, tick: Tick, inputs: &[Self::Input]);
}

/// An ECS world paired with its (stateless) logic `G`, usable as a [`Sim`].
pub struct EcsWorld<G: EcsLogic> {
    world: World,
    _logic: PhantomData<G>,
}

impl<G: EcsLogic> EcsWorld<G> {
    /// Create and initialize the world via `G::setup`.
    pub fn new() -> Self {
        let mut world = World::new();
        G::setup(&mut world);
        Self {
            world,
            _logic: PhantomData,
        }
    }

    /// Shared access to the underlying world (for queries / inspection).
    pub fn world(&self) -> &World {
        &self.world
    }

    /// Mutable access to the underlying world.
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }
}

impl<G: EcsLogic> Default for EcsWorld<G> {
    fn default() -> Self {
        Self::new()
    }
}

impl<G: EcsLogic> Clone for EcsWorld<G> {
    fn clone(&self) -> Self {
        Self {
            world: self.world.clone(), // deep copy, the snapshot
            _logic: PhantomData,
        }
    }
}

impl<G: EcsLogic> Sim for EcsWorld<G> {
    type Input = G::Input;

    fn step(&mut self, tick: Tick, inputs: &[Self::Input]) {
        G::step(&mut self.world, tick, inputs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opcusdb_time::Timeline;

    // A tiny counting world: one entity holds a count; each tick adds the inputs.
    #[derive(Clone, Debug, PartialEq)]
    struct Counter(i64);

    struct CountLogic;
    impl EcsLogic for CountLogic {
        type Input = i64;
        fn setup(world: &mut World) {
            let e = world.spawn();
            world.insert(e, Counter(0));
        }
        fn step(world: &mut World, _tick: Tick, inputs: &[i64]) {
            let sum: i64 = inputs.iter().sum();
            for id in world.matching::<(Counter,)>() {
                world.get_mut::<Counter>(id).unwrap().0 += sum;
            }
        }
    }

    fn value(w: &EcsWorld<CountLogic>) -> i64 {
        w.world().query::<Counter>().map(|(_, c)| c.0).next().unwrap()
    }

    #[test]
    fn ecs_world_runs_on_timeline() {
        let mut tl = Timeline::new(EcsWorld::<CountLogic>::new(), 4, 8);
        for i in 1..=5 {
            tl.advance(vec![i]);
        }
        assert_eq!(value(tl.state()), 15); // 1+2+3+4+5
    }

    #[test]
    fn replay_reproduces_ecs_state() {
        let mut tl = Timeline::new(EcsWorld::<CountLogic>::new(), 4, 8);
        for i in 1..=10 {
            tl.advance(vec![i]);
        }
        let replayed = Timeline::replay(EcsWorld::<CountLogic>::new(), tl.log());
        assert_eq!(value(&replayed), value(tl.state()));
    }

    #[test]
    fn rollback_then_resim_reproduces_ecs_state() {
        let mut tl = Timeline::new(EcsWorld::<CountLogic>::new(), 4, 8);
        let inputs: Vec<i64> = (1..=8).collect();
        for &i in &inputs {
            tl.advance(vec![i]);
        }
        let final_value = value(tl.state());
        assert!(tl.seek(3));
        for &i in &inputs[3..] {
            tl.advance(vec![i]);
        }
        assert_eq!(value(tl.state()), final_value);
    }
}
