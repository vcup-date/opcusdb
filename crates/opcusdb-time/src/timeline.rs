//! The Timeline (`CORE_SPEC.md` §9): fixed-timestep stepping, a keyframe ring,
//! and rollback / replay over a deterministic simulation.
//!
//! State is treated as a fold over an input log: `state(t) = step^t(initial,
//! log)`. Because [`Sim::step`] is deterministic, the Timeline can rebuild any
//! tick from a keyframe + the log — which is the single mechanism behind
//! rollback (netcode), the `fsm-lab` time scrubber, and byte-identical replay.
//!
//! The input log is the source of truth and is always retained, so full-history
//! [`replay`](Timeline::replay) and [`seek`](Timeline::seek) to any past tick
//! always work. Keyframes are a *bounded* ring that merely speeds up recent
//! seeks; the initial state is kept separately as a baseline for older seeks.
//!
//! Generic over the state type so it does not depend on `World` serialization
//! (that wiring comes once `World` is snapshot-able — CORE_SPEC §14 Q1).

use crate::tick::Tick;
use std::collections::VecDeque;

/// A deterministic simulation: the state advances one tick at a time given that
/// tick's inputs. Must be pure (no wall-clock / ambient randomness) so replay is
/// exact. `Clone` is required so the Timeline can snapshot keyframes.
pub trait Sim: Clone {
    /// Per-tick input applied by [`step`](Sim::step).
    type Input;

    /// Advance exactly one tick, applying `inputs`. Deterministic in `(self, tick, inputs)`.
    fn step(&mut self, tick: Tick, inputs: &[Self::Input]);
}

/// A rollback-capable timeline over a [`Sim`].
pub struct Timeline<S: Sim> {
    /// The state at tick 0 — the always-available seek baseline.
    initial: S,
    /// The live state, at tick [`Self::tick`].
    state: S,
    /// Number of steps taken so far (the current tick).
    tick: u64,
    /// `log[t]` is the inputs applied at tick `t` (producing the state at `t+1`).
    log: Vec<Vec<S::Input>>,
    /// Recent `(tick, state)` keyframes, ascending by tick, bounded by `max_keyframes`.
    keyframes: VecDeque<(u64, S)>,
    /// Take a keyframe whenever the tick is a multiple of this (>= 1).
    snapshot_every: u64,
    /// Maximum keyframes retained (the rollback acceleration window).
    max_keyframes: usize,
}

impl<S: Sim> Timeline<S> {
    /// Create a timeline starting from `initial`. `snapshot_every` (clamped to >=1)
    /// is the keyframe cadence; `max_keyframes` (clamped to >=1) bounds the ring.
    pub fn new(initial: S, snapshot_every: u64, max_keyframes: usize) -> Self {
        Self {
            state: initial.clone(),
            initial,
            tick: 0,
            log: Vec::new(),
            keyframes: VecDeque::new(),
            snapshot_every: snapshot_every.max(1),
            max_keyframes: max_keyframes.max(1),
        }
    }

    /// The current tick (number of steps taken).
    #[inline]
    pub fn tick(&self) -> Tick {
        Tick(self.tick)
    }

    /// The highest recorded tick (length of the log).
    #[inline]
    pub fn len(&self) -> u64 {
        self.log.len() as u64
    }

    /// Whether nothing has been recorded yet.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.log.is_empty()
    }

    /// The current state.
    #[inline]
    pub fn state(&self) -> &S {
        &self.state
    }

    /// The full input log (the replayable source of truth).
    #[inline]
    pub fn log(&self) -> &[Vec<S::Input>] {
        &self.log
    }

    /// Step one tick forward, applying `inputs`. If the timeline is currently
    /// seeked into the past (tick < log length), the future is first truncated —
    /// i.e. this branches history (rollback-then-resimulate).
    pub fn advance(&mut self, inputs: Vec<S::Input>) {
        let t = self.tick;
        // Branch: drop any recorded future beyond the current tick.
        if (t as usize) < self.log.len() {
            self.log.truncate(t as usize);
            while matches!(self.keyframes.back(), Some(&(kt, _)) if kt > t) {
                self.keyframes.pop_back();
            }
        }
        self.log.push(inputs);
        // Disjoint field borrows: &mut self.state and &self.log[t].
        self.state.step(Tick(t), &self.log[t as usize]);
        self.tick = t + 1;

        if self.tick % self.snapshot_every == 0 {
            self.keyframes.push_back((self.tick, self.state.clone()));
            if self.keyframes.len() > self.max_keyframes {
                self.keyframes.pop_front();
            }
        }
    }

    /// Restore the live state to `target` (a tick in `0..=len`). Returns `false`
    /// if `target` is out of range. Uses the nearest keyframe `<= target` (or the
    /// initial baseline) and replays the log forward — works backward or forward.
    pub fn seek(&mut self, target: u64) -> bool {
        if target > self.log.len() as u64 {
            return false;
        }
        // Best baseline: the latest keyframe at or before target, else initial@0.
        let mut base_tick = 0u64;
        let mut base_state = &self.initial;
        for (kt, ks) in &self.keyframes {
            if *kt <= target && *kt >= base_tick {
                base_tick = *kt;
                base_state = ks;
            }
        }
        let mut state = base_state.clone();
        for t in base_tick..target {
            state.step(Tick(t), &self.log[t as usize]);
        }
        self.state = state;
        self.tick = target;
        true
    }

    /// Replay an input log from `initial` into the final state, ignoring any
    /// timeline instance. The oracle for replay-determinism tests.
    pub fn replay(initial: S, log: &[Vec<S::Input>]) -> S {
        let mut state = initial;
        for (t, inputs) in log.iter().enumerate() {
            state.step(Tick(t as u64), inputs);
        }
        state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A trivial deterministic sim: sum of all inputs ever applied.
    #[derive(Clone, Debug, PartialEq)]
    struct Counter {
        value: i64,
    }
    impl Sim for Counter {
        type Input = i64;
        fn step(&mut self, _tick: Tick, inputs: &[i64]) {
            for i in inputs {
                self.value += i;
            }
        }
    }

    fn run(n: i64, every: u64, max_kf: usize) -> Timeline<Counter> {
        let mut tl = Timeline::new(Counter { value: 0 }, every, max_kf);
        for i in 1..=n {
            tl.advance(vec![i]); // add 1, 2, 3, ... -> triangular numbers
        }
        tl
    }

    #[test]
    fn advance_accumulates() {
        let tl = run(5, 4, 4);
        assert_eq!(tl.tick(), Tick(5));
        assert_eq!(tl.state().value, 15); // 1+2+3+4+5
        assert_eq!(tl.len(), 5);
    }

    #[test]
    fn replay_matches_live_state() {
        // Acceptance: replaying the log reproduces the live state exactly.
        let tl = run(10, 3, 3);
        let replayed = Timeline::replay(Counter { value: 0 }, tl.log());
        assert_eq!(&replayed, tl.state());
    }

    #[test]
    fn seek_back_then_forward_is_lossless() {
        let mut tl = run(10, 3, 3);
        let final_state = tl.state().clone();
        assert!(tl.seek(4));
        assert_eq!(tl.state().value, 10); // 1+2+3+4
        assert!(tl.seek(10));
        assert_eq!(tl.state(), &final_state, "scrub forward restores exactly");
    }

    #[test]
    fn seek_to_zero_works_after_keyframe_eviction() {
        // Tiny ring forces eviction of early keyframes; initial baseline saves us.
        let mut tl = run(20, 2, 2);
        assert!(tl.seek(0));
        assert_eq!(tl.state().value, 0);
        assert!(tl.seek(3));
        assert_eq!(tl.state().value, 6); // 1+2+3
    }

    #[test]
    fn rollback_then_resim_same_inputs_reproduces() {
        // Acceptance #2: rewind to T, re-advance identical inputs -> identical state.
        let original = run(8, 4, 4);
        let final_state = original.state().clone();

        let mut tl = run(8, 4, 4);
        assert!(tl.seek(3)); // rewind to tick 3
                             // Re-advance ticks 3..8 with the SAME inputs (4,5,6,7,8).
        for i in 4..=8i64 {
            tl.advance(vec![i]);
        }
        assert_eq!(tl.state(), &final_state);
        assert_eq!(tl.len(), 8);
    }

    #[test]
    fn advance_after_seek_branches_history() {
        let mut tl = run(5, 8, 4); // 1+2+3+4+5 = 15
        assert!(tl.seek(2)); // value = 3 (1+2)
        tl.advance(vec![100]); // branch: new tick 2
        assert_eq!(tl.tick(), Tick(3));
        assert_eq!(tl.state().value, 103);
        assert_eq!(tl.len(), 3, "future truncated to the new branch");
    }

    #[test]
    fn out_of_range_seek_is_rejected() {
        let mut tl = run(3, 2, 2);
        assert!(!tl.seek(99));
        // State unchanged after a rejected seek.
        assert_eq!(tl.state().value, 6);
    }

    #[test]
    fn deterministic_across_two_runs() {
        let a = run(12, 3, 3);
        let b = run(12, 3, 3);
        assert_eq!(a.state(), b.state());
        assert_eq!(a.log(), b.log());
    }
}
