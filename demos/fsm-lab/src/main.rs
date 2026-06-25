//! `fsm-lab` CLI — run, record, replay, and scrub the traffic intersection.
//! A dogfood of the opcusdb core (statechart + timers + deterministic RNG + Timeline).
//!
//! Usage:
//!   fsm-lab [run] [--ticks N] [--seed S]   print the per-tick signal/queue state
//!   fsm-lab record <file> [--ticks N] [--seed S]   capture a golden trace to a file
//!   fsm-lab replay <file>                  re-run and assert it reproduces the trace
//!   fsm-lab scrub  <file> --to T           rebuild and show the state at tick T

use opcusdb_fsm_lab::record::{capture, Record};
use opcusdb_fsm_lab::{Intersection, DEFAULT_SEED};
use opcusdb_time::Timeline;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str);
    match cmd {
        Some("record") => cmd_record(&args),
        Some("replay") => cmd_replay(&args),
        Some("scrub") => cmd_scrub(&args),
        Some("run") | None => {
            cmd_run();
            ExitCode::SUCCESS
        }
        Some(other) if other.starts_with("--") => {
            cmd_run();
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("unknown command `{other}` (try: run, record, replay, scrub)");
            ExitCode::FAILURE
        }
    }
}

fn cmd_run() {
    let ticks = arg_value("--ticks").unwrap_or(20);
    let seed = arg_value("--seed").unwrap_or(DEFAULT_SEED);
    let mut tl = Timeline::new(Intersection::new_seeded(seed), 8, 4);
    println!("tick |  NS     EW    | walk | qNS qEW");
    println!("-----+---------------+------+--------");
    for _ in 0..ticks {
        tl.advance(vec![]);
        let s = tl.state();
        let c = s.ctx();
        let t = s.traffic();
        println!(
            "{:>4} | {:<6} {:<6}| {:<4} | {:>3} {:>3}",
            tl.tick().get(),
            format!("{:?}", c.ns),
            format!("{:?}", c.ew),
            if c.walk { "WALK" } else { "" },
            t.ns.waiting,
            t.ew.waiting,
        );
    }
    let t = tl.state().traffic();
    println!("\nseed {seed:#x} over {ticks} ticks:");
    println!("  NS: crossed={} max_queue={}", t.ns.crossed, t.ns.max_queue);
    println!("  EW: crossed={} max_queue={}", t.ew.crossed, t.ew.max_queue);
}

fn cmd_record(args: &[String]) -> ExitCode {
    let Some(file) = positional(args) else {
        eprintln!("usage: fsm-lab record <file> [--ticks N] [--seed S]");
        return ExitCode::FAILURE;
    };
    let ticks = arg_value("--ticks").unwrap_or(60);
    let seed = arg_value("--seed").unwrap_or(DEFAULT_SEED);
    let rec = capture(seed, ticks);
    match std::fs::write(&file, rec.to_text()) {
        Ok(()) => {
            println!("recorded {ticks} ticks (seed {seed:#x}) to {file}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("write {file}: {e}");
            ExitCode::FAILURE
        }
    }
}

fn cmd_replay(args: &[String]) -> ExitCode {
    let Some(file) = positional(args) else {
        eprintln!("usage: fsm-lab replay <file>");
        return ExitCode::FAILURE;
    };
    let rec = match read_record(&file) {
        Ok(r) => r,
        Err(code) => return code,
    };
    match rec.verify() {
        Ok(n) => {
            println!("OK: replayed {n} ticks from {file}, reproduced exactly (seed {:#x})", rec.seed);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("REPLAY FAILED: {e}");
            ExitCode::FAILURE
        }
    }
}

fn cmd_scrub(args: &[String]) -> ExitCode {
    let Some(file) = positional(args) else {
        eprintln!("usage: fsm-lab scrub <file> --to T");
        return ExitCode::FAILURE;
    };
    let rec = match read_record(&file) {
        Ok(r) => r,
        Err(code) => return code,
    };
    let Some(to) = arg_value("--to") else {
        eprintln!("scrub needs --to T");
        return ExitCode::FAILURE;
    };
    if to > rec.ticks {
        eprintln!("--to {to} exceeds recorded ticks {}", rec.ticks);
        return ExitCode::FAILURE;
    }
    // Rebuild the run and seek to the requested tick (demonstrates Timeline::seek).
    let mut tl = Timeline::new(Intersection::new_seeded(rec.seed), 8, 4);
    for _ in 0..rec.ticks {
        tl.advance(vec![]);
    }
    tl.seek(to);
    let s = tl.state();
    let c = s.ctx();
    let t = s.traffic();
    println!(
        "tick {to}: NS={:?} EW={:?} walk={} qNS={} qEW={}",
        c.ns, c.ew, c.walk, t.ns.waiting, t.ew.waiting
    );
    ExitCode::SUCCESS
}

fn read_record(file: &str) -> Result<Record, ExitCode> {
    let text = std::fs::read_to_string(file).map_err(|e| {
        eprintln!("read {file}: {e}");
        ExitCode::FAILURE
    })?;
    Record::parse(&text).map_err(|e| {
        eprintln!("parse {file}: {e}");
        ExitCode::FAILURE
    })
}

/// The first non-flag argument after the subcommand.
fn positional(args: &[String]) -> Option<String> {
    args.iter().skip(1).find(|a| !a.starts_with("--")).cloned()
}

/// Parse `--flag N` (decimal or `0x` hex) from the args.
fn arg_value(flag: &str) -> Option<u64> {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == flag {
            return args.next().and_then(|n| {
                n.strip_prefix("0x")
                    .and_then(|h| u64::from_str_radix(h, 16).ok())
                    .or_else(|| n.parse().ok())
            });
        }
    }
    None
}
