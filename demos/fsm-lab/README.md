# fsm-lab

A deterministic, replayable **traffic intersection** built on the opcusdb core, the first end-to-end demo (`CORE_SPEC.md` §12).

It exercises every core pillar at once:

| Pillar | Crate | Used for |
|---|---|---|
| Statechart | `opcusdb-fsm` | the intersection as a **parallel** machine: a `lights` region (phase cycle) and a `ped` region (walk signal) |
| Timers | `opcusdb-time` | the phase clock, each phase arms the next phase's timer |
| Timeline | `opcusdb-time` | drives the sim tick-by-tick and gives **rollback, scrub, and byte-identical replay** for free |
| Deterministic RNG | `opcusdb-core` | seeded car arrivals (replay-safe) |

Cars are modeled as per-axis queue counts (not ECS entities yet) so the whole sim
stays `Clone`, which is what lets the Timeline snapshot and roll it back. They
arrive via `Rng::chance` and cross only while their light is green/yellow.

## Run it

```sh
# print the per-tick signal + queue state
cargo run -p opcusdb-fsm-lab --bin fsm-lab -- --ticks 20 --seed 7

# record a golden trace, then verify it reproduces exactly from disk
cargo run -p opcusdb-fsm-lab --bin fsm-lab -- record run.log --ticks 60 --seed 7
cargo run -p opcusdb-fsm-lab --bin fsm-lab -- replay run.log     # OK or a frame-diff
cargo run -p opcusdb-fsm-lab --bin fsm-lab -- scrub  run.log --to 10
```

`replay` re-runs from the seed and asserts every recorded frame reproduces
byte-for-byte, a determinism check against a file (tampering exits non-zero with
the first mismatch). `scrub --to T` rebuilds the run and `Timeline::seek`s to tick T.

Example output (one full cycle is 10 ticks):

```
tick |  NS     EW    | walk
-----+---------------+-----
   1 | Green  Red   |
   4 | Yellow Red   |
   5 | Red    Red   | WALK
   6 | Red    Green |
   9 | Red    Yellow|
  10 | Red    Red   | WALK
  11 | Green  Red   |
```

## What it demonstrates

- **Parallel regions + cross-region interlock.** The pedestrian `walk` signal is
  a separate orthogonal region whose eventless transitions are guarded by
  `all_red`, context the `lights` region writes. So the walk signal can *only*
  activate while both car axes are red.
- **Safety by construction.** Car phases are mutually exclusive, so the two axes
  are never simultaneously "go". The test suite asserts this invariant on every
  tick across 200 steps.
- **Determinism → replay & rollback.** Because the sim is pure and deterministic,
  `Timeline::replay(fresh_start, log)` reproduces the live state exactly, and
  seeking to a past tick then re-simulating yields an identical state.

## Tests

```sh
cargo test -p opcusdb-fsm-lab
```

- `safety_invariant_never_two_go_axes`, no crossing greens, ever.
- `pedestrian_walks_during_all_red`, walk signal activates, only during all-red.
- `phases_cycle_in_order`, exact colour sequence over a full cycle.
- `replay_reproduces_live_state`, replay determinism (acceptance #1).
- `rollback_then_resim_reproduces`, rollback equivalence (acceptance #2).
- `scrub_back_and_forward_is_lossless`, time-scrubbing round-trips.

## Not yet (see repo `TODO.md`)

- Scene B: a quest-graph statechart (hierarchy + guards + context).
- Cars arriving via a seeded Poisson process (needs a deterministic PRNG).
- `--record` / `replay` / `scrub` CLI subcommands over a log file (needs the
  serializer decision, `CORE_SPEC.md` §14 Q1).
