//! `loadtest` CLI, run the swarm and report throughput.
//!
//! Usage: `loadtest [--entities N] [--ticks T] [--seed S]`
//! (defaults: 100000 entities, 100 ticks, seed 1)
//!
//! Note: `std::time::Instant` is used only to *measure* the harness, the
//! simulation itself uses no wall-clock (determinism contract §2).

use opcusdb_loadtest::Swarm;
use std::time::Instant;

fn main() {
    let entities = arg_value("--entities").unwrap_or(100_000) as u32;
    let ticks = arg_value("--ticks").unwrap_or(100);
    let seed = arg_value("--seed").unwrap_or(1);

    let build_start = Instant::now();
    let mut swarm = Swarm::new(entities, seed);
    let build = build_start.elapsed();

    let run_start = Instant::now();
    for _ in 0..ticks {
        swarm.step();
    }
    let elapsed = run_start.elapsed();

    let updates = entities as u128 * ticks as u128;
    let secs = elapsed.as_secs_f64();
    let per_sec = if secs > 0.0 { updates as f64 / secs } else { 0.0 };
    let center = swarm.count_in_region(
        opcusdb_loadtest::WIDTH / 4,
        opcusdb_loadtest::HEIGHT / 4,
        3 * opcusdb_loadtest::WIDTH / 4,
        3 * opcusdb_loadtest::HEIGHT / 4,
    );

    println!("opcusdb load test");
    println!("  entities : {entities}");
    println!("  ticks    : {ticks}");
    println!("  seed     : {seed}");
    println!("  build    : {:.3} ms", build.as_secs_f64() * 1e3);
    println!("  sim time : {:.3} ms ({:.3} ms/tick)", secs * 1e3, secs * 1e3 / ticks as f64);
    println!("  throughput: {:.2} M entity-updates/sec", per_sec / 1e6);
    println!("  in center half: {center} / {entities}");
    println!("  checksum : {:#018x}", swarm.checksum());
}

/// Parse `--flag N` from the args.
fn arg_value(flag: &str) -> Option<u64> {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == flag {
            return args.next().and_then(|n| n.parse().ok());
        }
    }
    None
}
