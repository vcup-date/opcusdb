//! Scene B, a quest statechart (`CORE_SPEC.md` §12).
//!
//! Demonstrates the statechart features the intersection doesn't stress:
//! - **deep hierarchy**: `quest > active > {collecting, escorting}`;
//! - **context-driven guards**: progress gated by collected/escorted counts;
//! - an **ancestor transition**: `active --Timeout--> failed` fires from any
//!   active substate (collecting *or* escorting);
//! - **internal transitions**: `CollectItem`/`EscortStep` mutate context without
//!   changing state.
//!
//! Like the intersection it is a [`Sim`] driven by the [`Timeline`], so it is
//! deterministic, replayable, and rollback-able.

use opcusdb_fsm::{Action, ChartBuilder, Guard, MachineState, StateChart};
use opcusdb_time::{Sim, Tick, Timers};
use std::rc::Rc;

/// Default quest parameters.
const DEFAULT_DEADLINE: u64 = 100;
const DEFAULT_NEEDED: u32 = 3;
const DEFAULT_TARGET: u32 = 2;

/// Events that drive the quest.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum QSig {
    /// Begin the quest (`not_started -> active`).
    Start,
    /// Pick up one required item.
    CollectItem,
    /// Advance the escort by one step.
    EscortStep,
    /// The deadline elapsed (delivered by a timer).
    Timeout,
}

/// Observable quest phase, set by each leaf's entry action.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum QPhase {
    /// Not yet begun.
    #[default]
    NotStarted,
    /// Gathering items.
    Collecting,
    /// Escorting to the destination.
    Escorting,
    /// Finished successfully.
    Completed,
    /// Failed (timed out).
    Failed,
}

/// Quest context: progress counters, the parameters guards read, and the
/// driver-mediated timer request.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct QCtx {
    /// Current phase (mirror of the active leaf, for easy inspection).
    pub phase: QPhase,
    /// Items collected so far.
    pub items: u32,
    /// Items required to advance to escorting.
    pub needed: u32,
    /// Escort steps taken so far.
    pub escort: u32,
    /// Escort steps required to complete.
    pub target: u32,
    /// Ticks allowed once active before timing out.
    pub deadline: u64,
    /// Set by `active`'s entry: schedule a timeout this many ticks out.
    pub pending_timer: Option<u64>,
}

fn set_phase(p: QPhase) -> Action<QCtx, QSig> {
    Box::new(move |c: &mut QCtx| {
        c.phase = p;
        vec![]
    })
}

fn guard(f: impl Fn(&QCtx) -> bool + 'static) -> Guard<QCtx> {
    Box::new(f)
}

fn build_quest_chart() -> StateChart<QCtx, QSig> {
    let mut b = ChartBuilder::<QCtx, QSig>::new();

    let quest = b.compound("quest", None);
    let not_started = b.leaf("not_started", Some(quest));
    let active = b.compound("active", Some(quest));
    let collecting = b.leaf("collecting", Some(active));
    let escorting = b.leaf("escorting", Some(active));
    let completed = b.leaf("completed", Some(quest));
    let failed = b.leaf("failed", Some(quest));
    b.initial(quest, not_started);
    b.initial(active, collecting);

    b.on_entry(not_started, set_phase(QPhase::NotStarted));
    b.on_entry(collecting, set_phase(QPhase::Collecting));
    b.on_entry(escorting, set_phase(QPhase::Escorting));
    b.on_entry(completed, set_phase(QPhase::Completed));
    b.on_entry(failed, set_phase(QPhase::Failed));
    // Entering `active` arms the deadline timer (the driver schedules it).
    b.on_entry(
        active,
        Box::new(|c: &mut QCtx| {
            c.pending_timer = Some(c.deadline);
            vec![]
        }),
    );

    // Begin.
    b.transition(not_started, Some(QSig::Start), None, Some(active), None);
    // Collecting: each item is an internal transition; an eventless guarded
    // transition advances once enough are held.
    b.transition(
        collecting,
        Some(QSig::CollectItem),
        None,
        None,
        Some(Box::new(|c: &mut QCtx| {
            c.items += 1;
            vec![]
        })),
    );
    b.transition(
        collecting,
        None,
        Some(guard(|c| c.items >= c.needed)),
        Some(escorting),
        None,
    );
    // Escorting: same shape, completing when the target is reached.
    b.transition(
        escorting,
        Some(QSig::EscortStep),
        None,
        None,
        Some(Box::new(|c: &mut QCtx| {
            c.escort += 1;
            vec![]
        })),
    );
    b.transition(
        escorting,
        None,
        Some(guard(|c| c.escort >= c.target)),
        Some(completed),
        None,
    );
    // Ancestor transition: a timeout anywhere in `active` fails the quest.
    b.transition(active, Some(QSig::Timeout), None, Some(failed), None);

    b.build()
}

/// A quest as a [`Sim`]. Shared immutable chart + live machine state + timers.
#[derive(Clone)]
pub struct Quest {
    chart: Rc<StateChart<QCtx, QSig>>,
    state: MachineState<QCtx>,
    timers: Timers<QSig>,
}

impl Default for Quest {
    fn default() -> Self {
        Self::new()
    }
}

impl Quest {
    /// A quest with default parameters.
    pub fn new() -> Self {
        Self::new_with(DEFAULT_DEADLINE, DEFAULT_NEEDED, DEFAULT_TARGET)
    }

    /// A quest with explicit deadline / items-needed / escort-target.
    pub fn new_with(deadline: u64, needed: u32, target: u32) -> Self {
        let chart = Rc::new(build_quest_chart());
        let mut state = MachineState::new(QCtx {
            needed,
            target,
            deadline,
            ..Default::default()
        });
        chart.start(&mut state); // enters not_started; no timer until active
        Self {
            chart,
            state,
            timers: Timers::new(),
        }
    }

    /// The current phase.
    pub fn phase(&self) -> QPhase {
        self.state.ctx.phase
    }

    /// The full machine state (configuration + context).
    pub fn machine(&self) -> &MachineState<QCtx> {
        &self.state
    }

    fn fire(&mut self, ev: QSig, now: Tick) {
        self.chart.send(&mut self.state, ev);
        if let Some(d) = self.state.ctx.pending_timer.take() {
            self.timers.after(now, d, QSig::Timeout);
        }
    }
}

impl Sim for Quest {
    type Input = QSig;

    fn step(&mut self, tick: Tick, inputs: &[QSig]) {
        for ev in inputs {
            self.fire(ev.clone(), tick);
        }
        for ev in self.timers.due(tick) {
            self.fire(ev, tick);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opcusdb_time::Timeline;

    fn drive(quest: Quest, per_tick: Vec<Vec<QSig>>) -> Timeline<Quest> {
        let mut tl = Timeline::new(quest, 8, 4);
        for evs in per_tick {
            tl.advance(evs);
        }
        tl
    }

    #[test]
    fn happy_path_completes() {
        use QSig::*;
        let tl = drive(
            Quest::new(),
            vec![
                vec![Start],
                vec![CollectItem],
                vec![CollectItem],
                vec![CollectItem], // 3 >= needed -> auto escorting
                vec![EscortStep],
                vec![EscortStep], // 2 >= target -> auto completed
            ],
        );
        assert_eq!(tl.state().phase(), QPhase::Completed);
    }

    #[test]
    fn guard_gates_progress() {
        use QSig::*;
        // Only 2 of 3 items: must remain collecting.
        let tl = drive(Quest::new(), vec![vec![Start], vec![CollectItem], vec![CollectItem]]);
        assert_eq!(tl.state().phase(), QPhase::Collecting);
    }

    #[test]
    fn timeout_from_collecting_fails() {
        use QSig::*;
        // deadline 5: start, then idle past it.
        let tl = drive(
            Quest::new_with(5, 3, 2),
            vec![vec![Start], vec![], vec![], vec![], vec![], vec![]],
        );
        assert_eq!(tl.state().phase(), QPhase::Failed);
    }

    #[test]
    fn ancestor_timeout_fires_from_escorting() {
        use QSig::*;
        // Reach escorting (3 items), then time out before completing.
        let tl = drive(
            Quest::new_with(8, 3, 5),
            vec![
                vec![Start],
                vec![CollectItem, CollectItem, CollectItem], // -> escorting
                vec![EscortStep],                            // not enough (target 5)
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
                vec![], // tick 8 -> Timeout
            ],
        );
        assert_eq!(tl.state().phase(), QPhase::Failed);
    }

    #[test]
    fn completing_before_deadline_ignores_timeout() {
        use QSig::*;
        // Finish quickly with a short deadline; the stray Timeout must be a no-op
        // because `active` is no longer in the configuration.
        let tl = drive(
            Quest::new_with(4, 1, 1),
            vec![vec![Start, CollectItem, EscortStep], vec![], vec![], vec![], vec![], vec![]],
        );
        assert_eq!(tl.state().phase(), QPhase::Completed);
    }

    #[test]
    fn replay_and_rollback_reproduce() {
        use QSig::*;
        let script = vec![
            vec![Start],
            vec![CollectItem],
            vec![CollectItem],
            vec![CollectItem],
            vec![EscortStep],
            vec![EscortStep],
        ];
        let tl = drive(Quest::new(), script.clone());
        // Replay from a fresh quest.
        let replayed = Timeline::replay(Quest::new(), tl.log());
        assert_eq!(replayed.machine(), tl.state().machine());

        // Rollback to tick 2, then re-simulate the exact original inputs from there.
        let mut tl2 = drive(Quest::new(), script.clone());
        let final_machine = tl2.state().machine().clone();
        assert!(tl2.seek(2));
        for evs in script[2..].iter().cloned() {
            tl2.advance(evs);
        }
        assert_eq!(tl2.state().machine(), &final_machine);
    }
}
