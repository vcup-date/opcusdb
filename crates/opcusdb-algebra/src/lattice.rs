//! The [`Lattice`] trait and law-checkers.
//!
//! See `CORE_SPEC.md` §7. A lattice's [`merge`](Lattice::merge) is a
//! join-semilattice operation: **commutative, associative, and idempotent**.
//! Those three laws are exactly what make CRDT replication conflict-free, any
//! order of merges from any set of replicas converges to the same value. The
//! [`assert_lattice_laws`] helper turns "the laws hold" into a test, not a hope.

/// A type whose values can be merged into a least upper bound.
///
/// Implementors must guarantee, for all `a`, `b`, `c`:
/// - **idempotent**: `merge(a, a) == a`
/// - **commutative**: `merge(a, b) == merge(b, a)`
/// - **associative**: `merge(merge(a, b), c) == merge(a, merge(b, c))`
pub trait Lattice: Clone {
    /// Merge `other` into `self`, raising `self` to the least upper bound of the two.
    fn merge(&mut self, other: &Self);
}

/// Functional form of [`Lattice::merge`]: return the join of `a` and `b` without
/// mutating the caller's bindings.
pub fn join<L: Lattice>(mut a: L, b: &L) -> L {
    a.merge(b);
    a
}

/// Assert the three lattice laws hold across every combination of `samples`.
///
/// O(n³) in the number of samples, keep the set small (≈4–8 diverse values).
/// Intended for use in CRDT unit tests.
pub fn assert_lattice_laws<L>(samples: &[L])
where
    L: Lattice + PartialEq + core::fmt::Debug,
{
    for a in samples {
        assert_eq!(join(a.clone(), a), *a, "idempotency failed: {a:?}");
        for b in samples {
            assert_eq!(
                join(a.clone(), b),
                join(b.clone(), a),
                "commutativity failed: {a:?} ⊔ {b:?}"
            );
            for c in samples {
                let left = join(join(a.clone(), b), c);
                let right = join(a.clone(), &join(b.clone(), c));
                assert_eq!(left, right, "associativity failed: {a:?} {b:?} {c:?}");
            }
        }
    }
}
