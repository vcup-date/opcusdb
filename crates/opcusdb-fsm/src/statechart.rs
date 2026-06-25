//! Hierarchical + parallel statechart engine (`CORE_SPEC.md` §11).
//!
//! An SCXML-class statechart: states form a tree of **compound** (one active
//! child), **parallel** (all children active — orthogonal regions), and **leaf**
//! nodes. Events drive transitions with **run-to-completion** semantics, where
//! the active **configuration** is the set of currently-active states.
//!
//! Two-part design so machine state stays snapshot-friendly:
//! - [`StateChart`] is the immutable definition (holds the guard/action closures);
//!   shared, never cloned.
//! - [`MachineState`] is `{ config, ctx }` — `Clone` when `Ctx: Clone`, so the
//!   Timeline (§9) can snapshot/rollback a running machine.
//!
//! A transition *is* a `reduce` over the context; a guard *is* a pure predicate;
//! the machine's history *is* a `fold` of its events — so replay/rollback is free.
//!
//! Implemented: compound/parallel/leaf, LCA-based exit/entry, child-over-ancestor
//! priority with document-order tie-break, non-conflicting parallel transitions,
//! eventless (automatic) transitions, internal transitions, raised internal events.

use std::collections::{BTreeSet, VecDeque};

/// A predicate over the context deciding whether a transition is enabled.
pub type Guard<Ctx> = Box<dyn Fn(&Ctx) -> bool>;
/// An action run on entry/exit/transition. Mutates context and may raise events
/// (returned as a list of event kinds, processed within the same RTC step).
pub type Action<Ctx, K> = Box<dyn Fn(&mut Ctx) -> Vec<K>>;

/// Identifies a state within one [`StateChart`]; also its document order.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct StateId(pub usize);

/// The kind of a state node.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StateKind {
    /// No children; an atomic state.
    Leaf,
    /// Exactly one child active at a time (its `initial`).
    Compound,
    /// All children active simultaneously (orthogonal regions).
    Parallel,
}

struct Transition<Ctx, K> {
    trigger: Option<K>, // None = eventless / automatic
    guard: Option<Guard<Ctx>>,
    target: Option<StateId>, // None = internal transition (action only)
    action: Option<Action<Ctx, K>>,
}

struct Node<Ctx, K> {
    #[allow(dead_code)]
    label: &'static str,
    parent: Option<StateId>,
    depth: u32,
    kind: StateKind,
    children: Vec<StateId>,
    initial: Option<StateId>,
    on_entry: Option<Action<Ctx, K>>,
    on_exit: Option<Action<Ctx, K>>,
    transitions: Vec<Transition<Ctx, K>>,
}

/// The mutable, snapshot-friendly part of a running machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MachineState<Ctx> {
    /// All currently-active states (compound + parallel ancestors and leaves).
    pub config: BTreeSet<StateId>,
    /// Local context data the actions/guards read and write.
    pub ctx: Ctx,
}

impl<Ctx> MachineState<Ctx> {
    /// A machine state with an empty configuration and the given context.
    pub fn new(ctx: Ctx) -> Self {
        Self {
            config: BTreeSet::new(),
            ctx,
        }
    }

    /// Whether `id` is currently active.
    pub fn is_active(&self, id: StateId) -> bool {
        self.config.contains(&id)
    }
}

/// The immutable statechart definition. Build one with [`ChartBuilder`].
pub struct StateChart<Ctx, K> {
    nodes: Vec<Node<Ctx, K>>,
    root: StateId,
}

/// Maximum microsteps per run-to-completion, as a backstop against an
/// always-enabled eventless transition (a chart bug). Generous; real charts settle fast.
const RTC_LIMIT: u32 = 100_000;

impl<Ctx, K: PartialEq + Clone> StateChart<Ctx, K> {
    /// The root state id.
    pub fn root(&self) -> StateId {
        self.root
    }

    /// Initialize `st` into the chart's initial configuration, then settle all
    /// eventless transitions (run-to-completion).
    pub fn start(&self, st: &mut MachineState<Ctx>) {
        st.config.clear();
        let mut queue: VecDeque<K> = VecDeque::new();
        self.enter(st, self.root, &mut queue);
        self.run_to_completion(st, queue);
    }

    /// Send an external event of kind `k` and run to completion.
    pub fn send(&self, st: &mut MachineState<Ctx>, k: K) {
        let mut queue = VecDeque::new();
        queue.push_back(k);
        self.run_to_completion(st, queue);
    }

    // --- run-to-completion ------------------------------------------------

    fn run_to_completion(&self, st: &mut MachineState<Ctx>, mut queue: VecDeque<K>) {
        let mut steps = 0u32;
        loop {
            steps += 1;
            if steps > RTC_LIMIT {
                debug_assert!(false, "statechart RTC did not settle (eventless cycle?)");
                break;
            }
            // Fire any enabled eventless transitions first.
            if self.microstep(st, None, &mut queue) {
                continue;
            }
            // Then process one queued (triggered) event.
            if let Some(k) = queue.pop_front() {
                self.microstep(st, Some(&k), &mut queue);
                continue;
            }
            break;
        }
    }

    /// One microstep for `trigger`: select non-conflicting enabled transitions and
    /// apply them. Returns whether anything fired.
    fn microstep(
        &self,
        st: &mut MachineState<Ctx>,
        trigger: Option<&K>,
        queue: &mut VecDeque<K>,
    ) -> bool {
        // 1. Collect enabled candidates: source active, trigger matches, guard true.
        let mut cands: Vec<(StateId, usize)> = Vec::new();
        for &sid in &st.config {
            for (ti, tr) in self.nodes[sid.0].transitions.iter().enumerate() {
                let matches = match (&tr.trigger, trigger) {
                    (None, None) => true,
                    (Some(a), Some(b)) => a == b,
                    _ => false,
                };
                if matches && tr.guard.as_ref().map_or(true, |g| g(&st.ctx)) {
                    cands.push((sid, ti));
                }
            }
        }
        if cands.is_empty() {
            return false;
        }
        // 2. Priority: deeper source first, then document order (state id, then index).
        cands.sort_by(|&(a, ai), &(b, bi)| {
            self.nodes[b.0]
                .depth
                .cmp(&self.nodes[a.0].depth)
                .then(a.cmp(&b))
                .then(ai.cmp(&bi))
        });
        // 3. Greedily select transitions with disjoint exit sets (parallel regions
        //    can fire together; conflicts resolve to the higher-priority one).
        let mut exited: BTreeSet<StateId> = BTreeSet::new();
        let mut chosen: Vec<(StateId, usize, BTreeSet<StateId>)> = Vec::new();
        for (sid, ti) in cands {
            let exset = match self.nodes[sid.0].transitions[ti].target {
                Some(target) => self.exit_set(st, sid, target),
                None => BTreeSet::new(),
            };
            if exset.is_disjoint(&exited) {
                exited.extend(&exset);
                chosen.push((sid, ti, exset));
            }
        }
        if chosen.is_empty() {
            return false;
        }
        // 4. Apply each chosen transition.
        for (sid, ti, exset) in chosen {
            let tr = &self.nodes[sid.0].transitions[ti];
            match tr.target {
                None => {
                    // Internal transition: action only, no state change.
                    if let Some(a) = &tr.action {
                        queue.extend(a(&mut st.ctx));
                    }
                }
                Some(target) => {
                    let domain = self.lca(sid, target);
                    // Exit deepest-first.
                    let mut to_exit: Vec<StateId> = exset.into_iter().collect();
                    to_exit.sort_by_key(|s| core::cmp::Reverse(self.nodes[s.0].depth));
                    for s in to_exit {
                        if let Some(a) = &self.nodes[s.0].on_exit {
                            queue.extend(a(&mut st.ctx));
                        }
                        st.config.remove(&s);
                    }
                    // Transition action.
                    if let Some(a) = &self.nodes[sid.0].transitions[ti].action {
                        queue.extend(a(&mut st.ctx));
                    }
                    // Enter from domain down to target (shallowest-first), completing target.
                    self.enter_path(st, domain, target, queue);
                }
            }
        }
        true
    }

    // --- entry / exit helpers --------------------------------------------

    fn enter(&self, st: &mut MachineState<Ctx>, id: StateId, queue: &mut VecDeque<K>) {
        st.config.insert(id);
        if let Some(a) = &self.nodes[id.0].on_entry {
            queue.extend(a(&mut st.ctx));
        }
        self.complete_entry(st, id, queue);
    }

    /// After a state is in the config, activate its required descendants.
    fn complete_entry(&self, st: &mut MachineState<Ctx>, id: StateId, queue: &mut VecDeque<K>) {
        match self.nodes[id.0].kind {
            StateKind::Leaf => {}
            StateKind::Compound => {
                let init = self.nodes[id.0]
                    .initial
                    .expect("compound state must have an initial child");
                self.enter(st, init, queue);
            }
            StateKind::Parallel => {
                for &c in &self.nodes[id.0].children {
                    self.enter(st, c, queue);
                }
            }
        }
    }

    /// Enter the chain from just below `domain` down to `target`, then complete
    /// `target`. Intermediate parallel states also enter their off-path regions.
    fn enter_path(
        &self,
        st: &mut MachineState<Ctx>,
        domain: StateId,
        target: StateId,
        queue: &mut VecDeque<K>,
    ) {
        let mut chain = Vec::new();
        let mut cur = target;
        while cur != domain {
            chain.push(cur);
            cur = self.nodes[cur.0]
                .parent
                .expect("target must be a descendant of domain");
        }
        chain.reverse();

        if chain.is_empty() {
            // target == domain (e.g. self-transition to an ancestor): re-complete it.
            self.complete_entry(st, target, queue);
            return;
        }
        let last = chain.len() - 1;
        for (i, &id) in chain.iter().enumerate() {
            st.config.insert(id);
            if let Some(a) = &self.nodes[id.0].on_entry {
                queue.extend(a(&mut st.ctx));
            }
            if i == last {
                self.complete_entry(st, id, queue);
            } else if self.nodes[id.0].kind == StateKind::Parallel {
                // Enter sibling regions not on the path with their defaults.
                let next_on_path = chain[i + 1];
                let others: Vec<StateId> = self.nodes[id.0]
                    .children
                    .iter()
                    .copied()
                    .filter(|&c| c != next_on_path)
                    .collect();
                for c in others {
                    self.enter(st, c, queue);
                }
            }
        }
    }

    /// Active states that must be exited for a transition `source -> target`:
    /// the active proper descendants of the transition domain (LCA).
    fn exit_set(&self, st: &MachineState<Ctx>, source: StateId, target: StateId) -> BTreeSet<StateId> {
        let domain = self.lca(source, target);
        st.config
            .iter()
            .copied()
            .filter(|&s| self.is_proper_descendant(s, domain))
            .collect()
    }

    fn is_proper_descendant(&self, s: StateId, ancestor: StateId) -> bool {
        let mut cur = self.nodes[s.0].parent;
        while let Some(p) = cur {
            if p == ancestor {
                return true;
            }
            cur = self.nodes[p.0].parent;
        }
        false
    }

    /// Least common ancestor of `a` and `b` (they share the root).
    fn lca(&self, a: StateId, b: StateId) -> StateId {
        let mut anc = BTreeSet::new();
        let mut cur = Some(a);
        while let Some(c) = cur {
            anc.insert(c);
            cur = self.nodes[c.0].parent;
        }
        let mut x = b;
        loop {
            if anc.contains(&x) {
                return x;
            }
            x = self.nodes[x.0].parent.expect("a and b share the root");
        }
    }
}

/// Builder for a [`StateChart`].
pub struct ChartBuilder<Ctx, K> {
    nodes: Vec<Node<Ctx, K>>,
}

impl<Ctx, K: PartialEq + Clone> Default for ChartBuilder<Ctx, K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Ctx, K: PartialEq + Clone> ChartBuilder<Ctx, K> {
    /// A new, empty builder.
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    fn add(&mut self, label: &'static str, parent: Option<StateId>, kind: StateKind) -> StateId {
        let id = StateId(self.nodes.len());
        let depth = match parent {
            Some(p) => self.nodes[p.0].depth + 1,
            None => 0,
        };
        self.nodes.push(Node {
            label,
            parent,
            depth,
            kind,
            children: Vec::new(),
            initial: None,
            on_entry: None,
            on_exit: None,
            transitions: Vec::new(),
        });
        if let Some(p) = parent {
            self.nodes[p.0].children.push(id);
        }
        id
    }

    /// Add a compound state (one active child at a time).
    pub fn compound(&mut self, label: &'static str, parent: Option<StateId>) -> StateId {
        self.add(label, parent, StateKind::Compound)
    }

    /// Add a parallel state (all children active simultaneously).
    pub fn parallel(&mut self, label: &'static str, parent: Option<StateId>) -> StateId {
        self.add(label, parent, StateKind::Parallel)
    }

    /// Add a leaf (atomic) state.
    pub fn leaf(&mut self, label: &'static str, parent: Option<StateId>) -> StateId {
        self.add(label, parent, StateKind::Leaf)
    }

    /// Set the default initial child of a compound state.
    pub fn initial(&mut self, parent: StateId, child: StateId) {
        self.nodes[parent.0].initial = Some(child);
    }

    /// Set a state's entry action.
    pub fn on_entry(&mut self, id: StateId, action: Action<Ctx, K>) {
        self.nodes[id.0].on_entry = Some(action);
    }

    /// Set a state's exit action.
    pub fn on_exit(&mut self, id: StateId, action: Action<Ctx, K>) {
        self.nodes[id.0].on_exit = Some(action);
    }

    /// Add a transition from `from`. `trigger` `None` is eventless (automatic);
    /// `target` `None` is an internal transition (runs `action`, no state change).
    pub fn transition(
        &mut self,
        from: StateId,
        trigger: Option<K>,
        guard: Option<Guard<Ctx>>,
        target: Option<StateId>,
        action: Option<Action<Ctx, K>>,
    ) {
        self.nodes[from.0].transitions.push(Transition {
            trigger,
            guard,
            target,
            action,
        });
    }

    /// Finalize the chart. Panics (debug) if structural invariants are violated.
    pub fn build(self) -> StateChart<Ctx, K> {
        let root = self
            .nodes
            .iter()
            .position(|n| n.parent.is_none())
            .expect("chart must have a root state");
        for n in &self.nodes {
            match n.kind {
                StateKind::Compound => debug_assert!(
                    n.initial.is_some() && !n.children.is_empty(),
                    "compound '{}' needs an initial child",
                    n.label
                ),
                StateKind::Parallel => {
                    debug_assert!(!n.children.is_empty(), "parallel '{}' needs regions", n.label)
                }
                StateKind::Leaf => {}
            }
        }
        StateChart {
            nodes: self.nodes,
            root: StateId(root),
        }
    }
}
