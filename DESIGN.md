# opcusdb — a time-aware, policy-driven replicated ECS

> Foundational framework for real-time multiplayer + AI-agent worlds.
> **Status: design only. No code yet.** Supersedes the earlier PLAN.md sketch.
> This file is the **vision/architecture**. The buildable, engineering-grade
> spec for the first milestone (the core: ECS + Timeline + algebra + the state
> machine model behind `fsm-lab`) lives in **`CORE_SPEC.md`**.
> This design is original — it is *not* modeled on SpacetimeDB. Its independent
> foundations are: event sourcing, CRDTs (lattice merge), rollback netcode
> (GGPO-style), RTS lockstep, and data-oriented ECS.

---

## 0. The problem with "pick one netcode"

WoW, Overwatch, LoL, a state machine, and a human+AI chatroom each demand a
*different* consistency/authority/latency tradeoff:

| Target | Authority | Tick | Sync style | Topology |
|---|---|---|---|---|
| **WoW** (persistent MMO) | server | ~10–20 Hz, soft | authoritative + AOI delta | sharded servers |
| **Overwatch** (room shooter) | server | 60 Hz, hard | client-predict + server reconcile + lag-comp | dedicated/listen |
| **LoL** (MOBA) | none / shared | deterministic | **send inputs only**, identical sim everywhere | lockstep / relay |
| **State machine** | varies | event-driven | reducers + guards + timers | any |
| **Human+AI chat** | eventual | n/a | append log, offline-merge | P2P mesh OK |

A normal engine hard-codes **one** of these. **opcusdb's thesis: the
consistency/authority/topology is a *policy* you attach per-component and
per-session — over one common core.** That single decision is what lets one
framework serve all five without being "AI slop" glue.

---

## 1. Three load-bearing ideas (what's actually different)

### Idea 1 — Consistency is a per-component policy, not a global mode
Every component type declares **how it replicates and how conflicts resolve**:

```rust
component Position   : Authoritative<Vec3>;      // server owns; clients predict
component Health     : Authoritative<i32>;
component Velocity    : Deterministic<Vec3>;      // only the lockstep sim writes it; never sent
component ChatLog     : Crdt<Rga<Message>>;       // conflict-free append, mergeable offline / P2P
component Cosmetic    : Crdt<Lww<SkinId>>;        // last-write-wins register
component Cursor      : Local<Vec2>;              // never leaves this machine
component ThreatLevel : Derived<f32> = |w| compute_threat(w);  // pure, never stored/synced
```

In one world, `Position` is server-authoritative (Overwatch/WoW), `ChatLog` is a
CRDT that merges across a P2P mesh, and `ThreatLevel` is a pure derived view.
**No other framework lets you mix these in the same entity.** This is the
"functional features for syncing" you asked for — see §4.

### Idea 2 — Time is a first-class axis (the Timeline)
State is a *fold over an ordered event log*: `state(t) = fold(reduce, snapshot, events ≤ t)`.
The engine keeps a bounded ring of recent ticks. Because systems are **pure**
(§3), it can cheaply: rewind, re-simulate, reconcile predictions, do lag
compensation ("rewind the world to when the shot was fired"), and produce
replays (a replay is just the input log + seed). One mechanism — the Timeline —
gives you rollback (Overwatch/LoL), interpolation, lag-comp, and deterministic
replay. See §5.

### Idea 3 — One core, everywhere (native + WASM)
The simulation core is **one Rust crate**, compiled to:
- native (mac/win/linux) → dedicated servers and native clients,
- **WASM** → runs *inside the browser* for PixiJS/Three.js clients **and** as a
  browser P2P peer that can be authoritative.

Same bytes simulate on server, native client, and browser. That is what makes
**deterministic lockstep and rollback actually safe** (no "the server's float
math differs from the client's") and what makes **serverless/P2P real** (a
browser tab can host). This is a deliberate architectural commitment, not an
afterthought.

---

## 2. Layered architecture

```
┌──────────────────────────────────────────────────────────────────┐
│  Client engine bindings:  PixiJS · Three.js (TS/WASM) · Unity ·    │
│                           Godot (C#/GDScript) · native Rust        │  ← §8
├──────────────────────────────────────────────────────────────────┤
│  SDK: reactive queries (subscribe) · intents (act) · agent runtime │  ← §4,§9
├──────────────────────────────────────────────────────────────────┤
│  Topology policy: Dedicated · Listen+host-migrate · Mesh(P2P) ·    │
│                   Lockstep · MeshOfMeshes (federation)             │  ← §6
├──────────────────────────────────────────────────────────────────┤
│  Sync engine: per-component replication policies · delta/CRDT      │  ← §4
│               merge · AOI/interest · prediction & reconciliation   │
├──────────────────────────────────────────────────────────────────┤
│  Timeline: tick ring · rollback · lag-comp · replay · snapshots    │  ← §5
├──────────────────────────────────────────────────────────────────┤
│  ECS core: generational entities · SoA component tables · pure     │  ← §3
│            systems · deterministic scheduler · spatial index       │
├──────────────────────────────────────────────────────────────────┤
│  Storage: event log (WAL) · CoW snapshots · optional SQL sidecar   │  ← §10
├──────────────────────────────────────────────────────────────────┤
│  Transport: QUIC (native) · WebTransport (browser↔server) ·        │  ← §6
│             WebRTC DataChannels (browser P2P) · ICE/STUN/TURN      │
└──────────────────────────────────────────────────────────────────┘
```

**The entire stack from "Topology" down is one Rust core.** The top two layers
are thin, generated, per-language adapters.

---

## 3. ECS core (data-oriented + deterministic)

- **Entities**: generational ids `(index:u32, gen:u32)` → O(1), no use-after-free.
- **Storage**: **sparse-set per component** (fast add/remove, stable ids — good for
  churny game entities) with optional **archetype** packing for heavy
  iteration-bound systems. SoA columns → cache-friendly, vectorizable.
- **Systems**: declared as **pure functions** `fn(view) -> changeset` with explicit
  read/write component sets. The scheduler:
  - derives a dependency DAG from declared access → **auto-parallelizes** non-conflicting systems across cores;
  - in **deterministic mode** falls back to a fixed, stable execution order + sorted entity iteration + a **seeded PRNG drawn from the log** → identical results on every peer (mandatory for LoL/lockstep).
- **Determinism toolkit**: optional **fixed-point math** (`fx32`/`fx64`) for
  cross-platform identical arithmetic; deterministic collections (no hash-map
  iteration in sim); float discipline lint. Determinism is *opt-in per world* —
  WoW doesn't need it, LoL does.
- **Spatial index**: uniform grid + loose-quadtree hybrid, rebuilt/maintained
  per tick; powers AOI queries (§4) and broad-phase.

---

## 4. The functional sync algebra (your "functional features")

Five pure primitives. Everything in the SDK is built from these.

| Primitive | Signature | Used for |
|---|---|---|
| **reduce** | `(State, Event) -> State` | authoritative state transitions, state machines |
| **merge** | `(T, T) -> T` (a join-semilattice: commutative, associative, idempotent) | CRDT components → conflict-free P2P/offline |
| **select** (lens) | `World -> T`, memoized, auto-invalidated | derived/computed components, no manual sync |
| **query** | `World -> Set<Entity>` pure predicate (incl. spatial AOI) | **reactive subscriptions** = a query that streams its delta |
| **fold** | `(snapshot, [Event]) -> State` | the Timeline; replay; recovery |

**CRDT library** (for `Crdt<…>` components and P2P): LWW-register, G/PN-counter,
OR-Set, **RGA/Logoot** for ordered text & chat, and a delta-state encoding so the
mesh gossips *small deltas*, not whole objects. Because merges are lattice joins,
**any partition heals automatically** — the basis for serverless mode.

**Why this matters:** a *subscription is just a query*, *a state machine is just a
reduce*, *a derived stat is just a select*, and *P2P conflict resolution is just a
merge*. The same algebra expresses an MMO interest set, a MOBA input fold, and a
chatroom's mergeable history. That is the "complex projects, not glue" property.

```rust
// reactive AOI subscription — one line, pure, engine-agnostic
let nearby = world.subscribe(query!(Position within 40.0 of me) & has::<Renderable>());
nearby.on_delta(|d| binding.apply(d));   // Pixi/Three/Unity/Godot adapter consumes deltas
```

---

## 5. The Timeline engine (rollback / prediction / lag-comp / replay)

A bounded ring buffer of the last `N` ticks: each tick stores its inputs + a
periodic keyframe snapshot. Operations, all enabled by pure systems:

- **Client-side prediction**: client runs the real sim locally on its own inputs immediately.
- **Server reconciliation**: when the authoritative tick arrives, rewind to it, replay buffered local inputs, smooth-correct the visual delta (Overwatch).
- **Rollback (P2P/lockstep)**: a late remote input for tick `t` → rewind to `t`, re-sim forward. Cheap because sim is deterministic & pure (LoL, fighting-game-grade).
- **Lag compensation**: server rewinds the world to the shooter's render-time to validate a hit ("favor the shooter").
- **Replay & spectate**: persist the input log + seed; re-fold to reproduce any match exactly. Also the basis of deterministic debugging and load-test capture.

Tunables per world: tick rate, input delay vs. rollback window, snapshot cadence,
interpolation buffer depth.

---

## 6. Topology & transport (incl. serverless P2P + mesh-of-meshes)

The sim core is topology-blind. A **Topology policy** decides membership,
authority, and event flow:

| Policy | Authority | Best for | Notes |
|---|---|---|---|
| **Dedicated** | one server | WoW zones, ranked Overwatch | classic; durable |
| **Listen + host-migrate** | one peer (a player) | casual rooms | host leaves → CRDT/state handoff to a new host |
| **Lockstep** | shared (deterministic) | LoL/RTS | broadcast inputs only; tiny bandwidth; needs determinism (§3) |
| **Mesh (P2P)** | none (CRDT) | chatrooms, co-op sandboxes, agent swarms | gossip delta-CRDTs; no server needed |
| **MeshOfMeshes** | federated | many rooms / a shared "overworld" of pods | super-peers/relays bridge local meshes; DHT or light tracker for discovery |

**Transport matrix:**
- Native ↔ native / server: **QUIC** (`quinn`) — reliable streams (transactions, chat) + unreliable datagrams (positions), TLS, conn migration.
- Browser ↔ server: **WebTransport** (falls back to WebSocket).
- Browser ↔ browser (P2P): **WebRTC DataChannels** (ordered+unreliable modes), NAT traversal via **ICE/STUN**, **TURN** relay fallback. (Browsers can't do raw UDP — this is why the mesh layer abstracts transport.)

**Mesh-of-meshes** = a federation: each room/zone is a local mesh; **super-peers**
(or a cheap relay) bridge rooms and carry only cross-room CRDT deltas; a DHT or
minimal tracker handles peer discovery. This is how "serverless" scales past one
room without a central authority — and it degrades gracefully to relay-assisted
when NATs are hostile.

**Honest CAP note:** you cannot simultaneously have (a) zero servers, (b) strong
authoritative anti-cheat, and (c) lowest latency. So: P2P-mesh ⇒ CRDT/eventual
(great for chat, co-op, agents); competitive integrity ⇒ at least a host or
dedicated authority. opcusdb makes the tradeoff *explicit and switchable*, not
hidden.

---

## 7. How each target maps onto the same core (concrete configs)

**WoW-like MMO** — `Topology::Dedicated`, sharded by zone, tick 15 Hz, all
gameplay components `Authoritative`, AOI grid subscriptions, WAL+snapshot
persistence, SQL sidecar for accounts/market history. Cross-zone travel = entity
handoff over the mesh-of-meshes bus.

**Overwatch-like room** — `Topology::Dedicated`/`Listen`, tick 60 Hz hard,
`Authoritative` components + Timeline client-prediction + lag-comp; rooms are
ephemeral worlds spun up per match; no long-term persistence (just match replay).

**LoL-like MOBA** — `Topology::Lockstep`, deterministic world (fixed-point,
seeded RNG, stable scheduling), send **inputs only**, Timeline rollback for late
inputs; replay = input log. Optional relay for ordering/anti-cheat.

**State machine** — define states as entities, transitions as `reduce`, timers
via the deterministic scheduler, guards as pure predicates. Topology irrelevant;
works embedded (no network) or replicated.

**Human + AI chatroom** — `Topology::Mesh`, `ChatLog : Crdt<Rga<Message>>` merges
offline/partition, presence as `Crdt<Lww>`, AI agents are first-class peers (§9).
Optional relay/super-peer for history & discovery. No server required.

---

## 8. Client-engine bindings (Pixi / Three / Unity / Godot)

**opcusdb owns the model; the engine owns rendering.** Engines don't impose their
ECS — opcusdb streams component deltas and a thin per-engine adapter maps them to
engine objects.

- **Schema IDL** (`*.realm`): declare components, their replication policy, and
  events once. `opcusdb gen` produces typed bindings:
  - **TypeScript** (PixiJS, Three.js) + the WASM core for in-browser sim/P2P,
  - **C#** (Unity, Godot-mono),
  - **GDScript** (Godot),
  - **Rust** (native).
- **Reactive view layer**: `subscribe(query)` → delta stream; adapters:
  - Pixi: delta → create/update/destroy `Sprite`s in a container.
  - Three: delta → `Object3D`/instanced meshes; transforms from `Position/Rotation`.
  - Unity/Godot: a `ReplicatedEntity` component/node binds GameObject/Node lifetimes to opcusdb entities; interpolation buffer built in.
- Prediction/interpolation helpers shipped per engine so movement looks smooth
  regardless of tick rate.

---

## 9. AI agents as first-class clients (the "AI agent age" angle)

An AI agent is just a **headless client** over the same SDK:
- **Perceive**: subscribe to a `query` (its area/topic of interest) → a structured,
  diffable state stream (token-frugal: send deltas, not the whole world).
- **Act**: emit intents/events through the same reducer/transport path as humans —
  so agents and players are indistinguishable to the world.
- **Agent runtime**: optional helper that maps the perception stream → a tool/JSON
  schema and the model's tool-calls → intents, with rate/permission policies.
- Because chat/world state can be **CRDT**, swarms of agents can run **serverless on
  a mesh**, merging their effects conflict-free. This is what makes opcusdb a
  substrate for agent-populated worlds, not just games.

---

## 10. Do you need another (relational) database?

**For live, real-time state: no** — opcusdb's in-memory tables + event-log + CoW
snapshots are the source of truth and the fast path.

**For cold / business / analytical data: yes, keep a boring SQL DB** (SQLite
embedded, or Postgres at scale) as an **async sidecar** for: accounts & billing,
auth, audit/compliance logs, global market history, full-text search, analytics,
GDPR deletion. Push these off the hot path via the event log (CDC-style). Forcing
that data into the realtime engine would be the wrong tool. So: opcusdb does *all
the realtime work*; SQL handles *durable, queryable, cold* concerns. This honesty
is the difference between a real system and slop.

---

## 11. Cross-platform

- Core: Rust → mac/win/linux native + `wasm32` (browser) + can target mobile later.
- Transport: QUIC native; WebTransport/WebRTC in browser — covered by the topology layer so app code is identical.
- CI builds/tests all three desktop OSes + a headless WASM run; determinism tests
  diff sim output across platforms byte-for-byte (the determinism gate for LoL mode).

---

## 12. Demos (shipped to prove it's not glue)

1. **`dots-mesh`** — browser (Pixi) P2P mesh, no server: thousands of CRDT dots, two tabs merge after going offline. Proves mesh + CRDT + WASM core.
2. **`arena-60`** — Overwatch-style room: dedicated server, 60 Hz, client prediction + lag-comp hit validation, Three.js client. Proves Timeline.
3. **`lockstep-moba`** — 2–5 player deterministic MOBA-lite, inputs-only, rollback, byte-identical replay across mac/win/linux/browser. Proves determinism.
4. **`zone-world`** — WoW-style: 2 zones, server-authoritative, AOI, WAL persistence + crash-recovery, cross-zone handoff. Proves persistence + AOI + federation.
5. **`fsm-lab`** — pure embedded state-machine playground (traffic system / quest graph), no network. Proves the core algebra standalone.
6. **`agora`** — human+AI chatroom on a mesh: AI agents as peers perceiving/acting via the SDK, CRDT history, offline-merge. Proves the agent substrate.
7. Engine parity: the same `zone-world` model rendered by **Pixi, Three, Unity, and Godot** to prove the binding layer.

---

## 13. Documentation plan

- **Concepts**: the policy model, the Timeline, the sync algebra (with diagrams).
- **Guide per target**: "build an MMO", "build a 60Hz room", "build a lockstep
  game", "build a serverless chatroom", "embed a state machine", "add AI agents".
- **Reference**: IDL spec, component policies, CRDT catalog, topology policies, SDK API per language.
- **Cookbooks**: prediction tuning, lag-comp, host migration, NAT/TURN setup, SQL sidecar/CDC, determinism debugging.
- **Runnable**: every demo in §12 is `cargo run` / `npm run` with a README; docs link to the exact lines.
- **Determinism & netcode test reports** published from CI.

---

## 14. Roadmap

| Phase | Deliverable | Proves | Demo |
|---|---|---|---|
| **0 Core** | ECS (sparse-set+SoA, generational ids), pure systems, deterministic scheduler, spatial index | data model + determinism | `fsm-lab` |
| **1 Timeline** | tick ring, rollback, replay, snapshots, WAL | time-as-axis | replay of `fsm-lab` |
| **2 Sync algebra** | per-component policies, CRDT lib, reactive queries/AOI | functional sync | — |
| **3 Transport** | QUIC + WebTransport + WebRTC, topology policies | server & P2P | `arena-60`, `dots-mesh` |
| **4 Determinism gate** | fixed-point, cross-platform byte-diff CI | lockstep safety | `lockstep-moba` |
| **5 Bindings** | IDL + codegen for TS/C#/GDScript/Rust; engine adapters | multi-engine | engine-parity demo |
| **6 Scale** | sharding, mesh-of-meshes federation, SQL sidecar, host migration | complex systems | `zone-world` |
| **7 Agents** | agent runtime + perception/action mapping | AI-age substrate | `agora` |

**Language: Rust** — no GC jitter (kills hard tick budgets), one core compiling to
native + WASM (the determinism & P2P thesis), memory safety (no UAF exploits),
mature QUIC/WASM/CRDT ecosystem. Go is fine for *peripheral* services (matchmaker,
web dashboard) but its GC disqualifies it from the hot deterministic core; C++
gives equal speed but reintroduces the memory-safety class of MMO bugs.

---

## 15. Honest limits
- Deterministic lockstep requires arithmetic & scheduling discipline; it is the
  hardest mode to keep correct across platforms — hence the CI byte-diff gate.
- Pure-P2P cannot give competitive-grade anti-cheat; that needs an authority.
- Mesh discovery/NAT traversal needs STUN always and **TURN sometimes** (not 100%
  serverless in hostile networks — TURN is a thin relay, not an authority).
- Seamless single-shard EVE-scale worlds remain research-grade; federation
  (mesh-of-meshes) is the pragmatic answer, not a single global authority.
```
