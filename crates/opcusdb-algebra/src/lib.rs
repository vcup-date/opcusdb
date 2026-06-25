//! `opcusdb-algebra`, the functional sync algebra (`CORE_SPEC.md` §7).
//!
//! Pure, `World`-independent primitives the rest of opcusdb builds on:
//! - [`reduce`]: `Reduce` + `fold`, event-sourced state transitions (a
//!   statechart transition is a `Reduce`; the Timeline rebuilds state via `fold`).
//! - [`lattice`]: the `Lattice` trait + law-checkers for conflict-free merge.
//! - [`crdt`]: convergent replicated data types (`LwwReg`, `GCounter`,
//!   `PNCounter`, `OrSet`) backing the `Crdt<…>` component policy and P2P mesh.
//! - [`rga`]: a Replicated Growable Array, the ordered-sequence CRDT for
//!   text/chat (the chatroom substrate).
//!
//! The fifth primitive, `select` (a memoized derived view), is `World`-coupled and
//! lives in `opcusdb-core::select`. Together: reduce · merge · select · query · fold.

pub mod crdt;
pub mod lattice;
pub mod reduce;
pub mod rga;

pub use crdt::{GCounter, LwwReg, OrSet, PNCounter, PeerId, Tag};
pub use lattice::{assert_lattice_laws, join, Lattice};
pub use reduce::{fold, Reduce};
pub use rga::{OpId, Rga};
