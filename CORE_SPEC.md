# opcusdb — Core Spec (Phase 0–2 + `fsm-lab`)

> Buildable, engineering-grade spec for the **core**: ECS, the functional sync
> algebra, the Timeline, and the **statechart** model that `fsm-lab` demonstrates.
> Companion to `DESIGN.md` (the vision). **Still no code — this is the spec to
> build against.** Rust, single-process, no network yet (network is Phase 3).
>
> Design rule for this layer: **everything is deterministic and pure**, so that
> the same machinery later gives us rollback, replay, and lockstep for free.

---

## 1. Scope & non-goals

**In scope (what `fsm-lab` exercises):**
- ECS core: entities, component tables, resources, queries, systems, scheduler.
- The 5-primitive functional algebra (`reduce / merge / select / query / fold`), formalized with laws.
- Timeline: fixed-timestep loop, tick ring, snapshots, **rollback**, **replay**.
- Component **replication policies** as local behaviors (so the same code works networked later).
- A **statechart** engine (hierarchical + parallel states, guards, timers, actions) built *on* the algebra.
- `fsm-lab` demo: a 4-way traffic intersection + a small quest graph.

**Out of scope here (later phases):** transport/QUIC/WebRTC, CRDT gossip over a
wire, AOI/spatial, multi-engine bindings, persistence to SQL. The CRDT *merge*
functions are specified (they're pure and testable now) but not yet gossiped.

---

## 2. Determinism contract (the foundation)

Every rule below is mandatory for sim code (systems, reducers, guards, actions):

1. **No wall-clock, no ambient randomness.** Time comes only from `tick`. Randomness only from `World::rng` (a seeded, explicitly-advanced PRNG, e.g. PCG/wyrand; seed stored in the log).
2. **No unordered iteration in sim.** Use `BTreeMap`/sorted vectors; never iterate `HashMap` in systems. Entity iteration order is **ascending entity index**.
3. **Fixed execution order.** The scheduler may run systems in parallel *only* when their declared read/write sets prove independence; results must equal the canonical serial order (verified in CI).
4. **No floats in `fsm-lab`** (it's discrete). When real-valued sim arrives, use `fx32`/`fx64` fixed-point in deterministic worlds.
5. **All sim state lives in the `World`.** No `static mut`, no thread-locals, no hidden caches. Snapshot = serialize the `World`; that must capture *everything*.

A `#[sim]` proc-macro/lint enforces 1–4 at compile time where possible.

---

## 3. Data model & type system

A component/event/value is any type implementing `Reflect` (serialize, deserialize,
clone, stable byte layout). Two serialization targets:

- **wire/log/snapshot**: `bitcode` or `rkyv` (compact, fast, versioned). Used by the Timeline.
- **debug/inspect**: a self-describing `Value` enum (for tooling, replays viewer, agent JSON).

```rust
enum Value { Unit, Bool(bool), I64(i64), U64(u64), F64(f64),
             Str(Box<str>), Bytes(Box<[u8]>), List(Vec<Value>),
             Map(BTreeMap<Box<str>, Value>), Entity(EntityId) }
```

Schema is declared in Rust now; the `.realm` IDL + codegen is Phase 5. Each
component type registers a `ComponentId` (dense u32) and its replication policy
(§7) at world build time.

---

## 4. Entities

```rust
struct EntityId { index: u32, gen: u32 }   // 8 bytes, Copy, Ord by (index,gen)

struct Entities {
    generations: Vec<u32>,   // generations[index]; even = dead-ish, see below
    free: Vec<u32>,          // recycled indices (LIFO; deterministic)
    len: u32,
}
```

- `spawn()` → pop `free` (or push new) → bump generation → return id.
- `despawn(id)` → validate `gen`, increment `generations[index]`, push index to `free`.
- `is_alive(id)` → `generations[id.index] == id.gen`.
- Recycling is LIFO and fully determined by the call sequence → snapshot-safe.

`gen` overflow (2³² spawns on one slot) is a non-issue in practice; assert in debug.

---

## 5. Component storage — sparse set (with archetype escape hatch)

Default storage per component type is a **sparse set**: O(1) insert/remove/lookup,
dense contiguous values for cache-friendly iteration, stable across churn (ideal
for game entities and FSM instances that come and go).

```rust
struct SparseSet<T> {
    sparse: Vec<u32>,        // entity.index -> dense slot (or NONE)
    dense:  Vec<u32>,        // dense slot   -> entity.index   (for back-map)
    gens:   Vec<u32>,        // dense slot   -> entity.gen     (validate stale ids)
    data:   Vec<T>,          // dense slot   -> component value (SoA-friendly)
}
```

- `insert/get/get_mut/remove` are O(1); `remove` swaps last into the hole (swap-remove) and patches `sparse`.
- Iteration walks `dense`/`data` in dense order; **for deterministic sim, queries
  iterate entities in ascending `entity.index`** (sort the dense view, or keep a
  secondary ordered index — see §8). Sparse sets are unordered by construction, so
  the ordered view is materialized by the query layer, not the storage.
- **Archetype escape hatch**: components flagged `#[storage(archetype)]` are packed
  by archetype for iteration-heavy systems. `fsm-lab` uses sparse sets only.

`World` holds: `Entities`, a `ComponentId -> ErasedStore` map, and a typed
**resource** map (singletons like `Clock`, `Rng`, config).

---

## 6. Events & the log

Events are the *only* way state changes outside a system's own writes. An event is
typed data plus metadata:

```rust
struct Event<E> { tick: Tick, seq: u32, source: Source, payload: E }
enum Source { System, Player(PeerId), Agent(AgentId), Timer(TimerId) }
```

- Per tick, inbound events are collected, **sorted by `(seq, source)`** for a total order, then folded.
- The **log** is the append-only sequence of `(tick, [events])`. It is the
  ground truth from which any state is reproducible: `state = fold(snapshot, log_tail)`.
- For `fsm-lab` the log is in-memory + optional file dump; Phase 1 adds the on-disk WAL.

---

## 7. Functional algebra (formalized, with laws)

Five primitives. Tests assert the laws — this is how we avoid "looks right" slop.

| Primitive | Signature | Laws (CI-checked) |
|---|---|---|
| `reduce` | `(S, &E) -> S` | **pure & total**; `reduce` over the same events from the same `S` yields identical `S` (determinism) |
| `merge` | `(T, T) -> T` | **commutative, associative, idempotent** (join-semilattice) → CRDT safety |
| `select` | `&World -> T` | **pure**; depends only on declared inputs; memoized, invalidated when inputs' versions change |
| `query` | `&World -> OrderedSet<EntityId>` | **pure**; stable order (ascending index); a *subscription* is a query whose result-delta is streamed |
| `fold` | `(S0, &[E]) -> S` = `events.iter().fold(S0, reduce)` | equals replaying the log; basis of Timeline & recovery |

**Component replication policies** are *interpretations* of these primitives, and
behave correctly even single-process (so nothing changes when we go networked):

| Policy | Local behavior now | Networked behavior later |
|---|---|---|
| `Authoritative<T>` | plain mutable component; only systems/reducers write it | server writes; clients predict + reconcile |
| `Deterministic<T>` | written only by the lockstep sim; excluded from snapshots-as-delta (recomputed) | never sent; recomputed from inputs |
| `Crdt<L: Lattice>` | stored as a lattice value; writes go through `merge` | delta-gossiped, conflict-free |
| `Local<T>` | normal component; flagged non-replicable | never leaves the machine |
| `Derived<T>` | a registered `select`; never stored, computed on read | never sent; recomputed per peer |

**CRDT lattice catalog** (pure, unit-tested now, gossiped later): `LwwReg<T>`
(value + Lamport ts + tiebreak peer id), `GCounter`/`PNCounter`, `OrSet<T>`
(add-wins, unique tags), `Rga<T>` (ordered sequence for text/chat). Each is a
`Lattice` with a `merge` that satisfies the three laws.

---

## 8. Queries & systems

**Query** = component-set filter + ordering. API sketch:

```rust
// read Position & Health, exclude Dead, iterate ascending entity index
for (e, (pos, hp)) in world.query::<(&Position, &Health)>().without::<Dead>() { ... }
```

- The query layer materializes the **ordered** entity view (ascending index) for determinism.
- A **reactive query** caches its last result-set + per-component version stamps;
  on change it emits a **delta** (`Added/Changed/Removed` entities + changed
  fields). This is the same delta type the network/binding layers consume later.

**System** = a pure function with *declared* access, plus a **command buffer** for
structural changes (spawn/despawn/insert/remove) applied at a deterministic
barrier after the system runs (so iteration isn't invalidated mid-loop).

```rust
#[sim]
fn ai_tick(q: Query<(&Position, &mut Velocity)>, clock: Res<Clock>, cmd: &mut Commands) { ... }
```

**Scheduler:**
1. From each system's declared (reads, writes) build a conflict graph.
2. Topologically order; group into stages of mutually-independent systems.
3. Execute stages; within a stage, systems may run on a thread pool (rayon) **iff**
   the serial-equivalence test passes. Otherwise serial.
4. CI runs every world in both `serial` and `parallel` modes and asserts byte-identical snapshots.

`fsm-lab` runs serial (it's tiny); the parallel path is validated but not required.

---

## 9. Timeline (tick loop, ring, snapshot, rollback, replay)

**Fixed timestep** with an accumulator; sim never sees variable dt:

```
accumulator += real_dt
while accumulator >= TICK_DT { step_one_tick(); accumulator -= TICK_DT }
render(interpolation = accumulator / TICK_DT)   // clients only
```

**Per-tick step:**
1. Collect & order inbound events for this tick (§6).
2. Fire due **timers** (§10) as events.
3. `fold` events via reducers; run systems via scheduler; apply command buffers.
4. Bump component versions for reactive queries; emit deltas.
5. Append `(tick, events)` to the log; periodically write a **keyframe snapshot**.

**Ring buffer**: keep the last `N` ticks of `(events, optional snapshot)`. Snapshot
cadence `K` (e.g. every 8 ticks). `N` covers the max rollback window.

**Snapshot** = full serialize of `World` (entities + all stores + resources incl.
`Rng` state). Use copy-on-write / double-buffer so snapshotting doesn't stall the
step. Snapshots are the *only* thing needed besides the log to reconstruct state.

**Rollback(to_tick):**
```
restore(nearest keyframe snapshot S where S.tick <= to_tick)
for t in (S.tick+1 ..= to_tick): step_one_tick(replay events[t])   // deterministic
```
Used later for prediction/lag-comp/lockstep; in `fsm-lab` it powers the **time
scrubber** in the demo UI (rewind the intersection and watch it re-simulate).

**Replay** = `rollback` from tick 0 (or any keyframe) using the recorded log;
byte-identical to the original by the determinism contract. Replays are the
test oracle: record a session, replay it, assert equal final snapshot.

---

## 10. Timers & scheduling inside the sim

Deterministic timers are essential for state machines (timeouts, debounce, delays):

```rust
struct Timer { fire_at: Tick, repeat: Option<u32>, payload: TimerEvent, id: TimerId }
```

- Timers live in a `BinaryHeap` keyed by `(fire_at, TimerId)` in a resource.
- At step start, pop all `fire_at <= now`, emit as `Source::Timer` events (ordered by `TimerId`).
- `after(ticks, ev)`, `every(ticks, ev)`, `cancel(id)`. All deterministic.

---

## 11. Statechart engine (the `fsm-lab` core), built on the algebra

Not a naive flat FSM — **hierarchical + parallel statecharts** (Harel/SCXML-class),
because the user explicitly wants complex systems, not toys.

**Model:**
- A machine *definition* is static data: a tree of states with optional **parallel
  regions**, each state having `on_entry` / `on_exit` actions and `transitions`.
- A machine *instance* is an entity with a `Machine` component holding the active
  **state configuration** (the set of currently-active leaf states across regions)
  and local context data.

```rust
struct State { id: StateId, parent: Option<StateId>, kind: Compound|Parallel|Leaf,
               on_entry: ActionId, on_exit: ActionId, initial: Option<StateId> }
struct Transition { from: StateId, event: EventKind, guard: GuardId,
                    target: StateId, actions: Vec<ActionId> }
struct Machine { def: MachineDefId, config: SmallVec<StateId>, ctx: Value }
```

**Semantics (one event → one microstep, run-to-completion):**
1. Select enabled transitions: those whose `from` ∈ active config, whose `event`
   matches, and whose **guard** (a pure `select`-style predicate over `(ctx, world)`) is true.
2. Resolve conflicts deterministically: child states win over ancestors; ties
   broken by document order (StateId).
3. Compute exit set / entry set (LCA of source & target), run `on_exit` (deepest
   first), transition `actions`, `on_entry` (shallowest first).
4. Actions are **reducers** over `(ctx, world)` — they may emit events, set timers,
   spawn/despawn entities (via command buffer). Pure & deterministic.
5. Loop until no eventless ("automatic") transitions remain (run-to-completion).

**Why this maps cleanly:** a transition *is* `reduce`; a guard *is* `select`; a
delayed transition *is* a `Timer`; the machine's history *is* the `fold` of its
event log — so rollback/replay of a statechart is free. State machines, MMO
gameplay, and chat moderation all reuse the same engine.

---

## 12. `fsm-lab` demo spec

A headless core + a tiny web inspector (later) — but the *logic* is pure Rust,
runnable as a CLI now.

**Scene A — 4-way traffic intersection** (showcases parallel regions + timers):
- One `Machine` per traffic light × 4, plus a `Controller` parallel machine
  coordinating N/S vs E/W phases with `green → yellow → red` timed transitions
  and an all-red safety interlock (a guard that forbids two greens on crossing axes).
- Cars are entities with a `Car` FSM (`approaching → waiting → crossing → gone`),
  spawned by a deterministic Poisson-ish process driven by `World::rng`.
- Metrics derived via `select`: average wait time, throughput, max queue length.

**Scene B — quest graph** (showcases hierarchy + guards + context):
- A `Quest` statechart: `not_started → active{collecting | escorting} → (completed | failed)`
  with guards over inventory/context and timers for fail-on-timeout.

**Acceptance tests (the anti-slop gate):**
1. **Replay determinism**: run 10k ticks, dump log, replay → byte-identical final snapshot.
2. **Rollback equivalence**: snapshot at tick T, run to T+200, rollback to T, re-run → identical to the original T+200.
3. **Serial == parallel** scheduler snapshots match.
4. **Statechart conformance**: a suite of SCXML-style scenarios (entry/exit order, parallel regions, guard conflicts, run-to-completion) with expected traces.
5. **Safety invariant**: the intersection never has crossing greens (property test over random seeds).
6. **CRDT laws**: property tests for every lattice in §7 (commutative/associative/idempotent + convergence on random op interleavings).

CLI: `opcusdb run fsm-lab --scene intersection --ticks 10000 --seed 42 --record out.log`
then `opcusdb replay out.log --assert-eq` and `opcusdb scrub out.log --to 3000`.

---

## 13. Crate / module layout (Rust workspace)

```
opcusdb/
  crates/
    opcusdb-core/        # entities, storage, world, query, scheduler, commands
    opcusdb-algebra/     # reduce/merge/select/query/fold + Lattice + CRDT catalog
    opcusdb-time/        # Timeline: tick loop, ring, snapshot, rollback, replay, timers
    opcusdb-fsm/         # statechart engine on top of core+algebra
    opcusdb-serde/       # Reflect, bitcode/rkyv, Value, schema registry
    opcusdb-macros/      # #[sim], #[component], derive(Reflect/Lattice)
  demos/
    fsm-lab/            # CLI + scenes (intersection, quest) + acceptance tests
  docs/                 # concepts + guides (mdBook)
  xtask/                # determinism CI: serial-vs-parallel, replay byte-diff
```

Dependencies pinned to mature, cross-platform crates only (rayon, smallvec, a PCG
rng, bitcode/rkyv). No transport deps in Phase 0–2.

---

## 14. Open questions to settle before coding `fsm-lab`

1. **Serializer**: `rkyv` (zero-copy, fastest snapshots, stricter types) vs
   `bitcode` (simpler, smaller code). Leaning `rkyv` for snapshot speed; revisit if ergonomics bite.
2. **Statechart definition source**: hand-built Rust builder API first; do we also
   want an **SCXML** importer early (great for tooling/interop) or defer to Phase 5?
3. **Memoization granularity** for `select`: per-component version stamps vs.
   per-entity dirty bits — affects reactive-query cost. Start coarse (component
   version), optimize later.
4. **Snapshot strategy**: full-copy double-buffer (simple) vs. true CoW pages
   (cheaper for big worlds). `fsm-lab` is small → start full-copy, design the
   trait so CoW slots in without API change.
5. **Time scrubber UI**: ship as a tiny web inspector now, or stay CLI-only for
   the first cut? (Leaning CLI-only; the web inspector is a nice early `dots-mesh`-adjacent task.)

These five are the only decisions blocking a clean Phase-0 start.
```
