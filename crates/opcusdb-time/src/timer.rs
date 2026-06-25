//! Deterministic timers (`CORE_SPEC.md` §10).
//!
//! [`Timers<E>`] schedules payloads to fire at future ticks. It is the
//! deterministic clock behind statechart timeouts, debounces, and periodic
//! systems. Entries are kept in a min-heap keyed by `(fire_at, TimerId)`, so
//! [`due`](Timers::due) returns due payloads in a stable order — same-tick timers
//! fire in `TimerId` (i.e. scheduling) order, satisfying the determinism contract.

use crate::tick::Tick;
use std::collections::{BinaryHeap, BTreeSet};

/// A handle to a scheduled timer, unique within one [`Timers`].
pub type TimerId = u64;

/// One scheduled timer. Ordered solely by `(fire_at, id)`, and *reversed* so the
/// standard max-`BinaryHeap` behaves as a min-heap (earliest fires first). The
/// payload is intentionally excluded from ordering, so `E` need not be `Ord`.
#[derive(Clone)]
struct Scheduled<E> {
    fire_at: Tick,
    id: TimerId,
    /// `Some(period)` for a repeating timer, `None` for one-shot.
    repeat: Option<u64>,
    payload: E,
}

impl<E> PartialEq for Scheduled<E> {
    fn eq(&self, other: &Self) -> bool {
        self.fire_at == other.fire_at && self.id == other.id
    }
}
impl<E> Eq for Scheduled<E> {}
impl<E> PartialOrd for Scheduled<E> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<E> Ord for Scheduled<E> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        // Reverse: smaller (fire_at, id) must be "greater" so it sits at the top.
        (other.fire_at, other.id).cmp(&(self.fire_at, self.id))
    }
}

/// A scheduler of timed payloads of type `E`.
#[derive(Clone)]
pub struct Timers<E> {
    heap: BinaryHeap<Scheduled<E>>,
    next_id: TimerId,
    /// Ids cancelled before they fired; consumed lazily when popped.
    cancelled: BTreeSet<TimerId>,
}

impl<E> Default for Timers<E> {
    fn default() -> Self {
        Self {
            heap: BinaryHeap::new(),
            next_id: 0,
            cancelled: BTreeSet::new(),
        }
    }
}

impl<E> Timers<E> {
    /// An empty scheduler.
    pub fn new() -> Self {
        Self::default()
    }

    /// Schedule `payload` to fire once at `now + delay`. A `delay` of 0 fires on
    /// the next [`due`](Timers::due) call for `now`. Returns the timer's id.
    pub fn after(&mut self, now: Tick, delay: u64, payload: E) -> TimerId {
        let id = self.alloc_id();
        self.heap.push(Scheduled {
            fire_at: now + delay,
            id,
            repeat: None,
            payload,
        });
        id
    }

    /// Cancel a scheduled timer. A cancelled one-shot never fires; a cancelled
    /// repeating timer stops permanently. No-op if the id already fired/unknown.
    pub fn cancel(&mut self, id: TimerId) {
        self.cancelled.insert(id);
    }

    /// Number of timers still in the heap (includes not-yet-reaped cancellations).
    #[inline]
    pub fn len(&self) -> usize {
        self.heap.len()
    }

    /// Whether no timers are scheduled.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    fn alloc_id(&mut self) -> TimerId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

impl<E: Clone> Timers<E> {
    /// Schedule `payload` to fire every `period` ticks, starting at `now + period`.
    /// `period` is clamped to at least 1 to avoid a zero-delay loop. Returns the id.
    pub fn every(&mut self, now: Tick, period: u64, payload: E) -> TimerId {
        let period = period.max(1);
        let id = self.alloc_id();
        self.heap.push(Scheduled {
            fire_at: now + period,
            id,
            repeat: Some(period),
            payload,
        });
        id
    }

    /// Pop and return every payload due at or before `now`, in `(fire_at, id)`
    /// order. Repeating timers are re-scheduled at fixed cadence (`fire_at +
    /// period`), so an overdue repeater "catches up" one firing per missed period
    /// — under fixed-timestep stepping (`now` advances by 1) that is exactly once.
    pub fn due(&mut self, now: Tick) -> Vec<E> {
        let mut out = Vec::new();
        while let Some(top) = self.heap.peek() {
            if top.fire_at > now {
                break;
            }
            let s = self.heap.pop().expect("peeked, so present");
            if self.cancelled.remove(&s.id) {
                continue; // cancelled: drop without firing or rescheduling
            }
            if let Some(period) = s.repeat {
                self.heap.push(Scheduled {
                    fire_at: s.fire_at + period,
                    id: s.id,
                    repeat: Some(period),
                    payload: s.payload.clone(),
                });
            }
            out.push(s.payload);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_shot_fires_at_its_tick_only() {
        let mut t: Timers<&str> = Timers::new();
        t.after(Tick(0), 3, "ping");
        assert!(t.due(Tick(0)).is_empty());
        assert!(t.due(Tick(2)).is_empty());
        assert_eq!(t.due(Tick(3)), vec!["ping"]);
        assert!(t.due(Tick(4)).is_empty(), "one-shot does not repeat");
        assert!(t.is_empty());
    }

    #[test]
    fn same_tick_timers_fire_in_scheduling_order() {
        let mut t: Timers<u32> = Timers::new();
        t.after(Tick(0), 1, 10); // id 0
        t.after(Tick(0), 1, 20); // id 1
        t.after(Tick(0), 1, 30); // id 2
        assert_eq!(t.due(Tick(1)), vec![10, 20, 30], "ordered by TimerId");
    }

    #[test]
    fn cancel_prevents_firing() {
        let mut t: Timers<&str> = Timers::new();
        let id = t.after(Tick(0), 2, "x");
        t.cancel(id);
        assert!(t.due(Tick(5)).is_empty());
    }

    #[test]
    fn repeating_timer_fires_each_period() {
        let mut t: Timers<&str> = Timers::new();
        let id = t.every(Tick(0), 2, "tick");
        assert!(t.due(Tick(1)).is_empty());
        assert_eq!(t.due(Tick(2)), vec!["tick"]);
        assert!(t.due(Tick(3)).is_empty());
        assert_eq!(t.due(Tick(4)), vec!["tick"]);
        // Cancel mid-stream stops it.
        t.cancel(id);
        assert!(t.due(Tick(6)).is_empty());
    }

    #[test]
    fn overdue_repeater_catches_up_per_period() {
        let mut t: Timers<u8> = Timers::new();
        t.every(Tick(0), 2, 1);
        // Jump straight to tick 6: should have fired at 2, 4, 6 -> three times.
        assert_eq!(t.due(Tick(6)), vec![1, 1, 1]);
    }

    #[test]
    fn deterministic_for_same_schedule() {
        let build = || {
            let mut t: Timers<u32> = Timers::new();
            t.after(Tick(0), 2, 1);
            t.every(Tick(0), 3, 2);
            t.after(Tick(0), 2, 3);
            t
        };
        let mut a = build();
        let mut b = build();
        for now in 1..=9u64 {
            assert_eq!(a.due(Tick(now)), b.due(Tick(now)), "tick {now}");
        }
    }
}
