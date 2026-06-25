//! `lockstep` CLI, run a tiny MOBA match and show that two peers fed the same
//! inputs stay byte-identical (the lockstep guarantee). Pure deterministic
//! fixed-point sim driven by the Timeline.

use opcusdb_lockstep::{Cmd, Match};
use opcusdb_time::{Sim, Tick};

fn main() {
    // A scripted set of orders (what would cross the wire as "inputs only").
    let script: Vec<Vec<Cmd>> = vec![
        vec![Cmd::move_to(0, 80, 10), Cmd::move_to(2, 50, 30)],
        vec![],
        vec![Cmd::move_to(1, 20, 90)],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
    ];

    // Two independent peers run the same deterministic sim on the same inputs.
    let mut peer_a = Match::new(2, 2);
    let mut peer_b = Match::new(2, 2);

    println!("tick | peerA checksum     | in sync");
    println!("-----+--------------------+--------");
    let mut synced = true;
    for (t, cmds) in script.iter().enumerate() {
        peer_a.step(Tick(t as u64), cmds);
        peer_b.step(Tick(t as u64), cmds);
        let ok = peer_a.checksum() == peer_b.checksum();
        synced &= ok;
        println!("{:>4} | {:#018x} | {}", t, peer_a.checksum(), if ok { "yes" } else { "DESYNC" });
    }

    println!("\nfinal unit positions (owner @ x,y):");
    for (owner, x, y) in peer_a.units() {
        println!("  p{owner} @ {x},{y}");
    }
    println!(
        "\ntwo peers stayed in perfect lockstep: {}",
        if synced { "yes" } else { "NO" }
    );
}
