//! `Rga`, a Replicated Growable Array (ordered sequence CRDT).
//!
//! See `CORE_SPEC.md` §7. An RGA is the conflict-free data type for ordered
//! text and chat: concurrent inserts and deletes from many peers merge into one
//! deterministic sequence. It is the substrate for the human+AI chatroom demo
//! (`DESIGN.md` §7, §9).
//!
//! Model (a causal tree): every inserted element has a unique [`OpId`] and an
//! `after` anchor (the element it was inserted to the right of, or the head).
//! The visible order is a pre-order traversal of that tree where **siblings sort
//! by id descending**, a fixed rule, so all replicas converge. Deletes are
//! monotonic tombstones (the node stays to anchor its children). Merge is the
//! union of nodes with delete-flags OR'd: commutative, associative, idempotent.

use crate::lattice::Lattice;
use std::collections::BTreeMap;

/// A globally-unique id for one insertion: a Lamport timestamp plus the peer.
/// Ordered by `(lamport, peer)`, which both totally orders ids and breaks
/// concurrent-insert ties deterministically.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct OpId {
    /// Logical clock value.
    pub lamport: u64,
    /// The originating peer.
    pub peer: u64,
}

impl OpId {
    /// Construct an id.
    pub fn new(lamport: u64, peer: u64) -> Self {
        Self { lamport, peer }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Node<T> {
    /// The element this was inserted after (`None` = at the head).
    after: Option<OpId>,
    value: T,
    deleted: bool,
}

/// A replicated growable array of `T`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rga<T> {
    nodes: BTreeMap<OpId, Node<T>>,
}

impl<T> Default for Rga<T> {
    fn default() -> Self {
        Self {
            nodes: BTreeMap::new(),
        }
    }
}

impl<T> Rga<T> {
    /// An empty sequence.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert `value` with unique `id`, positioned immediately after `after`
    /// (or at the head if `after` is `None`). The caller owns id allocation
    /// (typically a per-peer Lamport counter) to keep things deterministic.
    pub fn insert_after(&mut self, after: Option<OpId>, value: T, id: OpId) {
        self.nodes.insert(
            id,
            Node {
                after,
                value,
                deleted: false,
            },
        );
    }

    /// Append `value` at the end of the current visible sequence.
    pub fn append(&mut self, value: T, id: OpId) {
        let after = self.last_id();
        self.insert_after(after, value, id);
    }

    /// Tombstone the element with `id` (idempotent; unknown ids are ignored).
    pub fn delete(&mut self, id: OpId) {
        if let Some(n) = self.nodes.get_mut(&id) {
            n.deleted = true;
        }
    }

    /// All element ids in sequence order, including tombstoned ones.
    pub fn ordered_ids(&self) -> Vec<OpId> {
        // Group children by their anchor, each sibling list sorted descending.
        let mut children: BTreeMap<Option<OpId>, Vec<OpId>> = BTreeMap::new();
        for (id, node) in &self.nodes {
            children.entry(node.after).or_default().push(*id);
        }
        for v in children.values_mut() {
            v.sort_unstable_by(|a, b| b.cmp(a));
        }
        // Iterative pre-order DFS so deep (append-chain) sequences don't recurse.
        let mut out = Vec::with_capacity(self.nodes.len());
        let mut stack: Vec<OpId> = Vec::new();
        if let Some(roots) = children.get(&None) {
            // Push reversed so the highest-descending root is popped first.
            stack.extend(roots.iter().rev().copied());
        }
        while let Some(id) = stack.pop() {
            out.push(id);
            if let Some(kids) = children.get(&Some(id)) {
                stack.extend(kids.iter().rev().copied());
            }
        }
        out
    }

    /// The visible (non-deleted) elements in order.
    pub fn to_vec(&self) -> Vec<&T> {
        self.ordered_ids()
            .into_iter()
            .filter_map(|id| {
                let n = &self.nodes[&id];
                (!n.deleted).then_some(&n.value)
            })
            .collect()
    }

    /// The id of the last element in sequence order (deleted or not), for anchoring.
    pub fn last_id(&self) -> Option<OpId> {
        self.ordered_ids().last().copied()
    }

    /// Number of visible elements.
    pub fn len(&self) -> usize {
        self.nodes.values().filter(|n| !n.deleted).count()
    }

    /// Whether there are no visible elements.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<T: Clone> Lattice for Rga<T> {
    fn merge(&mut self, other: &Self) {
        for (id, node) in &other.nodes {
            match self.nodes.get_mut(id) {
                // Same insertion op everywhere; only the tombstone can change.
                Some(existing) => existing.deleted |= node.deleted,
                None => {
                    self.nodes.insert(*id, node.clone());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lattice::{assert_lattice_laws, join};

    fn op(l: u64, p: u64) -> OpId {
        OpId::new(l, p)
    }

    fn text(r: &Rga<char>) -> String {
        r.to_vec().into_iter().collect()
    }

    #[test]
    fn append_keeps_order() {
        let mut r: Rga<char> = Rga::new();
        r.append('h', op(1, 1));
        r.append('i', op(2, 1));
        r.append('!', op(3, 1));
        assert_eq!(text(&r), "hi!");
        assert_eq!(r.len(), 3);
    }

    #[test]
    fn delete_tombstones_but_keeps_others() {
        let mut r: Rga<char> = Rga::new();
        r.append('a', op(1, 1));
        r.append('b', op(2, 1));
        r.append('c', op(3, 1));
        r.delete(op(2, 1));
        assert_eq!(text(&r), "ac");
        r.delete(op(2, 1)); // idempotent
        assert_eq!(text(&r), "ac");
    }

    #[test]
    fn concurrent_inserts_converge() {
        // Common base "a", then two peers concurrently append after it.
        let mut base: Rga<char> = Rga::new();
        base.append('a', op(1, 1));
        let mut p1 = base.clone();
        let mut p2 = base.clone();
        p1.insert_after(Some(op(1, 1)), 'x', op(2, 1)); // peer 1
        p2.insert_after(Some(op(1, 1)), 'y', op(2, 2)); // peer 2 (higher peer)

        let m12 = join(p1.clone(), &p2);
        let m21 = join(p2, &p1);
        assert_eq!(m12, m21, "merge order-independent");
        // Siblings sort by id descending: op(2,2) before op(2,1) -> "ayx".
        assert_eq!(text(&m12), "ayx");
    }

    #[test]
    fn merge_is_idempotent_and_associative_over_samples() {
        let mut a: Rga<char> = Rga::new();
        a.append('a', op(1, 1));
        a.append('b', op(2, 1));
        let mut b: Rga<char> = Rga::new();
        b.insert_after(None, 'c', op(1, 2));
        b.delete(op(1, 2));
        let mut c = a.clone();
        c.delete(op(1, 1));
        assert_lattice_laws(&[Rga::new(), a, b, c]);
    }

    #[test]
    fn deletes_merge_monotonically() {
        let mut p1: Rga<char> = Rga::new();
        p1.append('a', op(1, 1));
        let mut p2 = p1.clone();
        p2.delete(op(1, 1)); // peer 2 deletes
        let merged = join(p1, &p2);
        assert!(merged.is_empty(), "a delete on one replica wins after merge");
    }
}
