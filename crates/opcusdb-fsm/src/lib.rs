//! `opcusdb-fsm` — hierarchical + parallel statechart engine (`CORE_SPEC.md` §11).
//!
//! See [`statechart`] for the model. This is the engine behind `fsm-lab`; it is
//! also what MMO gameplay states and chat moderation reuse. The engine is pure
//! and deterministic, and machine state is `Clone`, so it composes with the
//! Timeline (rollback/replay) directly.

pub mod statechart;

pub use statechart::{
    Action, ChartBuilder, Guard, MachineState, StateChart, StateId, StateKind,
};

#[cfg(test)]
mod tests {
    use super::statechart::*;

    /// Event kinds used across the tests.
    #[derive(Clone, PartialEq, Eq, Debug)]
    enum Sig {
        Tick,
        Go,
    }

    /// Context records a trace of entry/exit/action labels (to assert ordering)
    /// plus a couple of flags/counters for guard and internal-transition tests.
    #[derive(Clone, Debug, PartialEq, Eq, Default)]
    struct Ctx {
        trace: Vec<&'static str>,
        allow: bool,
        ready: bool,
        count: u32,
    }

    fn log(label: &'static str) -> Action<Ctx, Sig> {
        Box::new(move |c: &mut Ctx| {
            c.trace.push(label);
            vec![]
        })
    }

    #[test]
    fn flat_cycle() {
        let mut b = ChartBuilder::<Ctx, Sig>::new();
        let root = b.compound("root", None);
        let a = b.leaf("a", Some(root));
        let bb = b.leaf("b", Some(root));
        let c = b.leaf("c", Some(root));
        b.initial(root, a);
        b.transition(a, Some(Sig::Tick), None, Some(bb), None);
        b.transition(bb, Some(Sig::Tick), None, Some(c), None);
        b.transition(c, Some(Sig::Tick), None, Some(a), None);
        let chart = b.build();

        let mut st = MachineState::new(Ctx::default());
        chart.start(&mut st);
        assert!(st.is_active(a));
        chart.send(&mut st, Sig::Tick);
        assert!(st.is_active(bb) && !st.is_active(a));
        chart.send(&mut st, Sig::Tick);
        assert!(st.is_active(c));
        chart.send(&mut st, Sig::Tick);
        assert!(st.is_active(a), "cycled back");
    }

    #[test]
    fn ancestor_transition_and_child_priority() {
        // root > sub(compound: x initial, y) ; root also has leaf z.
        let mut b = ChartBuilder::<Ctx, Sig>::new();
        let root = b.compound("root", None);
        let sub = b.compound("sub", Some(root));
        let x = b.leaf("x", Some(sub));
        let y = b.leaf("y", Some(sub));
        let z = b.leaf("z", Some(root));
        b.initial(root, sub);
        b.initial(sub, x);
        // Ancestor `sub` handles Tick -> z; child `x` also handles Tick -> y.
        b.transition(sub, Some(Sig::Tick), None, Some(z), None);
        b.transition(x, Some(Sig::Tick), None, Some(y), None);
        let chart = b.build();

        let mut st = MachineState::new(Ctx::default());
        chart.start(&mut st);
        assert!(st.is_active(x));
        chart.send(&mut st, Sig::Tick);
        // Child wins over ancestor: we go to y, not z.
        assert!(st.is_active(y), "deeper transition has priority");
        assert!(!st.is_active(z));

        // Now from y, only the ancestor handles Tick -> z.
        chart.send(&mut st, Sig::Tick);
        assert!(st.is_active(z) && !st.is_active(sub));
    }

    #[test]
    fn guard_blocks_then_allows() {
        let mut b = ChartBuilder::<Ctx, Sig>::new();
        let root = b.compound("root", None);
        let a = b.leaf("a", Some(root));
        let bb = b.leaf("b", Some(root));
        b.initial(root, a);
        b.transition(
            a,
            Some(Sig::Go),
            Some(Box::new(|c: &Ctx| c.allow)),
            Some(bb),
            None,
        );
        let chart = b.build();

        let mut st = MachineState::new(Ctx::default());
        chart.start(&mut st);
        chart.send(&mut st, Sig::Go);
        assert!(st.is_active(a), "guard false blocks transition");

        st.ctx.allow = true;
        chart.send(&mut st, Sig::Go);
        assert!(st.is_active(bb), "guard true permits transition");
    }

    #[test]
    fn eventless_transition_settles_on_start() {
        let mut b = ChartBuilder::<Ctx, Sig>::new();
        let root = b.compound("root", None);
        let a = b.leaf("a", Some(root));
        let bb = b.leaf("b", Some(root));
        b.initial(root, a);
        // Automatic transition a -> b, gated by ctx.ready.
        b.transition(a, None, Some(Box::new(|c: &Ctx| c.ready)), Some(bb), None);
        let chart = b.build();

        let mut st = MachineState::new(Ctx {
            ready: true,
            ..Default::default()
        });
        chart.start(&mut st);
        assert!(st.is_active(bb), "eventless transition fired during RTC");
    }

    #[test]
    fn entry_exit_ordering_is_deepest_exit_first_shallowest_entry_first() {
        // root > s1(compound: a initial) ; root > s2(compound: c initial)
        let mut b = ChartBuilder::<Ctx, Sig>::new();
        let root = b.compound("root", None);
        let s1 = b.compound("s1", Some(root));
        let a = b.leaf("a", Some(s1));
        let s2 = b.compound("s2", Some(root));
        let c = b.leaf("c", Some(s2));
        b.initial(root, s1);
        b.initial(s1, a);
        b.initial(s2, c);
        b.on_exit(a, log("exit:a"));
        b.on_exit(s1, log("exit:s1"));
        b.on_entry(s2, log("enter:s2"));
        b.on_entry(c, log("enter:c"));
        b.transition(a, Some(Sig::Go), None, Some(s2), None);
        let chart = b.build();

        let mut st = MachineState::new(Ctx::default());
        chart.start(&mut st);
        st.ctx.trace.clear();
        chart.send(&mut st, Sig::Go);
        // Exit deepest-first (a then s1), then enter shallowest-first (s2 then c).
        assert_eq!(st.ctx.trace, vec!["exit:a", "exit:s1", "enter:s2", "enter:c"]);
        assert!(st.is_active(c));
    }

    #[test]
    fn parallel_regions_transition_simultaneously() {
        // root > p(parallel) > { r1(compound: a1 init, a2), r2(compound: b1 init, b2) }
        let mut b = ChartBuilder::<Ctx, Sig>::new();
        let root = b.compound("root", None);
        let p = b.parallel("p", Some(root));
        let r1 = b.compound("r1", Some(p));
        let a1 = b.leaf("a1", Some(r1));
        let a2 = b.leaf("a2", Some(r1));
        let r2 = b.compound("r2", Some(p));
        let b1 = b.leaf("b1", Some(r2));
        let b2 = b.leaf("b2", Some(r2));
        b.initial(root, p);
        b.initial(r1, a1);
        b.initial(r2, b1);
        b.transition(a1, Some(Sig::Tick), None, Some(a2), None);
        b.transition(b1, Some(Sig::Tick), None, Some(b2), None);
        let chart = b.build();

        let mut st = MachineState::new(Ctx::default());
        chart.start(&mut st);
        // Both regions active at their initials.
        assert!(st.is_active(a1) && st.is_active(b1));
        chart.send(&mut st, Sig::Tick);
        // One event advances BOTH regions (disjoint exit sets).
        assert!(st.is_active(a2) && st.is_active(b2));
        assert!(!st.is_active(a1) && !st.is_active(b1));
    }

    #[test]
    fn internal_transition_runs_action_without_state_change() {
        let mut b = ChartBuilder::<Ctx, Sig>::new();
        let root = b.compound("root", None);
        let a = b.leaf("a", Some(root));
        b.initial(root, a);
        b.on_entry(a, log("enter:a")); // should run once (at start), not on Tick
        b.transition(
            a,
            Some(Sig::Tick),
            None,
            None, // internal: no target
            Some(Box::new(|c: &mut Ctx| {
                c.count += 1;
                vec![]
            })),
        );
        let chart = b.build();

        let mut st = MachineState::new(Ctx::default());
        chart.start(&mut st);
        chart.send(&mut st, Sig::Tick);
        chart.send(&mut st, Sig::Tick);
        assert!(st.is_active(a));
        assert_eq!(st.ctx.count, 2, "internal action ran each time");
        assert_eq!(
            st.ctx.trace,
            vec!["enter:a"],
            "no re-entry on internal transition"
        );
    }

    #[test]
    fn deterministic_for_same_event_sequence() {
        let build = || {
            let mut b = ChartBuilder::<Ctx, Sig>::new();
            let root = b.compound("root", None);
            let a = b.leaf("a", Some(root));
            let bb = b.leaf("b", Some(root));
            b.initial(root, a);
            b.transition(a, Some(Sig::Tick), None, Some(bb), None);
            b.transition(bb, Some(Sig::Tick), None, Some(a), None);
            b.build()
        };
        let seq = [Sig::Tick, Sig::Tick, Sig::Tick];
        let run = |chart: &StateChart<Ctx, Sig>| {
            let mut st = MachineState::new(Ctx::default());
            chart.start(&mut st);
            for s in &seq {
                chart.send(&mut st, s.clone());
            }
            st
        };
        let c1 = build();
        let c2 = build();
        assert_eq!(run(&c1), run(&c2));
    }
}
