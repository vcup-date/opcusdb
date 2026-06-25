//! A lockstep MOBA-style simulation (`DESIGN.md` §7, the LoL target).
//!
//! Lockstep netcode: every peer runs the **same deterministic simulation** and
//! exchanges only **inputs** (tiny bandwidth), never state. Apply the same inputs
//! at the same tick on every peer and they stay in perfect, byte-identical sync, //! that is the whole model, and it requires determinism, which is why the physics
//! uses fixed-point [`Fx`] (no platform-divergent floats) and the entity order is
//! fixed. With the [`Timeline`](opcusdb_time::Timeline) it also gets rollback for
//! late/predicted inputs (GGPO-style).
//!
//! [`Match`] is the deterministic sim ([`Sim`](opcusdb_time::Sim)); inputs are
//! [`Cmd`]s. Two `Match`es fed the same input log produce identical [`Match::checksum`]s.

use opcusdb_core::Fx;
use opcusdb_time::{Sim, Tick};

/// A unit (champion/minion) on the field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Unit {
    owner: u8,
    x: Fx,
    y: Fx,
    tx: Fx, // move target
    ty: Fx,
    hp: i32,
}

/// A player command for a tick. Inputs-only: this is all that crosses the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmd {
    /// Order `unit` to move to `(x, y)`.
    MoveTo { unit: u32, x: Fx, y: Fx },
    /// Order `unit` to hold position.
    Stop { unit: u32 },
}

impl Cmd {
    /// Convenience: a move order with integer coordinates.
    pub fn move_to(unit: u32, x: i32, y: i32) -> Cmd {
        Cmd::MoveTo {
            unit,
            x: Fx::from_int(x),
            y: Fx::from_int(y),
        }
    }
}

/// The deterministic match state. `Clone`/`Eq`, so it snapshots on the Timeline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    units: Vec<Unit>,
    tick: u64,
}

impl Match {
    /// A match with `players` players, each owning `per_player` units placed at
    /// deterministic starting positions. Unit ids are `0..players*per_player`.
    pub fn new(players: u8, per_player: u32) -> Self {
        let mut units = Vec::new();
        for p in 0..players {
            for u in 0..per_player {
                // Spread players along Y, units along X, fully deterministic.
                let x = Fx::from_int(10 + u as i32 * 5);
                let y = Fx::from_int(10 + p as i32 * 20);
                units.push(Unit {
                    owner: p,
                    x,
                    y,
                    tx: x,
                    ty: y,
                    hp: 100,
                });
            }
        }
        Self { units, tick: 0 }
    }

    /// Units' `(owner, x, y)` as integers, for inspection/rendering.
    pub fn units(&self) -> impl Iterator<Item = (u8, i32, i32)> + '_ {
        self.units.iter().map(|u| (u.owner, u.x.floor_int(), u.y.floor_int()))
    }

    /// A checksum of the full state, equal iff two peers are in sync.
    pub fn checksum(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut mix = |v: i64| {
            h ^= v as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        };
        mix(self.tick as i64);
        for u in &self.units {
            mix(u.x.to_bits() as i64);
            mix(u.y.to_bits() as i64);
            mix(u.tx.to_bits() as i64);
            mix(u.ty.to_bits() as i64);
            mix(u.hp as i64);
        }
        h
    }
}

impl Sim for Match {
    type Input = Cmd;

    fn step(&mut self, _tick: Tick, inputs: &[Cmd]) {
        // 1. Apply commands (set targets). Inputs are processed in order.
        for cmd in inputs {
            match *cmd {
                Cmd::MoveTo { unit, x, y } => {
                    if let Some(u) = self.units.get_mut(unit as usize) {
                        u.tx = x;
                        u.ty = y;
                    }
                }
                Cmd::Stop { unit } => {
                    if let Some(u) = self.units.get_mut(unit as usize) {
                        u.tx = u.x;
                        u.ty = u.y;
                    }
                }
            }
        }

        // 2. Move every unit toward its target by a fixed speed (deterministic).
        let speed = Fx::frac(3, 2); // 1.5 units/tick
        for u in &mut self.units {
            let dx = u.tx - u.x;
            let dy = u.ty - u.y;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist == Fx::ZERO || dist <= speed {
                u.x = u.tx; // arrived
                u.y = u.ty;
            } else {
                u.x += dx / dist * speed;
                u.y += dy / dist * speed;
            }
        }
        self.tick += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opcusdb_time::Timeline;

    // A small scripted input log: two players each order a unit somewhere.
    fn script() -> Vec<Vec<Cmd>> {
        vec![
            vec![Cmd::move_to(0, 80, 10), Cmd::move_to(2, 50, 30)],
            vec![],
            vec![Cmd::move_to(1, 20, 90)],
            vec![],
            vec![],
            vec![Cmd::Stop { unit: 0 }],
            vec![],
            vec![],
        ]
    }

    #[test]
    fn two_peers_stay_in_perfect_sync() {
        // THE lockstep guarantee: same inputs -> identical state on every peer.
        let mut peer_a = Match::new(2, 2);
        let mut peer_b = Match::new(2, 2);
        for (t, cmds) in script().iter().enumerate() {
            peer_a.step(Tick(t as u64), cmds);
            peer_b.step(Tick(t as u64), cmds);
            assert_eq!(peer_a.checksum(), peer_b.checksum(), "desync at tick {t}");
        }
    }

    #[test]
    fn units_reach_their_targets() {
        let mut m = Match::new(1, 1);
        let mut tl_input = vec![vec![Cmd::move_to(0, 90, 40)]];
        for _ in 0..200 {
            tl_input.push(vec![]);
        }
        for (t, cmds) in tl_input.iter().enumerate() {
            m.step(Tick(t as u64), cmds);
        }
        let (_, x, y) = m.units().next().unwrap();
        assert_eq!((x, y), (90, 40), "unit arrives exactly at its target");
    }

    #[test]
    fn replay_reproduces() {
        let mut tl = Timeline::new(Match::new(2, 2), 4, 8);
        for cmds in script() {
            tl.advance(cmds);
        }
        let replayed = Timeline::replay(Match::new(2, 2), tl.log());
        assert_eq!(replayed.checksum(), tl.state().checksum());
    }

    #[test]
    fn rollback_then_resim_reproduces() {
        // Late/predicted inputs: rewind and re-simulate -> identical (GGPO-style).
        let s = script();
        let mut tl = Timeline::new(Match::new(2, 2), 4, 8);
        for cmds in &s {
            tl.advance(cmds.clone());
        }
        let final_sum = tl.state().checksum();
        assert!(tl.seek(2));
        for cmds in &s[2..] {
            tl.advance(cmds.clone());
        }
        assert_eq!(tl.state().checksum(), final_sum);
    }
}
