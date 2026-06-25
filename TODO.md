# opcusdb — build checklist (loop working memory)

The 5-min build loop reads this each iteration. Steps per iteration:
read code → think → evaluate → check goal → verify → read more → think deeper → iterate.

**GOAL:** build opcusdb per `CORE_SPEC.md`, starting with the `fsm-lab` core
(ECS + algebra + Timeline + statechart engine in Rust), then the demos/tests below.

## Done
- [x] Toolchain: rustup + stable 1.96 installed.
- [x] Workspace scaffold (`Cargo.toml`, `crates/opcusdb-core`).
- [x] §4 Entities: generational ids + LIFO allocator (5 tests).
- [x] §5 SparseSet storage: O(1) insert/get/remove, swap-remove, stale-id protection (4 tests).
- [x] §5 `World`: entities + type-erased component stores + resources; `despawn` clears all stores (closes the stale-id cleanup contract). (5 tests)
- [x] §8 Query layer: deterministic ascending joins `query`/`query2`/`query3` + `matching<F>`; read-only joins. (5 tests)
- [x] §8 Commands: deferred spawn (builder)/despawn/insert/remove, ordered deterministic apply barrier. (5 tests)
- [x] §7 algebra (new crate `opcusdb-algebra`): `reduce`/`fold`, `Lattice` + `assert_lattice_laws`, CRDT catalog (LwwReg, GCounter, PNCounter, OrSet) with law + convergence tests. (8 tests)
- [x] §10 Timers (new crate `opcusdb-time`): `Tick` + `Timers<E>` min-heap, `after`/`every`/`cancel`/`due`, deterministic same-tick ordering + catch-up. (6 tests)
- [x] §9 Timeline (`opcusdb-time`): generic `Sim` + `Timeline` — fixed-timestep `advance`, keyframe ring, `seek` (rollback/scrub), branch-on-advance, `replay` oracle. Acceptance #1 (replay==live) + #2 (rollback+resim) covered. (8 tests)
- [x] §11 Statechart engine (new crate `opcusdb-fsm`): SCXML-class compound/parallel/leaf, LCA exit/entry, child-over-ancestor priority, eventless + internal transitions, guards, raised events, RTC. `StateChart` (immutable) vs `MachineState` (Clone, snapshot-friendly). (8 tests)
- [x] Project renamed realdb → **opcusdb** (all crates, dirs, docs); build still green.
- [x] **fsm-lab demo** (`demos/fsm-lab`): traffic intersection statechart — parallel car-lights + pedestrian regions, cross-region interlock guard, timer-driven phases — driven through the `Timeline`. Runnable CLI + acceptance tests (safety invariant, phase order, pedestrian interlock, replay==live, rollback+resim, scrub). (6 tests)
- [x] §2 `Rng` (opcusdb-core): deterministic PCG32, `seed`/`next_u32`/`next_u64`/`below` (unbiased Lemire)/`range`/`chance`, `Clone+Eq` for snapshots. (7 tests)
- [x] fsm-lab **cars**: per-axis lanes (queue/crossed/wait/max_queue) with `Rng::chance` arrivals, crossing only while light is "go"; metrics in CLI (`--seed`). Replay/rollback proven to reproduce RNG-driven traffic exactly. (8 fsm-lab tests)
- [x] fsm-lab **Scene B quest graph** (`quest.rs`): deep hierarchy `quest>active>{collecting,escorting}`, ctx-driven eventless guards, internal transitions, ancestor `active--Timeout-->failed` (timer via pending-timer pattern), replay+rollback. (6 quest tests)
- [x] fsm-lab **record/replay/scrub CLI** (`record.rs`): std-only golden-trace format; `replay` re-runs from seed and asserts byte-exact reproduction (detects tampering, exit 1); `scrub --to T` via `Timeline::seek`. End-to-end verified against a file. (4 record tests)
- [x] `cargo test` green (79: 31 core + 8 algebra + 14 time + 8 fsm + 18 fsm-lab), clippy clean. Full CLI verified.

NOTE: fsm-lab (CORE_SPEC §12) is COMPLETE — Scene A (intersection) + Scene B (quest) + run/record/replay/scrub CLI. Acceptance coverage: replay determinism ✓, rollback equivalence ✓, safety invariant ✓, statechart conformance ✓, CRDT laws ✓, replay-from-disk ✓. `serial==parallel` N/A (no parallel scheduler yet).

- [x] `Rga` CRDT (opcusdb-algebra `rga.rs`): causal-tree ordered sequence for text/chat — `insert_after`/`append`/`delete` (monotonic tombstones), iterative DFS materialization, sibling order by id desc, `Lattice` merge with law + convergence tests. Unblocks the chatroom demo. (5 tests)

- [x] **Human+AI chatroom demo** (`demos/chatroom`): CRDT mesh — `Rga<Message>` log + `OrSet<String>` presence; in-process gossip; scripted offline-partition + heal-to-convergence; AI agent as a peer (perceive log → `agent_reply` → append). CLI + 5 tests + README. (5 tests)

- [x] **Multi-user load test** (`demos/load-test`): swarm of N movers on the ECS `World` (first real World-in-sim use); seeded/deterministic (position checksum); throughput bench. Verified **100k entities × 100 ticks ≈ 41M updates/s (~2.4ms/tick)**, well under a 50ms budget. CLI + 5 tests + README. (5 tests)
- [x] `cargo test` green (94: 31 core + 13 algebra + 14 time + 8 fsm + 18 fsm-lab + 5 chatroom + 5 loadtest), clippy clean.

- [x] **`select` + change detection** (`opcusdb-core`): per-store mutation `version` (bumped on insert/remove/get_mut/iter_mut), `World::component_version`, and `Select<T>` — a **memoized derived view** that recomputes only when a declared dependency changed. Completes the **5-primitive algebra** (reduce·merge·select·query·fold) and is the basis for reactive subscriptions. (3 tests, 55 core)

- [x] **Top-level `README.md`**: project entry point — thesis, crate map, five-game-types→demos table, quick-start (test/run/browser/determinism gate), docs index, status. Verified accurate against actual structure (5 crates + FFI + 6 demos, 136 tests / 28 binaries, ~7.4k LoC).

## Requested demos — status
- [x] State machines → `fsm-lab` (intersection + quest).
- [x] Human + AI chatroom → `demos/chatroom`.
- [x] Multi-user / many-user load → `demos/load-test`.
- [x] Web / PixiJS → `bindings/wasm`: ECS core compiled to `wasm32` (no wasm-bindgen, minimal C-ABI), PixiJS harness (`web/index.html`+`main.mjs`), `build.sh`. **Headless-verified in Node** (`verify.mjs`: determinism + bounds). Browser render not auto-tested (no headless browser here) but the WASM contract is verified.
- [x] Simple Unity test → `bindings/ffi/native/Unity_OpcusdbSwarm.cs` (C# P/Invoke over the native cdylib). Native C-ABI **verified runnable** via `native/verify.c` (`NATIVE VERIFY OK`). C# script is glue (no engine in CI).
- [x] A few Godot tests → `bindings/ffi/native/Godot_OpcusdbSwarm.cs` (Godot 4 .NET, same C-ABI). GDScript path = GDExtension over the same ABI (noted).

- [x] **Particle galaxy demo** (`demos/particles` + `bindings/ffi` `pfield_*` + `web/particles.html`): interactive fixed-point attractor/swirl sim on the ECS; mouse attracts, hold repels; runs in WASM, PixiJS glow render. (4 tests)
- [x] **Netcode/cooldown demo** (`demos/netcode`): WoW-style ability cooldown (GCD + ability_cd as deterministic decay + ready-guard); proves **lag handling = Timeline rollback** — late input rolled back & re-simulated == on-time state; misprediction correction. CLI + 4 tests. Answers "does it handle network lag / cooldowns".

- [x] **Simulated network + prediction/reconciliation loop** (`demos/netcode` `net.rs`): deterministic `Link` (latency/jitter/loss), reliable up-link + lossy latest-wins down-link, `Session` = client-predicts-instantly + server-authoritative + reconcile-to-snapshot+replay-unacked. Tests: converges under latency, converges despite 60% snapshot loss, prediction is instant, deterministic. (4 net tests)

- [x] **Visual netcode demo** (`bindings/ffi` `session_*` + `web/netcode.html`): predicted client dot vs lagging authoritative server ghost, live latency/loss sliders; the whole client/server loop runs in WASM. Node-verified the session exports (client leads under latency, both converge on drain).

## Network lag — where it stands
- Prediction + reconciliation + lag-comp are the Timeline (DESIGN §5). Mechanism BUILT + verified. Cooldown over lag DEMONSTRATED. **Full predict/reconcile loop over a simulated laggy/lossy link DEMONSTRATED + tested** (`net.rs`) and **VISUALIZED in-browser** (`netcode.html`).
- NOT yet built: the **real transport** — `Link` is the seam; swap in QUIC (native, `quinn`) + WebRTC datachannels (browser) and the loop above is unchanged.

## Browser demos (served from bindings/ffi/web)
- `index.html` — swarm (load test). `particles.html` — interactive particle galaxy. `netcode.html` — prediction-vs-authority with lag sliders.

## Next up — Phase 1+ (user may want to steer the track)
- [ ] **Three.js demo** reusing the same wasm C-ABI (3D dots) — cheap, builds on what's done.
- [ ] **Unity/Godot bindings**: expose the C-ABI as a native cdylib (P/Invoke for Unity/Godot-mono; GDExtension for Godot) — reuse the swarm contract.
- [ ] **Networking track (DESIGN §6)**: QUIC (native) + WebRTC (browser) so the chatroom mesh + swarm run across machines.
- [x] **World snapshot-ability**: `World` is now deep-`Clone` (components + resources require `Clone`; erased `dyn_clone`/`res_clone`; resources via stable trait upcasting). Snapshot-independence test. This is the in-memory snapshot (§9) — no serializer needed for rollback. (32 core tests)
- [x] **Wired `World` into `Timeline`** (new crate `opcusdb-ecs`): `EcsLogic` marker trait (zero-sized logic) + `EcsWorld<G>` (World + PhantomData) implementing `Sim`. ECS sims now get rollback/replay. Proven: 3000-entity swarm replay==live and rollback+resim==original at scale. (3 ecs tests + 1 loadtest scale-rollback test)

### ECS is now a first-class rollback-able sim
The ECS World composes with the Timeline exactly like the hand-written sims — every netcode/replay capability (prediction, reconciliation, scrub, byte-exact replay) now applies to ECS games too.
- [x] **Query improvements**: pick-smallest-store join (O(min) candidates for asymmetric joins, e.g. few Velocity among many Position), `matching_without::<F, X>` exclusion filters, and safe ordered `World::for_each_mut` (deterministic single-component mutation, no get_mut dance). (3 new core tests, 35 total)
- [x] **Scheduler** (`opcusdb-core::scheduler`): `Schedule` with `.system(name).reads::<T>().writes::<T>().build(fn)`; conflict detection, parallel **stage** computation, deterministic serial `run`, `plan()` introspection. Verified the parallel-safety property: **independent systems commute** (reordering them is equivalent). (6 tests, 41 core)
- [x] **Spatial index** (`opcusdb-core::spatial`): uniform `SpatialGrid` for AOI/interest queries — `insert`/`clear`/`query_aabb`/`query_radius` (exact, ascending), cheap per-tick rebuild. Validated against brute-force over thousands of random entities. The #1 MMO scaling lever (interest management). (4 tests, 45 core)
- [ ] **Multi-threaded stage execution**: hand each thread a disjoint World slice (needs encapsulated `unsafe` World-splitting). The scheduling/analysis is done; only the thread-pool remains. Would also enable safe multi-store mutable joins.
- [x] Used `SpatialGrid` for AOI: swarm `mark_near` interest sets (verified vs brute-force), exposed via FFI, and the swarm browser demo highlights the interest set following the cursor.
- [x] §3 **Fixed-point math** (`opcusdb-core::fx`): `Fx` Q16.16 (i32 + i64 intermediates), `+ - * / neg` operators, `from_int`/`frac`/`from_num`/`to_f64`/`floor_int`/`abs`/`sqrt`/`clamp`; pure-integer → cross-platform identical bits. Unblocks deterministic real-valued / lockstep sims. (7 tests, 52 core)
- [x] **Lockstep MOBA demo** (`demos/lockstep`, DESIGN §7 LoL target): deterministic inputs-only `Match` sim using fixed-point `Fx`; two peers fed the same input log stay **byte-identical every tick** (the lockstep guarantee), plus Timeline replay + rollback. CLI + 4 tests.

- [x] **Cross-target determinism gate** (`bindings/ffi/determinism_gate.sh`): runs the same swarm on the **native** build and the **WASM** build and asserts byte-identical checksums (verified `0xf9a6e033d627b0d3` native == wasm for 2000×120). Proves CORE_SPEC's "byte-identical across platforms" — the bedrock of lockstep/replay. Exposed `swarm_checksum` via FFI.

- [x] **WAL persistence + crash recovery** (`demos/netcode/wal.rs`, CORE_SPEC §9 Phase 1): std-only append-only input log; recovery = fresh deterministic sim + replay (no World serialization needed). Survives "crash"; truncated trailing line skipped (last-durable-tick guarantee); recovery == in-memory Timeline replay. The cooldown state survives a server restart. (3 tests)

### All five DESIGN game types now demonstrated
- WoW (persistent/AOI) → swarm + `SpatialGrid` interest sets. Overwatch (room, predict/reconcile) → `demos/netcode`. **LoL (lockstep) → `demos/lockstep`.** State machines → `fsm-lab`. Human+AI chat → `demos/chatroom`.

## Decisions pending (CORE_SPEC §14)
1. Serializer: rkyv vs bitcode (leaning rkyv).
2. SCXML importer now or Phase 5 (leaning defer; Rust builder API first).
3. select memoization granularity (start coarse: component version).
4. Snapshot: full-copy double-buffer first; CoW behind same trait later.
5. fsm-lab UI: CLI-first; web inspector later.

## Notes / invariants
- Determinism contract (CORE_SPEC §2) is non-negotiable for all sim code.
- Build/test: `source "$HOME/.cargo/env" && cargo test --workspace`.
- `unsafe_code = warn` workspace-wide; prefer safe Rust.
