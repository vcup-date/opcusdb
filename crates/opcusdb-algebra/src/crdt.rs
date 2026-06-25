//! The CRDT catalog (state-based / convergent replicated data types).
//!
//! See `CORE_SPEC.md` §7. Each type here is a [`Lattice`]: replicas merge
//! conflict-free and converge regardless of message order or duplication. These
//! back the `Crdt<…>` component policy and the serverless P2P mesh (DESIGN §6).
//!
//! Implemented this iteration: [`LwwReg`], [`GCounter`], [`PNCounter`], [`OrSet`].
//! Deferred: `Rga` (ordered sequence for text/chat) — see TODO.

use crate::lattice::Lattice;
use std::collections::{BTreeMap, BTreeSet};

/// Identifies a replica/peer. Used to break ties and tag operations uniquely.
pub type PeerId = u64;

// ---------------------------------------------------------------------------
// Last-Write-Wins register
// ---------------------------------------------------------------------------

/// A register holding a single value; concurrent writes resolve by the highest
/// `(timestamp, peer)` — a total order, so the result is deterministic. The peer
/// id breaks equal-timestamp ties, guaranteeing convergence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LwwReg<T> {
    value: T,
    ts: u64,
    peer: PeerId,
}

impl<T: Clone> LwwReg<T> {
    /// Create a register with an initial value stamped `(ts, peer)`.
    pub fn new(value: T, ts: u64, peer: PeerId) -> Self {
        Self { value, ts, peer }
    }

    /// Record a local write. Applies only if `(ts, peer)` beats the current stamp,
    /// so out-of-order or stale writes are ignored — keeping LWW well-defined.
    pub fn set(&mut self, value: T, ts: u64, peer: PeerId) {
        if (ts, peer) > (self.ts, self.peer) {
            self.value = value;
            self.ts = ts;
            self.peer = peer;
        }
    }

    /// The current value.
    pub fn get(&self) -> &T {
        &self.value
    }
}

impl<T: Clone> Lattice for LwwReg<T> {
    fn merge(&mut self, other: &Self) {
        if (other.ts, other.peer) > (self.ts, self.peer) {
            *self = other.clone();
        }
    }
}

// ---------------------------------------------------------------------------
// Grow-only counter
// ---------------------------------------------------------------------------

/// A counter that only increases. Each peer keeps its own tally; the value is the
/// sum, and merge takes the per-peer maximum (so re-delivered increments are idempotent).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GCounter {
    counts: BTreeMap<PeerId, u64>,
}

impl GCounter {
    /// An empty counter (value 0).
    pub fn new() -> Self {
        Self::default()
    }

    /// Add `by` to `peer`'s tally.
    pub fn inc(&mut self, peer: PeerId, by: u64) {
        *self.counts.entry(peer).or_insert(0) += by;
    }

    /// The total across all peers.
    pub fn value(&self) -> u64 {
        self.counts.values().sum()
    }
}

impl Lattice for GCounter {
    fn merge(&mut self, other: &Self) {
        for (&peer, &c) in &other.counts {
            let e = self.counts.entry(peer).or_insert(0);
            *e = (*e).max(c);
        }
    }
}

// ---------------------------------------------------------------------------
// Positive-Negative counter
// ---------------------------------------------------------------------------

/// A counter supporting both increment and decrement, as two [`GCounter`]s.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PNCounter {
    p: GCounter,
    n: GCounter,
}

impl PNCounter {
    /// An empty counter (value 0).
    pub fn new() -> Self {
        Self::default()
    }

    /// Increment `peer`'s tally by `by`.
    pub fn inc(&mut self, peer: PeerId, by: u64) {
        self.p.inc(peer, by);
    }

    /// Decrement on behalf of `peer` by `by`.
    pub fn dec(&mut self, peer: PeerId, by: u64) {
        self.n.inc(peer, by);
    }

    /// The signed value (increments minus decrements).
    pub fn value(&self) -> i64 {
        self.p.value() as i64 - self.n.value() as i64
    }
}

impl Lattice for PNCounter {
    fn merge(&mut self, other: &Self) {
        self.p.merge(&other.p);
        self.n.merge(&other.n);
    }
}

// ---------------------------------------------------------------------------
// Observed-Remove set (add-wins)
// ---------------------------------------------------------------------------

/// A unique tag for an add operation: `(peer, local_counter)`.
pub type Tag = (PeerId, u64);

/// An add-wins observed-remove set. Each add attaches a unique [`Tag`]; a remove
/// tombstones the tags it has observed for that element. An element is present
/// iff it has at least one add-tag that is not tombstoned — so a concurrent
/// add-vs-remove resolves in favor of the add. Both `adds` and `tombstones`
/// only grow, so merge is a union: commutative, associative, idempotent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OrSet<T: Ord + Clone> {
    adds: BTreeMap<T, BTreeSet<Tag>>,
    tombstones: BTreeSet<Tag>,
}

impl<T: Ord + Clone> OrSet<T> {
    /// An empty set.
    pub fn new() -> Self {
        Self {
            adds: BTreeMap::new(),
            tombstones: BTreeSet::new(),
        }
    }

    /// Add `value` with a caller-supplied unique `tag` (e.g. `(peer, counter)`).
    pub fn add(&mut self, value: T, tag: Tag) {
        self.adds.entry(value).or_default().insert(tag);
    }

    /// Remove `value` by tombstoning every add-tag currently observed for it.
    pub fn remove(&mut self, value: &T) {
        if let Some(tags) = self.adds.get(value) {
            for &t in tags {
                self.tombstones.insert(t);
            }
        }
    }

    /// Whether `value` is currently present (has a live, non-tombstoned tag).
    pub fn contains(&self, value: &T) -> bool {
        self.adds
            .get(value)
            .is_some_and(|tags| tags.iter().any(|t| !self.tombstones.contains(t)))
    }

    /// The current members, ascending.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.adds
            .iter()
            .filter(|(_, tags)| tags.iter().any(|t| !self.tombstones.contains(t)))
            .map(|(v, _)| v)
    }

    /// Number of current members.
    pub fn len(&self) -> usize {
        self.iter().count()
    }

    /// Whether there are no current members.
    pub fn is_empty(&self) -> bool {
        self.iter().next().is_none()
    }
}

impl<T: Ord + Clone> Lattice for OrSet<T> {
    fn merge(&mut self, other: &Self) {
        for (value, tags) in &other.adds {
            self.adds.entry(value.clone()).or_default().extend(tags);
        }
        self.tombstones.extend(&other.tombstones);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lattice::{assert_lattice_laws, join};

    #[test]
    fn lww_laws_and_semantics() {
        // Value tied to (ts,peer) so equal stamps imply equal value (well-defined).
        let mk = |ts: u64, peer: u64| LwwReg::new(ts * 100 + peer, ts, peer);
        let samples = [mk(1, 0), mk(1, 1), mk(2, 0), mk(3, 5)];
        assert_lattice_laws(&samples);

        // Higher timestamp wins; peer breaks ties.
        let a = LwwReg::new("a", 5, 1);
        let b = LwwReg::new("b", 7, 0);
        assert_eq!(join(a.clone(), &b).get(), &"b");
        let c = LwwReg::new("c", 7, 2); // same ts, higher peer
        assert_eq!(join(b, &c).get(), &"c");
    }

    #[test]
    fn gcounter_laws_and_sum() {
        let mut a = GCounter::new();
        a.inc(1, 3);
        a.inc(2, 4);
        let mut b = GCounter::new();
        b.inc(1, 5); // same peer, larger -> max wins on merge
        b.inc(3, 1);
        assert_lattice_laws(&[GCounter::new(), a.clone(), b.clone()]);

        let merged = join(a, &b);
        // peer1 = max(3,5)=5, peer2=4, peer3=1 -> 10
        assert_eq!(merged.value(), 10);
    }

    #[test]
    fn pncounter_signed_value() {
        let mut a = PNCounter::new();
        a.inc(1, 10);
        a.dec(1, 3);
        let mut b = PNCounter::new();
        b.dec(2, 2);
        assert_lattice_laws(&[PNCounter::new(), a.clone(), b.clone()]);
        assert_eq!(join(a, &b).value(), 5); // +10 -3 -2
    }

    #[test]
    fn orset_add_wins_and_converges() {
        // Two replicas start from a shared element, then act concurrently.
        let mut r1: OrSet<&str> = OrSet::new();
        r1.add("x", (1, 1));
        let mut r2 = r1.clone();

        // r1 removes x; r2 concurrently re-adds x with a fresh tag.
        r1.remove(&"x");
        r2.add("x", (2, 1));

        // Merge both ways -> converge, add-wins (x present).
        let m12 = join(r1.clone(), &r2);
        let m21 = join(r2, &r1);
        assert_eq!(m12, m21, "OrSet converges regardless of merge order");
        assert!(m12.contains(&"x"), "concurrent add beats remove (add-wins)");
    }

    #[test]
    fn orset_laws_and_membership() {
        let mut a: OrSet<u32> = OrSet::new();
        a.add(1, (1, 1));
        a.add(2, (1, 2));
        let mut b: OrSet<u32> = OrSet::new();
        b.add(2, (2, 1));
        b.add(3, (2, 2));
        b.remove(&2); // tombstones only b's own tag for 2
        assert_lattice_laws(&[OrSet::new(), a.clone(), b.clone()]);

        let m = join(a, &b);
        // 1 present; 2 present (a's tag survived b's remove — add-wins); 3 present.
        let mut members: Vec<_> = m.iter().copied().collect();
        members.sort_unstable();
        assert_eq!(members, vec![1, 2, 3]);
    }
}
