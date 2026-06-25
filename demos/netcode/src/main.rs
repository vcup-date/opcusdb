//! `cooldown` CLI, spam-cast an ability and watch the WoW-style cooldown gate
//! the casts, tick by tick. Pure deterministic sim driven by the Timeline.

use opcusdb_netcode::{Action, Combat, ABILITY_CD, GCD};
use opcusdb_time::Timeline;

fn main() {
    let ticks = 14u64;
    let mut tl = Timeline::new(Combat::default(), 8, 16);

    println!("WoW-style cooldown (GCD={GCD}, ability_cd={ABILITY_CD}); spamming Cast every tick\n");
    println!("tick | action | gcd acd | result");
    println!("-----+--------+---------+--------");
    for _ in 0..ticks {
        let ready = tl.state().ready();
        tl.advance(vec![Action::Cast]);
        let s = tl.state();
        println!(
            "{:>4} |  Cast  |  {}   {}  | {}",
            tl.tick().get(),
            s.gcd,
            s.ability_cd,
            if ready { "CAST!" } else { "on cooldown" }
        );
    }
    let s = tl.state();
    println!("\ntotals: {} casts, {} rejected", s.casts, s.rejected);
}
