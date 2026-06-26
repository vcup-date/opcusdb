# How opcusdb compares to similar frameworks

This document situates opcusdb against the projects that occupy nearby ground: realtime
sync backends, CRDT libraries, multiplayer game servers, embedded databases, and AI agent
simulations. It is meant to be honest about what opcusdb is, what it is not, and where it
sits in a crowded landscape.

The short version: no single project listed here occupies opcusdb's exact position. Each
competitor is specialized to one layer (sync, or merge, or netcode, or storage, or agent
simulation), and every one of them is built on a substantial dependency tree (Node and
npm, Postgres, Redis, the Elixir runtime, a WASM host, or, for the Rust ones, dozens of
crates). opcusdb's defining bet is to span all of those layers in one coherent, zero
dependency Rust codebase, trading production scale and ecosystem for auditability,
portability, and conceptual unity.

## What opcusdb actually is

opcusdb is a deterministic, time-aware Entity-Component-System engine in Rust. Its thesis
is that a simulated world is a pure deterministic function of its inputs, and that
consistency, authority, and network topology are a policy layered over a common core
rather than a hard-coded choice. Determinism is the spine: it is what makes replay,
rollback, lockstep, and crash recovery fall out of one design.

Concretely:

- **Core (`opcusdb-core`)**: an ECS with a `World` (deep-cloneable, which is the snapshot
  mechanism for rollback), generational entity ids, `SparseSet` component storage with a
  per-store version counter for change detection, conjunctive queries returning sorted ids,
  a uniform `SpatialGrid` for area-of-interest, a `Select` reactive memo (recompute only
  when a dependency's version changes), a conflict-graph `Schedule`, fixed-point `Fx`
  (Q16.16) arithmetic, and a seeded PCG32 `Rng`. Everything is determinism-first: no
  floats in the simulation, no wall clock, no ambient randomness.
- **CRDTs (`opcusdb-algebra`)**: hand-written `LwwReg`, `GCounter`, `PNCounter`, an
  add-wins observed-remove `OrSet`, and an `Rga` (replicated growable array) for sequences,
  all behind a `Lattice` trait with law-checkers.
- **Time (`opcusdb-time`)**: a `Timeline` where `state(t) = step^t(initial, log)`. The
  input log is the source of truth; bounded keyframes only accelerate seeks. Durability is
  "replay the log," so there is no serializer or on-disk database format in the core.
- **Realtime servers (`demos/server`)**: eleven separate authoritative server binaries on
  ports 9001 to 9011. The `World` lives on a single simulation thread and is never shared;
  clients send inputs over a channel and receive a broadcast snapshot. The WebSocket is
  hand-rolled (`ws.rs`: from-scratch SHA-1, base64, RFC 6455 framing, ping and close, a 1MB
  frame cap, no fragmentation, no TLS). Outbound HTTPS (for LLM calls) shells out to the
  system `curl`, so TLS is never linked into the binary.
- **Demos**: a shared-world cursor and boids sim, IRC-style chat with AI personas, a snake
  arena, gomoku, a platform fighter, a Vampire-Survivors-style horde game, an
  Overwatch-style FPS with lag-compensated hitscan and a Recall hero power that is literally
  the timeline, a Godot 3D MMO backend, a collaborative CRDT whiteboard, a tower defense,
  and Hearth, a town of LLM residents on daily schedules whom human visitors walk into and
  talk to. The browser demos run the Rust core compiled to WASM through a hand-written FFI
  (no wasm-bindgen), with a determinism gate asserting native and WASM produce byte
  identical checksums.

The "zero external dependencies" claim is verified: the lockfile contains only local
workspace crates, and `unsafe` appears only in the FFI shim. SHA-1, base64, the WebSocket
protocol, JSON, the PRNG, and fixed-point math are all hand-rolled in std.

It is fair to call opcusdb demo-grade rather than production-grade: the snapshot broadcast
is O(state x clients x tick) and the area-of-interest layer that would tame it exists in
the core but is not wired into the live servers, the JSON handling is substring scanning
rather than a real parser, there is no inbound TLS or auth, and the multi-threaded
scheduler is deliberately deferred. These are explicitly noted as scope choices, not
oversights.

## Category 1: Realtime sync and backend-as-a-service

| Project | Stack | Sync model | Hosting / license |
|---|---|---|---|
| SpacetimeDB | Rust core, WASM modules | Authoritative DB with live SQL-query subscriptions | Source-available (BSL to AGPL), self-host or Maincloud |
| Convex | Rust backend, TS functions | Reactive query-sync, strong consistency | FSL to Apache-2.0, cloud or self-host |
| ElectricSQL | Elixir server, TS client | Postgres read-sync via query "Shapes" | Apache-2.0 |
| Rocicorp Zero | TS and Rust | Query-sync plus optimistic-mutation reconciliation | Source-available |
| Liveblocks | Proprietary, JS SDKs | Managed CRDT (Yjs) plus presence | Hosted SaaS only |
| PartyKit / PartyServer | TS on Cloudflare Durable Objects | Server-authoritative rooms plus pub/sub | MIT, runs on Cloudflare |
| Ably | Proprietary | Pub/sub channels and presence at scale | Hosted SaaS only |
| Firebase / Firestore | Proprietary on GCP | Realtime listeners (last-write-wins) | Hosted SaaS only |
| Supabase Realtime | Elixir/Phoenix | Broadcast, presence (CRDT), Postgres changes | Apache-2.0 |

Recent context: SpacetimeDB reached 1.0 in February 2025 and is now past v1.11 with a 2.0
release candidate. ElectricSQL pivoted away from its original local-first SQLite-client
design to the Postgres read-sync engine and shipped a roughly 100x faster storage engine in
v1.1. Rocicorp Zero reached GA in 2026. PartyKit was acquired by Cloudflare.

The most direct conceptual peer is **SpacetimeDB**: it is the only mainstream project that,
like opcusdb, fuses the database and the authoritative server into a single Rust unit aimed
at games and live worlds. The contrast is sharp, though. SpacetimeDB syncs through
authoritative live SQL queries (no CRDTs), runs your logic as sandboxed WASM modules, and
sits on a full crate ecosystem and a WASM runtime. opcusdb hand-rolls the wire protocol and
storage, adds CRDTs as a first-class algebra, and links nothing external.

## Category 2: CRDT libraries

| Library | Stack | Algorithm | License |
|---|---|---|---|
| Yjs (and Yrs, the Rust port) | JS core, Rust port | Modified YATA sequence CRDT, rich provider ecosystem | MIT, the production default |
| Automerge | Rust core since 2.0 | RGA-style sequence, compressed columnar binary | MIT, local-first JSON. 3.0 cut steady-state memory by an order of magnitude |
| Loro | Rust core | Fugue plus Eg-walker, the richest data-type set, shallow snapshots | MIT, performance focused, reached 1.0 in late 2024 |
| diamond-types | Rust | Pioneered Eg-walker (event-graph walker) | ISC, research-grade "fastest CRDT" |

The major 2024 to 2026 advance here is the **Eg-walker** approach: store a plain operation
log and reconstruct a CRDT only transiently at merge time, bridging the performance gap
between operational transform and CRDTs. Notably, all four mature CRDT projects now center
on Rust cores with WASM and native bindings, which is the same language opcusdb chose.

opcusdb's relevance is that it hand-rolls its own `OrSet` and `Rga` rather than depending on
any of these. That trades their battle-tested correctness, rich-text support (Peritext and
the like), and provider ecosystems for full control and zero dependencies. Importantly,
none of these libraries are servers or databases: they are merge cores you must wrap.
opcusdb embeds its CRDTs inside a server and a world model, which the whiteboard demo
(an `OrSet` of strokes with offline merge on reconnect) shows end to end.

## Category 3: Multiplayer game servers and netcode

| Framework | Stack | Netcode model | License / hosting |
|---|---|---|---|
| Colyseus | TS / Node | Authoritative rooms with delta-compressed state sync | MIT, self-host or cloud |
| Nakama | Go on Postgres | Authoritative match loop, persistent backend | Apache-2.0 plus Enterprise |
| Photon (Quantum / Fusion) | Proprietary, C# / Unity | Quantum is deterministic predict-rollback; Fusion is authoritative with prediction | Proprietary, mostly hosted |
| Rivet | Rust core, TS actors | Durable stateful actors, state sync | Apache-2.0, repositioned "for the agentic era" in 2025 |
| Hathora | Hosted orchestration | (hosting, not netcode) | Shutting down game hosting in 2026 |

The pattern opcusdb's FPS and tower-defense demos implement is the classic authoritative
server with client-side prediction, reconciliation (replay unacknowledged inputs on a
corrected state), and snapshot interpolation for remote entities. Its closest spirit is
**Colyseus**: room-based authoritative state replication. The difference is that Colyseus is
Node plus optional Redis to scale, while opcusdb is a single Rust binary with its own
WebSocket and no external runtime. The deterministic-rollback lineage (GGPO, the Rust port
GGRS, Photon Quantum) is a cousin: opcusdb's determinism and Timeline give it the
ingredients for rollback (its FPS Recall power is exactly a timeline rewind), though it does
not ship a turnkey rollback-netcode library the way Quantum does.

## Category 4: Embedded and in-process databases

| DB | Lang | Model | Notes |
|---|---|---|---|
| SQLite | C | Relational OLTP, single file | Public domain, ubiquitous, added JSONB in 3.45 |
| redb | Pure Rust | Typed key-value tables, copy-on-write B-trees, MVCC, ACID | The closest pure-Rust embedded peer |
| sled | Rust | Ordered key-value, log-structured | Effectively dormant, a rewrite is ongoing |
| DuckDB | C++ | Relational OLAP, columnar, vectorized | MIT, analytics focused |

A relevant 2025-2026 note is the SQLite-in-Rust rewrite now shipping as Turso Database
(beta), distinct from the production C fork libSQL. The contrast with opcusdb is simple:
these are storage engines only, with no realtime sync, no CRDTs, and no server. Even the
pure-Rust ones (redb, sled) are dependencies that opcusdb deliberately forgoes by treating
durability as input-log replay rather than an on-disk format. opcusdb is not trying to be a
better redb; it has no persistent query store at all, by design.

## Category 5: AI-agent simulations

| Project | Stack | Architecture | License |
|---|---|---|---|
| Stanford Generative Agents (Smallville) | Python, Phaser front end | Memory stream (recency, importance, relevance) plus reflection and planning, researcher-run, no live human join | Apache-2.0, the seminal artifact |
| Generative Agent Simulations of 1,000 People | Python | Same lineage applied to replicate 1,052 real interviewed people | MIT |
| a16z / Convex "AI Town" | TypeScript end to end, React plus PixiJS | Same memory-stream pattern on Convex (DB, vector search, scheduling, transactions); humans can walk in and chat | MIT |

**AI Town is the closest analog to opcusdb's Hearth demo**, and the comparison is the most
instructive in this whole document. AI Town gets its realtime sync, reactive database,
vector search, scheduling, and transactions for free from Convex (TypeScript, a large
dependency stack, the Convex runtime), and runs its LLMs through SDKs, by default a local
Llama via Ollama. Hearth reproduces the same loop, that is, LLM residents on daily
schedules whom embodied human visitors join and converse with, on opcusdb's own ECS, its
own CRDT-capable world, and its hand-rolled WebSocket, with outbound LLM calls via system
`curl` and a free-model fallback chain instead of an SDK. Both use the Park-style memory and
reflection idea; the entire substrate underneath is the difference. Hearth also adds a twist
the research artifacts did not emphasize: it is a shared, multiplayer, human-and-AI society
rather than an offline simulation to observe.

## What spanning all five layers trades away, and what it gains

opcusdb is best understood not as a drop-in replacement for any one of these at production
scale, but as the only artifact that unifies all the layers from scratch. The competitor
set splits cleanly into three groups:

- Hosted SaaS (Ably, Firebase, Liveblocks, Photon): convenience and scale, but closed and
  rented.
- Open frameworks on heavy substrates (Convex, SpacetimeDB, Colyseus, Nakama, ElectricSQL,
  Zero, Rivet, AI Town): powerful, but dependency-laden and runtime-bound.
- Single-layer libraries (Yjs, Automerge, Loro, diamond-types; SQLite, sled, redb, DuckDB):
  excellent at one job, but you assemble the rest yourself.

What opcusdb trades away:

- **Production hardening.** It does not inherit decades of SQLite testing, the correctness
  proofs behind Yjs and Automerge, or the scale battle-testing of Photon and Ably.
- **Security surface.** A hand-rolled WebSocket and `curl`-shelling mean no native TLS
  termination, auth, or rate limiting that hosted platforms provide turnkey. (The Hearth
  server does add app-level hardening: prompt-injection resistance, input caps, slowloris
  and connection-flood limits, and keeping the API key out of the process list.)
- **Horizontal scale and operations.** No Redis or Postgres-backed clustering, no global
  edge network, no managed durability, replication, or SLAs.
- **Functionality and ecosystem.** No rich-text CRDT, no SQL optimizer, no matchmaking or
  LiveOps suite, no multi-language client SDKs, no integration marketplace.

What it gains:

- **Auditability.** The whole stack, that is, the wire protocol, the merge algorithms, the
  storage model, the server loop, the agent memory loop, is readable in one repository, with
  no opaque third-party internals.
- **Zero supply chain.** No crates.io or npm dependency graph means no transitive CVEs, no
  audit surface, no version churn, and reproducibility from a single source tree.
- **Portability.** A self-contained Rust binary with no runtime (no Node, no BEAM, no
  Postgres, no WASM host, no Redis) deploys anywhere Rust compiles, with nothing to
  provision.
- **Pedagogical clarity.** Because it implements CRDTs, WebSockets, an authoritative tick
  loop, fixed-point determinism, and an agent memory loop from first principles, it doubles
  as a legible reference for how each layer actually works, which the abstraction-heavy
  frameworks deliberately hide.
- **Conceptual unity.** One language and one data model flow through database, CRDT,
  realtime server, and both game and agent demos, eliminating the impedance mismatches
  (TypeScript to Rust to SQL, client SDK to server to database) that the multi-product
  stacks juggle. The same engine demonstrably powers an FPS, a tower defense, a CRDT
  whiteboard, and an LLM town, a breadth no single competitor demonstrates.

## Bottom line

If you need to ship a production multiplayer game, use Colyseus, Nakama, or Photon. If you
need collaborative editing, reach for Yjs or Automerge. If you want a reactive backend with
an agent town, Convex and AI Town are the path of least resistance. If you want an embedded
store, SQLite or redb. opcusdb's value is orthogonal to all of them: it is a single,
auditable, dependency-free Rust engine that shows how the database, the merge algebra, the
authoritative network loop, and an AI-agent world can be one coherent system built from the
ground up. Its closest neighbors are SpacetimeDB on the database-plus-server axis, AI Town
on the agent-world axis, Colyseus on the game-netcode axis, and the Rust CRDT cores on the
merge axis, but none of them, and no combination you can buy as one product, occupies the
same point: all of it, from scratch, in one language, with nothing underneath.
