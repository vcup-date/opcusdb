//! `reduce` and `fold` — the event-sourcing primitives.
//!
//! See `CORE_SPEC.md` §7. A [`Reduce`] state transition must be **pure and
//! total**: folding the same events from the same starting state always yields
//! the same result. This is the determinism that lets the Timeline (§9) rebuild
//! any state as `fold(snapshot, events)` and replay byte-for-byte. A statechart
//! transition (§11) is just a `Reduce`.

/// A state that evolves by applying events one at a time.
pub trait Reduce {
    /// The event type this state consumes.
    type Event;

    /// Apply a single event. Must be deterministic (no wall-clock, no ambient
    /// randomness — see the determinism contract, §2).
    fn reduce(&mut self, event: &Self::Event);
}

/// Fold a slice of events into `state`, left to right. Equivalent to replaying a
/// log: `fold(initial, &log) == the state after those events`.
pub fn fold<S: Reduce>(mut state: S, events: &[S::Event]) -> S {
    for e in events {
        state.reduce(e);
    }
    state
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny bank account: a state machine over deposit/withdraw events.
    #[derive(Clone, Debug, PartialEq)]
    struct Account {
        balance: i64,
    }
    enum Tx {
        Deposit(i64),
        Withdraw(i64),
    }
    impl Reduce for Account {
        type Event = Tx;
        fn reduce(&mut self, event: &Tx) {
            match event {
                Tx::Deposit(n) => self.balance += n,
                // Total: an overdraw is clamped, not a panic (purity/totality).
                Tx::Withdraw(n) => self.balance -= n.min(&self.balance.max(0)),
            }
        }
    }

    #[test]
    fn fold_applies_events_in_order() {
        let log = [Tx::Deposit(100), Tx::Withdraw(30), Tx::Deposit(5)];
        let acct = fold(Account { balance: 0 }, &log);
        assert_eq!(acct.balance, 75);
    }

    #[test]
    fn fold_is_deterministic() {
        let log = [Tx::Deposit(10), Tx::Withdraw(3), Tx::Deposit(7)];
        let a = fold(Account { balance: 0 }, &log);
        let b = fold(Account { balance: 0 }, &log);
        assert_eq!(a, b);
    }

    #[test]
    fn withdraw_is_total_no_negative() {
        let log = [Tx::Deposit(5), Tx::Withdraw(100)];
        let acct = fold(Account { balance: 0 }, &log);
        assert_eq!(acct.balance, 0, "overdraw clamps rather than panicking");
    }
}
