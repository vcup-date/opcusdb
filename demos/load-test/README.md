# load-test

A many-entity **swarm** that exercises the ECS `World` at scale, the "many
people supported" goal, and reports throughput. This is the first use of the ECS
core (entities + components + queries) in a running simulation.

Each entity is a mover with `Position` + `Velocity`; every tick a movement system
advances all of them on a toroidal field. Positions are seeded by the
deterministic `Rng`, so a run is reproducible (the position `checksum` is stable
across runs of the same seed).

## Run it

```sh
cargo run --release -p opcusdb-loadtest --bin loadtest -- --entities 100000 --ticks 100
```

Example (Apple silicon, release build):

```
opcusdb load test
  entities : 100000
  ticks    : 100
  build    : 5.0 ms
  sim time : 242.8 ms (2.428 ms/tick)
  throughput: 41.18 M entity-updates/sec
  in center half: 24918 / 100000
```

At a 20 Hz tick (50 ms budget), 100k entities use ~2.4 ms/tick, comfortably
within budget, with large headroom for the per-zone entity counts an MMO needs.

## Honest caveats

This measures the engine **before** the planned query-layer optimizations: the
movement system uses `matching::<(Position, Velocity)>()`, which allocates and
sorts a `Vec<EntityId>` every tick (O(N log N)) and looks components up per
entity. The "pick-smallest-store" iteration and mutable joins (see repo
`TODO.md`) will cut this materially, so treat 41M updates/s as a floor.

## Tests

```sh
cargo test -p opcusdb-loadtest
```

- `deterministic_checksum_same_seed`, same seed ⇒ identical positions after N ticks.
- `different_seeds_diverge`, `movement_wraps_at_edges`, `region_count_is_bounded`.
- `scale_runs_and_stays_deterministic`, 40k entities × 20 ticks, reproducible.
