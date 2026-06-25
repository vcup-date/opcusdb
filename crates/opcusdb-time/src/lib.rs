//! `opcusdb-time` — the Timeline (`CORE_SPEC.md` §9–§10).
//!
//! - [`tick`]: discrete simulation time ([`Tick`]).
//! - [`timer`]: deterministic [`Timers`] (min-heap, `due(now)`).
//! - [`timeline`]: fixed-timestep [`Timeline`] with keyframe ring, rollback, replay.
//!
//! The Timeline is generic over a [`Sim`] state, so it does not yet depend on
//! `World` serialization; wiring the `World` in comes once it is snapshot-able
//! (CORE_SPEC §14 Q1).

pub mod tick;
pub mod timeline;
pub mod timer;

pub use tick::Tick;
pub use timeline::{Sim, Timeline};
pub use timer::{TimerId, Timers};
