//! `load-test` — a many-entity swarm that exercises the ECS [`World`] at scale.
//!
//! This is the first use of the ECS in a running simulation (queries + commands
//! pattern, many entities per tick). It models "many users": each entity is a
//! mover with a [`Position`] and [`Velocity`]; every tick a movement system
//! advances all of them (toroidal wrap). It demonstrates the "many people
//! supported" goal and provides a throughput benchmark (see the `loadtest` bin).
//!
//! Determinism: positions/velocities are seeded by [`opcusdb_core::Rng`] and the
//! query layer iterates in ascending entity order, so a run is reproducible — the
//! [`Swarm::checksum`] over all positions is identical across runs of the same seed.

use opcusdb_core::{Rng, SpatialGrid, World};

/// Field width (positions wrap modulo this).
pub const WIDTH: i32 = 1000;
/// Field height.
pub const HEIGHT: i32 = 1000;

/// A mover's position on the toroidal field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    /// X coordinate in `0..WIDTH`.
    pub x: i32,
    /// Y coordinate in `0..HEIGHT`.
    pub y: i32,
}

/// A mover's per-tick velocity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Velocity {
    /// X delta per tick.
    pub dx: i32,
    /// Y delta per tick.
    pub dy: i32,
}

/// A swarm of moving entities backed by an ECS [`World`].
pub struct Swarm {
    world: World,
    count: u32,
    /// AOI index, rebuilt lazily by [`Swarm::mark_near`] (kept out of `step` so
    /// the movement benchmark measures only the simulation).
    grid: SpatialGrid,
    /// Per-entity interest flag (1 = in the last queried interest set). Index
    /// aligns with `write_positions` order (ascending entity).
    flags: Vec<u8>,
}

impl Swarm {
    /// Spawn `n` movers with seeded random positions and small velocities.
    pub fn new(n: u32, seed: u64) -> Self {
        let mut world = World::new();
        let mut rng = Rng::seed(seed);
        for _ in 0..n {
            let e = world.spawn();
            world.insert(
                e,
                Position {
                    x: rng.range(0, WIDTH as u32) as i32,
                    y: rng.range(0, HEIGHT as u32) as i32,
                },
            );
            world.insert(
                e,
                Velocity {
                    // dx, dy in -3..=3
                    dx: rng.range(0, 7) as i32 - 3,
                    dy: rng.range(0, 7) as i32 - 3,
                },
            );
        }
        Self {
            world,
            count: n,
            grid: SpatialGrid::new(WIDTH, HEIGHT, 64),
            flags: vec![0; n as usize],
        }
    }

    /// Compute the interest set within `radius` of `(cx, cy)` using the spatial
    /// grid (rebuilt from current positions), marking each entity's flag. This is
    /// the MMO interest-management primitive: O(local) instead of O(N). Returns
    /// the number of entities in the set.
    pub fn mark_near(&mut self, cx: i32, cy: i32, radius: i32) -> usize {
        self.grid.clear();
        for id in self.world.matching::<(Position,)>() {
            let p = self.world.get::<Position>(id).expect("has Position");
            self.grid.insert(id, p.x, p.y);
        }
        for f in &mut self.flags {
            *f = 0;
        }
        let near = self.grid.query_radius(cx, cy, radius);
        for id in &near {
            self.flags[id.index() as usize] = 1; // index aligns with positions order
        }
        near.len()
    }

    /// Per-entity interest flags from the last [`mark_near`](Self::mark_near),
    /// aligned with `write_positions` order.
    pub fn flags(&self) -> &[u8] {
        &self.flags
    }

    /// Number of entities.
    pub fn len(&self) -> u32 {
        self.count
    }

    /// Whether the swarm is empty.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Advance every entity one tick (movement system). Uses the matching-ids +
    /// `get_mut` pattern (mutable joins are a later query-layer feature).
    pub fn step(&mut self) {
        for id in self.world.matching::<(Position, Velocity)>() {
            let v = *self.world.get::<Velocity>(id).expect("matched has Velocity");
            let p = self.world.get_mut::<Position>(id).expect("matched has Position");
            p.x = (p.x + v.dx).rem_euclid(WIDTH);
            p.y = (p.y + v.dy).rem_euclid(HEIGHT);
        }
    }

    /// Write all positions as a flat `[x0, y0, x1, y1, ...]` buffer (ascending
    /// entity order). Reuses `out`'s capacity. Used by the WASM binding to hand
    /// positions to a renderer without per-entity FFI calls.
    pub fn write_positions(&self, out: &mut Vec<i32>) {
        out.clear();
        for (_, p) in self.world.query::<Position>() {
            out.push(p.x);
            out.push(p.y);
        }
    }

    /// Count entities whose position falls in `[x0,x1) × [y0,y1)` — an
    /// interest-management-style spatial query.
    pub fn count_in_region(&self, x0: i32, y0: i32, x1: i32, y1: i32) -> usize {
        self.world
            .query::<Position>()
            .filter(|(_, p)| p.x >= x0 && p.x < x1 && p.y >= y0 && p.y < y1)
            .count()
    }

    /// An order-independent... no — an order-*dependent*, deterministic checksum of
    /// all positions (the query iterates ascending entity id). Equal across runs
    /// of the same seed; a cheap way to assert reproducibility.
    pub fn checksum(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325; // FNV-1a offset basis
        for (_, p) in self.world.query::<Position>() {
            let mix = (p.x as u32 as u64).wrapping_mul(73_856_093)
                ^ (p.y as u32 as u64).wrapping_mul(19_349_663);
            h = (h ^ mix).wrapping_mul(0x0000_0100_0000_01b3); // FNV prime
        }
        h
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_checksum_same_seed() {
        let mut a = Swarm::new(5_000, 42);
        let mut b = Swarm::new(5_000, 42);
        for _ in 0..30 {
            a.step();
            b.step();
        }
        assert_eq!(a.checksum(), b.checksum());
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = Swarm::new(2_000, 1);
        let mut b = Swarm::new(2_000, 2);
        for _ in 0..10 {
            a.step();
            b.step();
        }
        assert_ne!(a.checksum(), b.checksum());
    }

    #[test]
    fn movement_wraps_at_edges() {
        // One entity is enough to check the toroidal wrap deterministically.
        let mut s = Swarm::new(1, 7);
        let before = s.checksum();
        s.step();
        // After a step the position changed (unless velocity is 0,0 — unlikely but
        // we only assert the field stays in-bounds via region count).
        let _ = before;
        assert_eq!(s.count_in_region(0, 0, WIDTH, HEIGHT), 1, "stays on field");
    }

    #[test]
    fn region_count_is_bounded() {
        let s = Swarm::new(10_000, 99);
        let all = s.count_in_region(0, 0, WIDTH, HEIGHT);
        assert_eq!(all, 10_000, "everyone is on the field");
        let quadrant = s.count_in_region(0, 0, WIDTH / 2, HEIGHT / 2);
        assert!(quadrant < all && quadrant > 0, "a sub-region holds some, not all");
    }

    #[test]
    fn interest_set_matches_brute_force() {
        let mut s = Swarm::new(8_000, 7);
        for _ in 0..10 {
            s.step();
        }
        let (cx, cy, r) = (500, 500, 90);
        let n = s.mark_near(cx, cy, r);

        // Brute-force the same query directly over the positions buffer.
        let mut pos = Vec::new();
        s.write_positions(&mut pos);
        let r2 = (r as i64) * (r as i64);
        let mut brute = 0usize;
        for (i, chunk) in pos.chunks_exact(2).enumerate() {
            let (dx, dy) = ((chunk[0] - cx) as i64, (chunk[1] - cy) as i64);
            let inside = dx * dx + dy * dy <= r2;
            assert_eq!(s.flags()[i] == 1, inside, "flag matches membership at {i}");
            if inside {
                brute += 1;
            }
        }
        assert_eq!(n, brute, "grid interest count == brute force");
        assert!(brute > 0, "the region should contain some entities");
    }

    #[test]
    fn scale_runs_and_stays_deterministic() {
        // A larger swarm to exercise the World at scale; assert it completes and
        // reproduces (perf numbers are reported by the `loadtest` binary, not here).
        let run = || {
            let mut s = Swarm::new(40_000, 2024);
            for _ in 0..20 {
                s.step();
            }
            (s.len(), s.checksum())
        };
        assert_eq!(run(), run());
    }
}

/// Proof that an ECS sim gets rollback/replay via the Timeline bridge: the swarm
/// movement expressed as `EcsLogic`, driven through a `Timeline`, with replay and
/// rollback reproducing the exact world state at scale.
#[cfg(test)]
mod rollback_tests {
    use super::{Position, Velocity, HEIGHT, WIDTH};
    use opcusdb_core::{Rng, World};
    use opcusdb_ecs::{EcsLogic, EcsWorld};
    use opcusdb_time::{Tick, Timeline};

    struct SwarmLogic;
    impl EcsLogic for SwarmLogic {
        type Input = ();
        fn setup(world: &mut World) {
            let mut rng = Rng::seed(2024);
            for _ in 0..3_000 {
                let e = world.spawn();
                world.insert(
                    e,
                    Position {
                        x: rng.range(0, WIDTH as u32) as i32,
                        y: rng.range(0, HEIGHT as u32) as i32,
                    },
                );
                world.insert(
                    e,
                    Velocity {
                        dx: rng.range(0, 7) as i32 - 3,
                        dy: rng.range(0, 7) as i32 - 3,
                    },
                );
            }
        }
        fn step(world: &mut World, _tick: Tick, _inputs: &[()]) {
            for id in world.matching::<(Position, Velocity)>() {
                let v = *world.get::<Velocity>(id).unwrap();
                let p = world.get_mut::<Position>(id).unwrap();
                p.x = (p.x + v.dx).rem_euclid(WIDTH);
                p.y = (p.y + v.dy).rem_euclid(HEIGHT);
            }
        }
    }

    fn checksum(w: &World) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for (_, p) in w.query::<Position>() {
            let mix = (p.x as u32 as u64).wrapping_mul(73_856_093)
                ^ (p.y as u32 as u64).wrapping_mul(19_349_663);
            h = (h ^ mix).wrapping_mul(0x0000_0100_0000_01b3);
        }
        h
    }

    #[test]
    fn swarm_replay_and_rollback_at_scale() {
        let mut tl = Timeline::new(EcsWorld::<SwarmLogic>::new(), 8, 8);
        for _ in 0..30 {
            tl.advance(vec![]);
        }
        let live = checksum(tl.state().world());

        // Replay from a fresh ECS world reproduces the live state.
        let replayed = Timeline::replay(EcsWorld::<SwarmLogic>::new(), tl.log());
        assert_eq!(checksum(replayed.world()), live);

        // Rewind 3000 entities to tick 10 and re-simulate -> identical world.
        assert!(tl.seek(10));
        for _ in 10..30 {
            tl.advance(vec![]);
        }
        assert_eq!(checksum(tl.state().world()), live);
    }
}
