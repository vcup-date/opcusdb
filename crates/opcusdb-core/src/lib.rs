//! `opcusdb-core` — the ECS foundation of opcusdb.
//!
//! This crate is being built bottom-up per `CORE_SPEC.md`:
//! - §4 [`entity`]: generational entity ids + allocator.
//! - §5 [`storage`]: sparse-set component storage.
//! - §5 [`component`]: component ids + type-erased store plumbing.
//! - §5 [`world`]: the entities + components + resources container.
//! - §8 [`query`]: deterministic ascending-order multi-component joins.
//! - §8 [`commands`]: deferred structural changes applied at a barrier.
//! - §8 [`scheduler`]: declared-access systems → conflict stages + serial run.
//! - §3 [`spatial`]: uniform-grid spatial index for AOI / interest queries.
//! - §3 [`fx`]: deterministic Q16.16 fixed-point math (cross-platform identical).
//! - §2 [`rng`]: deterministic seeded PRNG (PCG32) for sim randomness.
//!
//! The `World` is deep-`Clone`, so it rides the Timeline for rollback/replay via
//! the `opcusdb-ecs` bridge. Remaining: multi-threaded stage execution (needs
//! encapsulated `unsafe` World-splitting).
//!
//! Everything here obeys the determinism contract (§2): no wall-clock, no
//! ambient randomness, and allocation/recycling determined solely by the call
//! sequence — so state is always reproducible from a snapshot + the event log.

pub mod commands;
pub mod component;
pub mod entity;
pub mod fx;
pub mod query;
pub mod rng;
pub mod scheduler;
pub mod select;
pub mod spatial;
pub mod storage;
pub mod world;

pub use commands::{Commands, EntityCommands};
pub use component::ComponentId;
pub use entity::{Entities, EntityId};
pub use fx::Fx;
pub use query::Filter;
pub use rng::Rng;
pub use scheduler::{Schedule, SystemBuilder};
pub use select::Select;
pub use spatial::SpatialGrid;
pub use storage::SparseSet;
pub use world::World;
