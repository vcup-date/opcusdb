# opcusdb, A Local Real-Time State Database for MMORPGs / State Machines

> Research survey + architecture plan. Status: **design only, no code yet.**
> Recommended implementation language: **Rust** (rationale in §5.1).

---

## 1. Survey: open-source, free "real-time databases"

"Real-time DB" is overloaded. There are three distinct families. Only the third
is what an MMORPG actually needs as a *source of truth*.

### A. BaaS / sync-engine real-time DBs (client-facing, web-shaped)
Push changes to subscribed clients over WebSocket/SSE. Persistence-first, not simulation.

| Project | Lang | Storage | Real-time mechanism | License |
|---|---|---|---|---|
| **Supabase Realtime** | Elixir + Postgres | Postgres (disk) | Logical replication → WebSocket | Apache-2.0 |
| **RethinkDB** | C++ | Disk (B-tree) | `changefeeds` (push query results) | Apache-2.0 |
| **PocketBase** | Go | SQLite | SSE subscriptions | MIT |
| **SurrealDB** | Rust | RocksDB / TiKV / mem | LIVE SELECT queries | BSL→Apache |
| **Appwrite** | PHP/Go | MariaDB | WebSocket | BSD-3 |
| **Hasura / ElectricSQL** | Haskell/Elixir | Postgres | GraphQL subs / local-first sync | Apache-2.0 |
| **Firebase RTDB / Firestore** |, |, | (proprietary, NOT open source) | closed |

**Verdict for MMO:** disk-anchored, per-row subscriptions, ~10²–10³ writes/s before
you fight the disk. Great for chat, leaderboards, auction house. **Wrong tool for the
60×/s movement of 5,000 entities.**

### B. In-memory data stores (cache / fast KV, used as a *side* store)
| Project | Lang | Model | Throughput (single box) | License |
|---|---|---|---|---|
| **Redis / Valkey** | C | Single-thread KV | ~0.1–1M ops/s, P99 sub-ms | BSD / Valkey fork |
| **KeyDB** | C++ | Multi-thread Redis | ~2–3× Redis writes | BSD |
| **Dragonfly** | C++ | Shared-nothing, multi-core | **~3.8M QPS**, P99 <1ms, linear to many cores | BSL |
| **Tarantool** | C + Lua | In-mem + WAL + app server | 100k–1M tx/s, stored procs | BSD |
| **Aerospike (CE)** | C | Hybrid mem/SSD | very high, predictable | AGPL |

**Verdict for MMO:** these are *excellent building blocks* (and Dragonfly's
shared-nothing-per-core design is the model to copy), but they are generic
KV/structures with no notion of *spatial interest*, *tick loop*, or *atomic
game-logic transactions*. You'd bolt your simulation on top.

### C. Game-native real-time DBs (database == game server), the right category
| Project | Lang | Idea | License |
|---|---|---|---|
| **SpacetimeDB** (ClockworkLabs) | **Rust** | In-memory relational DB + WASM "reducers" run *inside* the DB; clients connect directly; powers the **BitCraft** MMO | BSL (core), permissive client SDKs |
| **Tarantool** (again) | C+Lua | In-mem + stored procs + WAL, closest classic analogue | BSD |
| Custom engines (EVE/WoW-style) | C++/C# | Bespoke authoritative servers, in-mem world + RDBMS persistence | closed |

**SpacetimeDB is the reference design for this project.** Key ideas to borrow:
- In-memory state is the source of truth; WAL gives durability.
- "Reducers" = atomic transactional functions invoked over the network (≈ commands to a state machine).
- Clients subscribe to a *query*; the DB streams row deltas.
- ~150k tx/s reported vs ~1.5k for Node+Postgres on the same logic, two orders of magnitude, purely from collapsing DB↔app boundary and staying in RAM.

---

## 2. Features & functions that matter (and which family provides them)

| Capability | Why an MMO needs it | A (BaaS) | B (KV) | C (game DB) | opcusdb target |
|---|---|---|---|---|---|
| In-memory source of truth | 60Hz mutation w/o disk stall | ✗ | ✓ | ✓ | ✓ |
| Atomic transactions / "reducers" | loot, trade, combat must not half-apply | partial | weak (MULTI) | ✓ | ✓ (core) |
| Tick / simulation loop | physics, AI, regen, DoT | ✗ | ✗ | ✓ | ✓ (built-in scheduler) |
| Spatial / interest queries (AOI) | only send what you can see | ✗ | ✗ | ✓ | ✓ (grid + radius) |
| Subscriptions + delta push | thin clients, low bandwidth | ✓ | pub/sub only | ✓ | ✓ (delta-compressed) |
| Durability (WAL + snapshot) | crash → recover the world | ✓ (disk) | optional | ✓ | ✓ (async WAL) |
| Horizontal shard / zoning | one box can't hold a continent | ✓ | ✓ | ✓ (roadmap) | ✓ (sharded, phased) |
| Schema / typed rows | components have fixed layout | ✓ | ✗ | ✓ | ✓ (codegen) |

---

## 3. Concurrency / IO / system-resource evaluation

**The MMORPG load shape** (per zone server, target ~2k–5k entities, 1k players):
- Read-dominant within a tick: each entity reads neighbors (AOI) → O(N·k).
- Write burst at tick boundary: position/velocity/health updates → thousands of small writes per tick.
- Tick budget at 20 Hz = **50 ms**; at 30 Hz = **33 ms**. Everything (sim + serialize + network) must fit.
- Network fan-out is the real bottleneck: 1k players × ~50 visible entities × 20 Hz ≈ **1M entity-updates/s to serialize and send.**

**Where each family breaks:**
- **A (disk BaaS):** disk fsync + per-row WebSocket = dies at a few thousand writes/s. The DB *is* the bottleneck.
- **B (Redis single-thread):** one core caps you ~0.5–1M simple ops/s, but every AOI query is multiple round-trips and serialization happens in *your* app, not the store → you pay it twice. No tick.
- **B (Dragonfly/KeyDB multi-core):** raw throughput is there, but no spatial model; you reimplement interest management + sim in a separate process and round-trip constantly.
- **C (SpacetimeDB):** logic runs *inside* the DB, no round-trip, in RAM → this is why it hits 100k+ tx/s on game logic.

**Resource accounting (single 16-core box, the realistic target):**
- **CPU:** simulation is parallelizable per spatial shard. With shared-nothing-per-core (Dragonfly model), ~14 worker cores × per-core sim → headroom for 10k–50k entities depending on logic cost.
- **Memory:** entity ≈ a few hundred bytes across components. 100k entities ≈ tens of MB of hot state. RAM is *not* the constraint; cache locality is, hence data-oriented (SoA) layout.
- **IO:** WAL is sequential append → cheap; batch + group-fsync per tick (or every N ms). Snapshots are the only random-ish IO; do them copy-on-write / fork-style off the hot path.
- **Network:** dominant cost. Mitigate with AOI culling, delta compression, and bit-packing. This drives the protocol design (§5.6), not the storage engine.

**Conclusion:** the bottleneck for an MMO is *not* storage durability, it's
(1) keeping sim state in cache-friendly RAM, (2) avoiding cross-core locks, and
(3) network fan-out. A purpose-built engine wins because it co-locates logic +
state + interest management and never round-trips.

---

## 4. Feasibility: building an MMORPG on this, yes, with caveats

**Proven:** SpacetimeDB + BitCraft is a live MMO whose *entire* backend (chat,
items, terrain, player state) is one in-memory DB module. So the architecture is
validated in production.

**What it gives you "for free":**
- Authoritative server state (cheat resistance), clients send intents, DB validates in a reducer.
- Persistence without a separate ORM/RDBMS for live state.
- Real-time sync to clients via subscriptions.

**What you still must build yourself (the DB doesn't do these):**
- Game design, content, client rendering.
- Movement prediction / reconciliation / lag compensation (client+protocol).
- Cross-shard zoning hand-off when one box isn't enough.
- Anti-cheat beyond authoritative validation.

**Honest scope ladder:**
- 1 box, 1 zone, ~1–5k concurrent: very achievable, the focus of this plan.
- Many zones, 50k+ concurrent: needs sharding + zone hand-off (phase 4), hard but well-trodden.
- Seamless single-world (EVE-scale): research-grade; out of scope.

---

## 5. The plan, **opcusdb** in Rust

### 5.1 Language choice: Rust (with the trade-offs stated)

| Lang | For | Against | Verdict |
|---|---|---|---|
| **Rust** | No GC → **no tick-jitter from GC pauses**; C-class perf; memory safety eliminates the use-after-free/data-race bugs that plague C++ servers; great async + thread-per-core ecosystem; WASM host story (wasmtime) for sandboxed reducers; **this is exactly what SpacetimeDB chose**. | Steeper learning curve; longer initial dev. | **CHOSEN** |
| **C/C++** | Absolute peak control; EnTT is a superb ECS; mature. | Manual memory safety = the #1 source of MMO server crashes/exploits; slower, riskier dev. | Use only if team is C++-native. |
| **Go** | Fastest to write; great concurrency ergonomics. | **GC stop-the-world pauses are poison for a fixed tick budget**; less control over memory layout. Fine for the *gateway/chat/auth* tier, not the hot sim core. | Use for peripheral services. |

**Recommendation:** Rust for the core engine. Go for the edge gateway/auth/chat if desired.

### 5.2 High-level architecture

```
                 ┌─────────────────────────────────────────┐
   clients  ───► │  Gateway (QUIC/UDP+TCP)  conn, auth, AOI │
                 └───────────────┬─────────────────────────┘
                                 │ intents (reducer calls)
                 ┌───────────────▼─────────────────────────┐
                 │              opcusdb core                  │
                 │  ┌─────────────┐   ┌──────────────────┐  │
                 │  │ Shard 0     │   │ Shard N          │  │
                 │  │ (core-pinned)│ … │ (core-pinned)    │  │
                 │  │  - tables    │   │  - tables        │  │
                 │  │  - tick loop │   │  - tick loop     │  │
                 │  │  - reducers  │   │  - reducers      │  │
                 │  │  - AOI grid  │   │  - AOI grid      │  │
                 │  └─────┬───────┘   └────────┬─────────┘  │
                 │   message bus (cross-shard, lock-free)    │
                 └───────────────┬─────────────────────────┘
                                 │ committed tx
                 ┌───────────────▼─────────────────────────┐
                 │  WAL (seq append) + Snapshots (CoW fork)  │
                 └───────────────────────────────────────────┘
```

### 5.3 Data model, relational/ECS hybrid, data-oriented
- World state = **typed tables**. A table ≈ a component type (`Position`, `Health`, `Inventory`).
- **Entity = generational id** `(index: u32, generation: u32)` → no use-after-free, O(1) lookup.
- Storage is **SoA (struct-of-arrays) columns** per table → cache-friendly, vectorizable, the ECS performance win.
- Schema defined once in Rust; **codegen** produces typed client bindings (Rust/C#/TS) so clients are type-safe.
- Indexes: primary by entity id; secondary spatial index (§5.5); optional hash indexes for lookups (e.g., player name).

### 5.4 Concurrency model, shared-nothing per core (copy Dragonfly + SpacetimeDB)
- World is partitioned into **shards** (spatial regions). Each shard is **single-threaded**, pinned to a core, **owns its data** → **no locks in the hot path**.
- Reducers run to completion on their shard's thread → trivially atomic, no cross-thread tearing.
- Cross-shard interactions (entity at a boundary, trade across zones) go through a **lock-free message bus** as async messages, applied at the next tick, never a synchronous cross-core lock.
- Executor: thread-per-core (e.g. `tokio` current-thread per shard, or a custom runtime); IO (WAL/network) on separate threads so sim never blocks.

### 5.5 Tick loop + reducers (the "state machine" core)
- Each shard runs a fixed-rate loop (configurable, default **20 Hz**):
  1. Drain inbound intents/messages.
  2. Execute **reducers** (atomic: fully apply or roll back), both player-invoked (move, attack, trade) and system (regen, AI, DoT) via a **scheduler** (timers/cron inside the DB).
  3. Compute dirty set (changed rows).
  4. Append committed tx to WAL (batched, group-fsync).
  5. For each subscribed client, diff against its AOI → emit **delta**.
- Reducers v1: **native Rust** systems (trait objects / function registry), fastest, simplest.
- Reducers v2: **WASM modules** via `wasmtime` for sandboxed, hot-reloadable, untrusted game logic (the SpacetimeDB model). Adds isolation + live deploy at a small perf cost.

### 5.6 Interest management + subscriptions (the bandwidth saver)
- **Uniform spatial grid** (cell ≈ view radius) per shard; entities bucketed by cell. AOI query = read 9 neighboring cells → O(1)-ish.
- Each client has a **subscription** = query (often "entities within radius R of my avatar").
- Per tick, server sends **deltas only** (spawn / update changed fields / despawn) for the client's AOI, **bit-packed + delta-compressed** against last sent state.
- Snapshot-on-subscribe, deltas thereafter; periodic keyframe to bound error/resync.

### 5.7 Networking
- **Two channels:**
  - *Unreliable/UDP (or QUIC datagrams):* high-frequency state (positions), latest-wins, drop-tolerant.
  - *Reliable/ordered (QUIC stream or TCP/WebSocket):* reducer calls, trades, chat, important events.
- Recommend **QUIC** (e.g. `quinn`): gives both reliable streams and datagrams, encryption, connection migration, one transport.
- Wire format: compact binary (custom or `bitcode`/`rkyv`); schema-versioned.

### 5.8 Persistence & recovery
- **WAL**: append-only log of committed transactions; sequential write; tunable fsync (per-tick group commit vs. every-N-ms, durability/throughput knob).
- **Snapshot**: periodic full-state checkpoint via copy-on-write / `fork`-style so the hot path isn't stalled; truncate WAL after snapshot.
- **Recovery**: load latest snapshot → replay WAL tail. Deterministic reducers make replay exact.
- Cold/relational data (auction house history, audit logs) can be flushed to Postgres/SQLite asynchronously, keep it *off* the hot path.

### 5.9 Observability & ops
- Per-shard metrics: tick duration histogram (the #1 health signal, alert if p99 > tick budget), entities/shard, WAL lag, bytes/client.
- Deterministic replay from WAL = invaluable for debugging and load reproduction.
- Admin/console reducers for inspection.

---

## 6. Phased roadmap

| Phase | Deliverable | Proves |
|---|---|---|
| **0. Spike** | Single-thread in-mem table store + generational entity ids + one reducer + 20 Hz tick loop. No network. | Core data model + tick atomicity. |
| **1. Persistence** | WAL append + snapshot + crash-recovery (replay). | Durability without disk stalls. |
| **2. Network + subs** | QUIC gateway, client connect, subscribe to AOI, delta push. A dumb client that renders dots moving. | End-to-end real-time loop. |
| **3. Spatial + scale (1 box)** | Spatial grid AOI, native reducers for move/attack, load test to target (e.g. 2k entities @ 20 Hz, p99 tick < budget). | Single-zone MMO viability. |
| **4. Shards + WASM** | Shared-nothing multi-shard on one box, cross-shard message bus, optional WASM reducers + hot reload. | Multi-core scale + safe game logic. |
| **5. Zoning / multi-box** | Cross-process/box zone hand-off, distributed WAL. | Beyond one machine. |

**Success criteria for Phase 3 (the real milestone):** on a 16-core box,
sustain N concurrent players at 20 Hz with p99 tick latency under the tick budget,
recoverable from a kill -9 with no committed-transaction loss.

---

## 7. One-paragraph summary
Build **opcusdb** in **Rust** as an in-memory-first, authoritative state engine
where game logic ("reducers") runs *inside* the database against cache-friendly
SoA tables, driven by a fixed-rate tick loop, sharded shared-nothing-per-core
(no hot-path locks), with spatial interest management to cull network fan-out,
durability via sequential WAL + copy-on-write snapshots, and QUIC for combined
reliable+unreliable transport. This is the SpacetimeDB/Tarantool model
specialized for MMORPG state machines, it wins over BaaS (disk-bound) and raw
KV stores (no spatial/tick/transaction model) precisely by collapsing the
database↔game-server boundary and never round-tripping.
