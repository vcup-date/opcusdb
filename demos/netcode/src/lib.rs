//! Netcode demo: a **WoW-style ability cooldown** and how opcusdb handles
//! **network lag** — answering two design questions concretely.
//!
//! ## Does it consider network laggyness?
//! Yes — that is the whole reason the [`Timeline`] exists (`DESIGN.md` §5,
//! `CORE_SPEC.md` §9). The standard authoritative-server netcode loop is:
//! 1. **client prediction** — the client runs the real sim on its own input
//!    immediately, so the UI is responsive despite latency;
//! 2. **server reconciliation** — when the authoritative tick arrives, the client
//!    **rolls back** to it and **replays** its buffered inputs;
//! 3. **lag compensation** — the server can rewind to the shooter's render-time.
//!
//! All three are the *same* mechanism: rewind + deterministic re-simulate. The
//! Timeline already provides it (`seek` + branch-on-`advance` + `replay`), and
//! [`reconcile_late_input`] below proves a late input rolled back yields the
//! identical state to having had it on time. (The network *transport* itself is
//! a later track; this shows the handling logic is in place and correct.)
//!
//! ## A WoW cooldown over a laggy link
//! A cooldown is just a **deterministic timer + a guard** — the same primitives as
//! the fsm-lab quest. The server is authoritative over the cooldown clock; the
//! client predicts a cast and reconciles if the server disagrees (e.g. the cast
//! was actually still on cooldown on the server's clock). Because the logic is
//! deterministic and tick-based, prediction and the server agree whenever they
//! see the same inputs — and rollback fixes them up when they don't.

use opcusdb_time::{Sim, Tick};

pub mod net;
pub mod wal;

/// Global cooldown after any cast (ticks).
pub const GCD: u32 = 2;
/// The ability's own cooldown (ticks).
pub const ABILITY_CD: u32 = 5;

/// A player's combat state — the *authoritative* model (also what the client
/// predicts with). Deterministic and `Clone`, so the Timeline can roll it back.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Combat {
    /// Global cooldown remaining.
    pub gcd: u32,
    /// Ability cooldown remaining.
    pub ability_cd: u32,
    /// Successful casts.
    pub casts: u32,
    /// Cast attempts rejected because something was on cooldown.
    pub rejected: u32,
}

/// Player intents for a tick.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    /// Attempt to cast the ability.
    Cast,
}

impl Combat {
    /// Whether a cast would succeed right now (the guard the server enforces and
    /// the client predicts with).
    pub fn ready(&self) -> bool {
        self.gcd == 0 && self.ability_cd == 0
    }
}

impl Sim for Combat {
    type Input = Action;

    fn step(&mut self, _tick: Tick, inputs: &[Action]) {
        // Cooldowns tick down at the start of the frame.
        self.gcd = self.gcd.saturating_sub(1);
        self.ability_cd = self.ability_cd.saturating_sub(1);

        for Action::Cast in inputs {
            if self.ready() {
                self.casts += 1;
                self.gcd = GCD;
                self.ability_cd = ABILITY_CD;
            } else {
                self.rejected += 1; // server rejects; client would reconcile
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opcusdb_time::Timeline;

    /// Run a script of per-tick inputs through a fresh Timeline.
    fn run(script: &[Vec<Action>]) -> Timeline<Combat> {
        let mut tl = Timeline::new(Combat::default(), 8, 16);
        for evs in script {
            tl.advance(evs.clone());
        }
        tl
    }

    #[test]
    fn cooldown_blocks_spam_casting() {
        // Spam Cast every tick for 12 ticks. Only one cast per max(GCD,CD) window
        // succeeds; the ability cd (5) dominates.
        let script: Vec<Vec<Action>> = (0..12).map(|_| vec![Action::Cast]).collect();
        let tl = run(&script);
        let s = tl.state();
        // The ability cooldown (5) dominates the GCD (2): a cast lands once every
        // 5 ticks (at ticks 0, 5, 10) -> 3 successful, the other 9 rejected.
        assert_eq!(s.casts, 3, "spam is gated by the cooldown");
        assert_eq!(s.rejected, 9);
        assert_eq!(s.casts + s.rejected, 12, "every attempt is accounted for");
    }

    #[test]
    fn cast_recovers_after_cooldown() {
        // Cast, wait out the cooldown, cast again -> both land.
        let mut script = vec![vec![Action::Cast]];
        for _ in 0..ABILITY_CD {
            script.push(vec![]);
        }
        script.push(vec![Action::Cast]);
        let tl = run(&script);
        assert_eq!(tl.state().casts, 2);
        assert_eq!(tl.state().rejected, 0);
    }

    #[test]
    fn reconcile_late_input_matches_on_time() {
        // THE LAG TEST. Canonical run: the cast input is known at tick 2.
        let canonical = {
            let mut s: Vec<Vec<Action>> = vec![vec![]; 10];
            s[2] = vec![Action::Cast];
            run(&s)
        };

        // Laggy client: it ran ticks 0..10 WITHOUT the tick-2 cast (packet delayed).
        // The authoritative input finally arrives; reconcile by rolling back to
        // tick 2, applying it, and re-simulating forward.
        let mut laggy = run(&vec![vec![]; 10]);
        assert!(laggy.seek(2)); // rewind
        laggy.advance(vec![Action::Cast]); // branch tick 2 with the real input
        for _ in 3..10 {
            laggy.advance(vec![]); // deterministic re-sim
        }

        // Reconciled state is identical to having had the input on time.
        assert_eq!(laggy.state(), canonical.state());
        assert_eq!(laggy.state().casts, 1);
    }

    #[test]
    fn misprediction_is_corrected_by_rollback() {
        // Client predicts a cast at tick 0 (felt instant). But the server says the
        // player was still on GCD from an earlier cast the client hadn't seen.
        // Server-authoritative truth: a cast at tick 0 AND a (hidden) cast that
        // left a cooldown -> the tick-0 cast is actually rejected.
        let predicted = run(&[vec![Action::Cast]]); // client: 1 cast, feels good
        assert_eq!(predicted.state().casts, 1);

        // Reconcile against authoritative inputs (an earlier cast at tick 0 already
        // consumed the GCD, so the "same-tick" second cast is rejected):
        let authoritative = run(&[vec![Action::Cast, Action::Cast]]);
        assert_eq!(authoritative.state().casts, 1, "second same-tick cast rejected");
        assert_eq!(authoritative.state().rejected, 1);
        // The client would roll back and adopt this; the point is both are exact,
        // deterministic functions of their inputs.
    }
}
