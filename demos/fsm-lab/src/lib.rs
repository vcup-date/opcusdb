//! `fsm-lab`, a deterministic, replayable traffic intersection.
//!
//! Demonstrates the opcusdb core composing end-to-end (`CORE_SPEC.md` §12):
//! - a **hierarchical + parallel statechart** ([`opcusdb_fsm`]) models the
//!   intersection as two orthogonal regions, the car lights and the pedestrian
//!   signal, coordinated by a **cross-region interlock guard** (the walk signal
//!   may only activate while both car axes are red);
//! - **deterministic timers** ([`opcusdb_time::Timers`]) drive the phase clock;
//! - the whole thing is a [`Sim`] driven by the **[`Timeline`]**, so it gets
//!   rollback, time-scrubbing, and byte-identical replay for free.
//!
//! Safety by construction: car phases are mutually exclusive, so the two axes are
//! never simultaneously "go", verified as an invariant across long randomized runs.

use opcusdb_core::Rng;
use opcusdb_fsm::{Action, ChartBuilder, Guard, MachineState, StateChart};
use opcusdb_time::{Sim, Tick, Timers};
use std::rc::Rc;

pub mod quest;
pub mod record;

/// Default seed for car arrivals when none is given.
pub const DEFAULT_SEED: u64 = 0xC0FFEE;
/// Per-axis probability a car arrives each tick, as `num/den`.
const ARRIVAL_NUM: u32 = 2;
const ARRIVAL_DEN: u32 = 5;
/// Cars that can clear the intersection per tick while the light is "go".
const CROSS_RATE: u32 = 2;

/// One approach lane's aggregate state. Cars are modeled as counts (not entities)
/// so the whole sim stays `Clone` and thus rollback/replay-able by the Timeline.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Lane {
    /// Cars currently waiting.
    pub waiting: u32,
    /// Total cars that have crossed (throughput).
    pub crossed: u32,
    /// Accumulated car·tick waiting time (sum of `waiting` each tick).
    pub wait_ticks: u64,
    /// High-water mark of the queue.
    pub max_queue: u32,
}

impl Lane {
    fn arrive(&mut self) {
        self.waiting += 1;
        self.max_queue = self.max_queue.max(self.waiting);
    }
    fn cross_if(&mut self, go: bool) {
        if go {
            let n = self.waiting.min(CROSS_RATE);
            self.waiting -= n;
            self.crossed += n;
        }
    }
    fn accrue(&mut self) {
        self.wait_ticks += self.waiting as u64;
    }
}

/// Traffic flowing through the intersection: a lane per axis plus the seeded RNG
/// that drives deterministic car arrivals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Traffic {
    /// North-south approach.
    pub ns: Lane,
    /// East-west approach.
    pub ew: Lane,
    rng: Rng,
}

impl Traffic {
    fn new(seed: u64) -> Self {
        Self {
            ns: Lane::default(),
            ew: Lane::default(),
            rng: Rng::seed(seed),
        }
    }

    /// Advance one tick given whether each axis is currently "go". Arrivals are
    /// drawn in a fixed order (ns then ew) so the RNG stream is deterministic.
    fn update(&mut self, ns_go: bool, ew_go: bool) {
        if self.rng.chance(ARRIVAL_NUM, ARRIVAL_DEN) {
            self.ns.arrive();
        }
        if self.rng.chance(ARRIVAL_NUM, ARRIVAL_DEN) {
            self.ew.arrive();
        }
        self.ns.cross_if(ns_go);
        self.ew.cross_if(ew_go);
        self.ns.accrue();
        self.ew.accrue();
    }
}

/// A single traffic light's colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Light {
    /// Stop.
    #[default]
    Red,
    /// Go.
    Green,
    /// Caution.
    Yellow,
}

impl Light {
    /// Whether traffic may be in the intersection (green or yellow).
    pub fn is_go(self) -> bool {
        matches!(self, Light::Green | Light::Yellow)
    }
}

/// The event kind that advances the light phase clock.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Sig {
    /// The current phase's timer elapsed; advance to the next phase.
    Phase,
}

/// The statechart's context: the observable signal state plus a one-shot request
/// to the driver to schedule the next phase timer.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Ctx {
    /// North-south car light.
    pub ns: Light,
    /// East-west car light.
    pub ew: Light,
    /// Whether the pedestrian "walk" signal is active.
    pub walk: bool,
    /// Whether both car axes are currently red (the interlock condition).
    pub all_red: bool,
    /// Set by a phase's entry action: how many ticks until the next phase.
    /// The driver consumes this after each run-to-completion.
    pub pending_timer: Option<u64>,
}

fn phase_entry(ns: Light, ew: Light, all_red: bool, dur: u64) -> Action<Ctx, Sig> {
    Box::new(move |c: &mut Ctx| {
        c.ns = ns;
        c.ew = ew;
        c.all_red = all_red;
        c.pending_timer = Some(dur);
        vec![]
    })
}

fn set_walk(walking: bool) -> Action<Ctx, Sig> {
    Box::new(move |c: &mut Ctx| {
        c.walk = walking;
        vec![]
    })
}

fn guard_all_red(want: bool) -> Guard<Ctx> {
    Box::new(move |c: &Ctx| c.all_red == want)
}

/// Build the intersection statechart: a parallel root with a `lights` region
/// (the phase cycle) and a `ped` region (the interlocked walk signal).
fn build_chart() -> StateChart<Ctx, Sig> {
    use Light::{Green, Red, Yellow};
    let mut b = ChartBuilder::<Ctx, Sig>::new();

    let system = b.parallel("system", None);

    // --- lights region: the phase cycle ---------------------------------
    let lights = b.compound("lights", Some(system));
    let ns_green = b.leaf("ns_green", Some(lights));
    let ns_yellow = b.leaf("ns_yellow", Some(lights));
    let all_red_1 = b.leaf("all_red_1", Some(lights));
    let ew_green = b.leaf("ew_green", Some(lights));
    let ew_yellow = b.leaf("ew_yellow", Some(lights));
    let all_red_2 = b.leaf("all_red_2", Some(lights));
    b.initial(lights, ns_green);

    // Phase durations (ticks). All-red interlock between every axis switch.
    b.on_entry(ns_green, phase_entry(Green, Red, false, 3));
    b.on_entry(ns_yellow, phase_entry(Yellow, Red, false, 1));
    b.on_entry(all_red_1, phase_entry(Red, Red, true, 1));
    b.on_entry(ew_green, phase_entry(Red, Green, false, 3));
    b.on_entry(ew_yellow, phase_entry(Red, Yellow, false, 1));
    b.on_entry(all_red_2, phase_entry(Red, Red, true, 1));

    // The cycle, each step driven by a Phase timer event.
    b.transition(ns_green, Some(Sig::Phase), None, Some(ns_yellow), None);
    b.transition(ns_yellow, Some(Sig::Phase), None, Some(all_red_1), None);
    b.transition(all_red_1, Some(Sig::Phase), None, Some(ew_green), None);
    b.transition(ew_green, Some(Sig::Phase), None, Some(ew_yellow), None);
    b.transition(ew_yellow, Some(Sig::Phase), None, Some(all_red_2), None);
    b.transition(all_red_2, Some(Sig::Phase), None, Some(ns_green), None);

    // --- pedestrian region: interlocked with the lights -----------------
    let ped = b.compound("ped", Some(system));
    let dont_walk = b.leaf("dont_walk", Some(ped));
    let walk = b.leaf("walk", Some(ped));
    b.initial(ped, dont_walk);
    b.on_entry(walk, set_walk(true));
    b.on_entry(dont_walk, set_walk(false));
    // Eventless transitions gated by the interlock guard, which reads context the
    // lights region writes, so the walk signal activates only during all-red.
    b.transition(dont_walk, None, Some(guard_all_red(true)), Some(walk), None);
    b.transition(walk, None, Some(guard_all_red(false)), Some(dont_walk), None);

    b.build()
}

/// The intersection as a [`Sim`]: an immutable chart shared via `Rc`, the live
/// machine state, and the timer queue. `Clone` (cheap, the `Rc` is shared) so
/// the [`Timeline`] can snapshot/rollback it.
#[derive(Clone)]
pub struct Intersection {
    chart: Rc<StateChart<Ctx, Sig>>,
    state: MachineState<Ctx>,
    timers: Timers<Sig>,
    traffic: Traffic,
}

impl Default for Intersection {
    fn default() -> Self {
        Self::new()
    }
}

impl Intersection {
    /// Build a started intersection with the default car-arrival seed.
    pub fn new() -> Self {
        Self::new_seeded(DEFAULT_SEED)
    }

    /// Build a started intersection with its first phase timer armed and car
    /// arrivals seeded by `seed`.
    pub fn new_seeded(seed: u64) -> Self {
        let chart = Rc::new(build_chart());
        let mut state = MachineState::new(Ctx::default());
        chart.start(&mut state);
        let mut timers = Timers::new();
        if let Some(d) = state.ctx.pending_timer.take() {
            timers.after(Tick::ZERO, d, Sig::Phase);
        }
        Self {
            chart,
            state,
            timers,
            traffic: Traffic::new(seed),
        }
    }

    /// The current observable context (light colours, walk signal).
    pub fn ctx(&self) -> &Ctx {
        &self.state.ctx
    }

    /// The current traffic state (per-axis queues and metrics).
    pub fn traffic(&self) -> &Traffic {
        &self.traffic
    }

    /// The full machine state (configuration + context), the snapshot-comparable
    /// logical state, excluding the shared immutable chart.
    pub fn machine(&self) -> &MachineState<Ctx> {
        &self.state
    }

    /// Run one event through the chart, then arm the next phase timer if the
    /// entered phase requested one.
    fn fire(&mut self, ev: Sig, now: Tick) {
        self.chart.send(&mut self.state, ev);
        if let Some(d) = self.state.ctx.pending_timer.take() {
            self.timers.after(now, d, Sig::Phase);
        }
    }
}

impl Sim for Intersection {
    /// External events (none in this scene, but the channel exists).
    type Input = Sig;

    fn step(&mut self, tick: Tick, inputs: &[Sig]) {
        // 1. Advance the light phases (external events, then due timers).
        for ev in inputs {
            self.fire(ev.clone(), tick);
        }
        for ev in self.timers.due(tick) {
            self.fire(ev, tick);
        }
        // 2. Cars react to the (now updated) light colours.
        let ns_go = self.state.ctx.ns.is_go();
        let ew_go = self.state.ctx.ew.is_go();
        self.traffic.update(ns_go, ew_go);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opcusdb_time::Timeline;

    fn run(ticks: u64) -> Timeline<Intersection> {
        let mut tl = Timeline::new(Intersection::new(), 8, 4);
        for _ in 0..ticks {
            tl.advance(vec![]);
        }
        tl
    }

    #[test]
    fn safety_invariant_never_two_go_axes() {
        // Step through several full cycles, checking the invariant every tick.
        let mut tl = Timeline::new(Intersection::new(), 8, 4);
        for _ in 0..200 {
            tl.advance(vec![]);
            let c = tl.state().ctx();
            assert!(
                !(c.ns.is_go() && c.ew.is_go()),
                "crossing greens at tick {:?}: ns={:?} ew={:?}",
                tl.tick(),
                c.ns,
                c.ew
            );
            // Interlock: the walk signal only ever shows during all-red.
            assert!(!c.walk || c.all_red, "walk while not all-red");
        }
    }

    #[test]
    fn pedestrian_walks_during_all_red() {
        // Over a couple of cycles, the walk signal must activate at least once,
        // and exactly when both axes are red.
        let mut tl = Timeline::new(Intersection::new(), 8, 4);
        let mut saw_walk = false;
        for _ in 0..40 {
            tl.advance(vec![]);
            let c = tl.state().ctx();
            if c.walk {
                saw_walk = true;
                assert!(c.all_red, "walk implies all-red");
            }
        }
        assert!(saw_walk, "pedestrian never got to walk");
    }

    #[test]
    fn phases_cycle_in_order() {
        // Sample the colours over the first full cycle (3+1+1+3+1+1 = 10 ticks).
        let mut tl = Timeline::new(Intersection::new(), 8, 4);
        let mut colours = Vec::new();
        for _ in 0..10 {
            tl.advance(vec![]);
            let c = tl.state().ctx();
            colours.push((c.ns, c.ew));
        }
        use Light::{Green, Red, Yellow};
        // ns green for ticks 0..3 then yellow, all-red, ew green x3, ew yellow, all-red.
        assert_eq!(
            colours,
            vec![
                (Green, Red),
                (Green, Red),
                (Green, Red),
                (Yellow, Red),
                (Red, Red),
                (Red, Green),
                (Red, Green),
                (Red, Green),
                (Red, Yellow),
                (Red, Red),
            ]
        );
    }

    #[test]
    fn replay_reproduces_live_state() {
        // Acceptance #1: replaying the log from a fresh start equals the live
        // state, including the RNG-driven traffic (replay-safe randomness).
        let tl = run(57);
        let replayed = Timeline::replay(Intersection::new(), tl.log());
        assert_eq!(replayed.machine(), tl.state().machine());
        assert_eq!(replayed.ctx(), tl.state().ctx());
        assert_eq!(replayed.traffic(), tl.state().traffic());
    }

    #[test]
    fn rollback_then_resim_reproduces() {
        // Acceptance #2: rewind to a past tick and re-simulate -> identical state,
        // traffic and RNG included.
        let original = run(50);
        let final_machine = original.state().machine().clone();
        let final_traffic = original.state().traffic().clone();

        let mut tl = run(50);
        assert!(tl.seek(17));
        for _ in 17..50 {
            tl.advance(vec![]);
        }
        assert_eq!(tl.state().machine(), &final_machine);
        assert_eq!(tl.state().traffic(), &final_traffic);
    }

    #[test]
    fn cars_flow_and_queues_stay_bounded() {
        let tl = run(300);
        let t = tl.state().traffic();
        assert!(t.ns.crossed > 0 && t.ew.crossed > 0, "cars crossed both axes");
        // Throughput (cross_rate over the green share) outpaces arrivals, so the
        // queues must not blow up.
        assert!(
            t.ns.max_queue < 40 && t.ew.max_queue < 40,
            "queues bounded: ns={} ew={}",
            t.ns.max_queue,
            t.ew.max_queue
        );
    }

    #[test]
    fn different_seeds_give_different_traffic() {
        let run_seed = |seed: u64| {
            let mut tl = Timeline::new(Intersection::new_seeded(seed), 8, 4);
            for _ in 0..120 {
                tl.advance(vec![]);
            }
            tl.state().traffic().clone()
        };
        assert_ne!(run_seed(1), run_seed(2));
    }

    #[test]
    fn scrub_back_and_forward_is_lossless() {
        let mut tl = run(40);
        let final_ctx = tl.state().ctx().clone();
        assert!(tl.seek(5));
        assert!(tl.seek(40));
        assert_eq!(tl.state().ctx(), &final_ctx);
    }
}
