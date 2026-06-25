<div align="center">

# opcusdb

**A deterministic, time-aware, policy-driven replicated ECS** for real-time
multiplayer games and AI-agent worlds — written in Rust, dependency-free.

![license](https://img.shields.io/badge/license-MIT-blue)
![rust](https://img.shields.io/badge/rust-1.80%2B-orange)
![tests](https://img.shields.io/badge/tests-158%20passing-success)
![deps](https://img.shields.io/badge/dependencies-none-brightgreen)
![targets](https://img.shields.io/badge/targets-native%20%2B%20WASM-informational)

*One deterministic core → a persistent MMO, a 60 Hz room shooter, a lockstep MOBA,
embedded state machines, and a serverless human + AI chatroom.*

</div>

---

## What it is

opcusdb is a small engine that treats a game/world as a **pure, deterministic
function of its inputs**. That single property is the spine of everything:

- **Replay** — re-run the input log and get byte-identical state.
- **Rollback** — rewind and re-simulate (client prediction, server reconciliation, lag compensation).
- **Lockstep** — every peer stays in sync exchanging only inputs.
- **Durability** — recover after a crash by replaying the log (no serializer needed).

The thesis (see [`DESIGN.md`](DESIGN.md)): **consistency / authority / topology is a
_policy_ layered over a common core, not a hard-coded choice** — so the *same*
engine serves wildly different netcode models. Every one of the five below is
demonstrated with running, tested code.

```
158 tests · 34 binaries · ~7.4k LoC Rust · clippy-clean · zero external deps
native + WASM proven byte-identical (cross-target determinism gate passes)
```

## See it run

The simulations run entirely in the **Rust core compiled to WASM**; the browser
only renders. (Screenshots are captured headlessly from the live demos.)

| | |
|:--:|:--:|
| <img src="assets/swarm-aoi.png" width="420"/> | <img src="assets/netcode-lag.png" width="420"/> |
| **Spatial AOI** — 4,000 entities; the **interest set** near the cursor is highlighted (MMO interest management) | **Netcode under lag** — the blue **predicted** client leads; the orange **authoritative server** ghost trails and reconciles |
| <img src="assets/netcode-sync.png" width="420"/> | <img src="assets/particles-attract.png" width="420"/> |
| **Same netcode, low latency** — client & server overlap (one loop, different lag) | **Particle galaxy (attract)** — fixed-point physics in the ECS, drawn with PixiJS |

<div align="center"><img src="assets/particles-repel.png" width="420"/><br/><b>Particle galaxy (repel)</b> — hold the mouse to blast particles outward</div>

> The four demos above run the engine **locally in one tab** (WASM) — they showcase
> the engine and netcode *logic*, not a shared session. For **actual multiplayer**,
> see below.

## Multiplayer — many browsers, one authoritative world

`opcusdb-server` runs the **real opcusdb ECS engine on the server** as the single
source of truth; browsers are thin clients that send inputs over **WebSocket** and
render the state the server broadcasts. Open it in several tabs/devices and
everyone shares the same live world — every cursor and every server-simulated dot.

```sh
cargo run -p opcusdb-server      # then open http://localhost:9001 in 2+ tabs
```

<div align="center">
<img src="assets/diagram-multiplayer.png" width="560" alt="multiplayer topology"/><br/>
<img src="assets/multiplayer.png" width="460"/><br/>
<b>One client's view of a shared world</b> — your white cursor, other players (p3/p5/p6), and dots the server simulates for everyone.
</div>

> The WebSocket server is **dependency-free** too: the HTTP serving and the
> WebSocket protocol (handshake SHA-1/base64 + framing) are hand-rolled in std
> Rust (`demos/server/src/ws.rs`). The shared-world logic and the WS handshake are
> unit-tested; the live path was checked with multiple concurrent browser clients.

## Human + AI chatroom (live, over OpenRouter)

`opcusdb-chat` is an IRC-style `#lobby`: **anyone logs in with a nick**, and **10
AI chatters** (OpenRouter, `deepseek/deepseek-v4-flash`) talk with you and each
other. Same hand-rolled WebSocket server; the AI calls go out through the system
`curl` (no HTTP/TLS dependency).

<div align="center">
<img src="assets/chatroom.png" width="620"/><br/>
<b>#lobby</b> — a human (<code>visitor</code>) and 10 AI chatters with distinct personas, talking to each other in real time.
</div>

```sh
export OPENROUTER_API_KEY=sk-or-...                 # your key — never committed
cargo run -p opcusdb-server --bin opcusdb-chat      # then open http://localhost:9002
```

The key is read from **`OPENROUTER_API_KEY`** and is **never stored in the repo**
(`.env` and `run-chat.sh` are gitignored; see [`.env.example`](.env.example)). To
save credits, the bots only chat while at least one human is connected.

## Gomoku — online Five-in-a-Row (五子棋) with rooms & a win leaderboard

`opcusdb-gomoku` is turn-based online **five-in-a-row** on a 15×15 Go board. A
**lobby lists the open rooms** (with player counts + status) so you can click to
join or watch — or create a new room. Black moves first; first to **5 in a row**
(any direction) wins. The Rust server is authoritative — it validates every move
and detects the win — and **win counts persist to a local DB file** (`gomoku.db`,
gitignored) as an all-time leaderboard.

<div align="center">
<img src="assets/gomoku-lobby.png" width="320"/> <img src="assets/gomoku.png" width="430"/><br/>
<b>Lobby</b> (live room list — join / watch / create) and a finished <b>game</b> (winning line highlighted, leaderboard).
</div>

```sh
cargo run -p opcusdb-server --bin opcusdb-gomoku     # open http://localhost:9004
# create a room, share the code (or ?room=CODE); 2nd player is white
```

## Arena — a multiplayer game with rooms, rules & a persistent leaderboard

`opcusdb-arena` is a real-time multiplayer **snake** game: **create or join a room
by code**, steer with arrows/WASD, eat food to grow and score, crash into a
wall/snake and you die (then auto-respawn). The Rust server is authoritative (one
grid per room, fixed tick, broadcast over WebSocket); scores persist to a small
**local DB file** (`leaderboard.db`, gitignored) shown as an all-time leaderboard.

<div align="center">
<img src="assets/arena.png" width="600"/><br/>
<b>opcusdb Arena</b> — neon board, per-room scores, and a persistent all-time leaderboard.
</div>

```sh
cargo run -p opcusdb-server --bin opcusdb-arena      # open http://localhost:9003
# create a room, share the code (or ?room=CODE), open more tabs and race
```

## Architecture

<div align="center"><img src="assets/diagram-arch.png" width="560" alt="architecture"/></div>

### Crate dependency graph

<div align="center"><img src="assets/diagram-crates.png" width="720" alt="crate dependency graph"/></div>

## How determinism powers everything

<div align="center"><img src="assets/diagram-determinism.png" width="760" alt="determinism enables replay, rollback, lockstep, WAL, cross-target"/></div>

### Mechanism: client prediction & server reconciliation

<div align="center"><img src="assets/diagram-sequence.png" width="720" alt="client prediction and server reconciliation sequence"/></div>

> Diagram sources live in [`assets/diagrams/`](assets/diagrams/) (`*.mmd`).

## Crate map

| Crate | What it is |
|---|---|
| `opcusdb-core` | ECS: generational entities, sparse-set storage, **snapshot-able `World`**, deterministic queries (joins / exclusion / pick-smallest / `for_each_mut`), command buffer, **system scheduler**, **spatial grid (AOI)**, change-detection + memoized **`select`**, **PRNG**, **fixed-point `Fx`** |
| `opcusdb-algebra` | the functional sync algebra — `reduce` · `merge` · `select` · `query` · `fold` — and CRDTs (`LwwReg`, `GCounter`, `PNCounter`, `OrSet`, `Rga`) with law-checkers |
| `opcusdb-time` | the **Timeline**: fixed-timestep loop, keyframe ring, **rollback / scrub / replay**, deterministic timers |
| `opcusdb-fsm` | hierarchical + parallel **statechart** engine (SCXML-class) |
| `opcusdb-ecs` | bridge: run an ECS `World` as a Timeline `Sim` (rollback/replay for ECS games) |
| `bindings/ffi` | one minimal **C-ABI** over the sims → **WASM** (browser) and **native** (Unity/Godot/C); no `wasm-bindgen` |
| `demos/server` | four authoritative servers over a **hand-rolled WebSocket** (dependency-free): a shared-world game, a **human/AI chatroom** (OpenRouter via `curl`), **Gomoku** (5-in-a-row), and **Arena** (snake) — both with rooms + persistent leaderboards |

## The five game types → demos

| DESIGN target | Demo | Run | Shows |
|---|---|---|---|
| **WoW** (persistent + AOI) | `load-test` | `cargo run --release -p opcusdb-loadtest --bin loadtest` | many-entity ECS swarm + spatial-grid **interest sets** |
| **Overwatch** (room, predict/reconcile) | `netcode` | `cargo run -p opcusdb-netcode --bin cooldown` | **prediction / reconciliation** over a simulated laggy link + **WAL recovery** |
| **LoL** (lockstep) | `lockstep` | `cargo run -p opcusdb-lockstep --bin lockstep` | deterministic **inputs-only** fixed-point sim; peers stay byte-identical |
| **state machines** | `fsm-lab` | `cargo run -p opcusdb-fsm-lab --bin fsm-lab` | traffic intersection + quest graph; `run`/`record`/`replay`/`scrub` |
| **human + AI chat** | `chatroom` | `cargo run -p opcusdb-chatroom --bin chatroom` | serverless **CRDT mesh** (`Rga` + `OrSet`), offline-merge, **AI agent as a peer** |
| *(bonus)* | `particles` | browser | interactive fixed-point particle galaxy |
| **real multiplayer** | `server` (game) | `cargo run -p opcusdb-server` → open :9001 in 2+ tabs | **authoritative ECS server + WebSocket**; many browsers share one live world |
| **live human + AI chat** | `server` (chat) | `OPENROUTER_API_KEY=… cargo run -p opcusdb-server --bin opcusdb-chat` → :9002 | IRC-style channel; anyone logs in; **10 AI chatters via OpenRouter** |
| **Gomoku (5-in-a-row)** | `server` (gomoku) | `cargo run -p opcusdb-server --bin opcusdb-gomoku` → :9004 | turn-based **five-in-a-row**; rooms; win detection; persistent win leaderboard |
| **multiplayer game (snake)** | `server` (arena) | `cargo run -p opcusdb-server --bin opcusdb-arena` → :9003 | **rooms + rules + score + persistent leaderboard** (local DB file) |

## Quick start

```sh
# build + test everything
cargo test --workspace

# REAL multiplayer: authoritative server; open http://localhost:9001 in 2+ tabs
cargo run -p opcusdb-server

# local (single-tab) browser demos (WASM core + PixiJS): swarm/AOI, particles, netcode
bash bindings/ffi/build.sh
cd bindings/ffi/web && python3 -m http.server 8080   # open http://localhost:8080

# prove the native and WASM builds are byte-identical
bash bindings/ffi/determinism_gate.sh
# native (Rust): 0xcd8d89b3520fccd1
# wasm   (node): 0xcd8d89b3520fccd1
# DETERMINISM GATE: PASS (byte-identical across targets)
```

Native **Unity / Godot** bindings (same C-ABI): see [`bindings/ffi/native/`](bindings/ffi/native/).

## Test cases — what's actually proven

`cargo test --workspace` → **158 passing across 34 binaries**, clippy-clean. A selection:

| Property proven | Test |
|---|---|
| `World` snapshot is a deep, independent copy | `core … snapshot_is_a_deep_independent_copy` |
| Joins pick the smallest store and stay correct | `core … pick_smallest_store_join_is_correct` |
| Independent systems commute (parallel-safe) | `core … reordering_independent_systems_is_equivalent` |
| Spatial AOI matches brute force (1000s of entities) | `core … aabb_matches_brute_force` / `radius_matches_brute_force` |
| `select` recomputes only when a dependency changes | `core … memoizes_until_a_dependency_changes` |
| Fixed-point bits are identical everywhere | `core … deterministic_known_bits` |
| CRDTs obey the lattice laws + converge | `algebra … *_laws_*`, `orset_add_wins_and_converges`, `concurrent_inserts_converge` |
| Rollback + re-sim reproduces state exactly | `time … rollback_then_resim_reproduces` |
| Seek works after keyframe eviction | `time … seek_to_zero_works_after_keyframe_eviction` |
| Parallel statechart regions transition together | `fsm … parallel_regions_transition_simultaneously` |
| ECS sim gets rollback/replay (3000 entities) | `load-test … swarm_replay_and_rollback_at_scale` |
| Intersection never shows crossing greens | `fsm-lab … safety_invariant_never_two_go_axes` |
| Chat peers converge after an offline partition | `chatroom … all_peers_converge_after_partition` |
| Late input rolled back == on-time | `netcode … reconcile_late_input_matches_on_time` |
| Converges despite 60% snapshot loss | `netcode … converges_despite_snapshot_loss` |
| State survives a crash (WAL replay) | `netcode … recovers_exact_state_after_crash` |
| Two lockstep peers stay byte-identical | `lockstep … two_peers_stay_in_perfect_sync` |
| Shared world holds every connected player + spawns | `server … shared_world_holds_all_players_and_spawns` |
| WebSocket handshake (SHA-1/base64) per RFC 6455 | `server … rfc6455_accept_example` |
| Gomoku detects horizontal/diagonal five-in-a-row | `server … detects_horizontal_five`, `detects_diagonal_five` |
| Snake eats/grows; wall-crash records score | `server … snake_moves_and_eats`, `wall_collision_kills_and_records_score` |

## Status

The core and the five-game-types thesis are **complete and verified**, and
`opcusdb-server` adds **real client/server multiplayer over WebSocket**.
Still open as explicit, dependency-affecting choices: **WebRTC / QUIC** (for
browser P2P meshes and lower-latency datagrams), client-side prediction wired to
the live server (the logic exists in `demos/netcode`), multi-threaded
scheduler-stage execution (needs encapsulated `unsafe` World-splitting), and an
on-disk serializer (`rkyv`/`bitcode`). The workspace is `unsafe`-free outside the
FFI shim and has **zero external dependencies**.

## Documentation

- [`DESIGN.md`](DESIGN.md) — vision/architecture (the policy model, Timeline, sync algebra, topologies).
- [`CORE_SPEC.md`](CORE_SPEC.md) — buildable engineering spec for the core.
- [`PLAN.md`](PLAN.md) — original research survey (open-source real-time DBs) + language rationale.
- Each demo has its own README.

## License

[MIT](LICENSE).
