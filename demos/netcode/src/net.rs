//! A deterministic **simulated network** plus the full **client-prediction /
//! server-reconciliation** loop (`DESIGN.md` §5/§6).
//!
//! This is the netcode loop a laggy link actually runs:
//! - the **client predicts** — it applies its own move the instant it's made, so
//!   motion feels lag-free;
//! - the **server is authoritative** — inputs arrive (delayed) over the up-link
//!   and it applies them;
//! - the **client reconciles** — each authoritative snapshot (delayed over the
//!   down-link) resets the client's confirmed state, drops acknowledged inputs,
//!   and replays the still-unacknowledged ones on top.
//!
//! Everything is deterministic (the [`Link`] uses a seeded RNG for jitter/loss),
//! so the loop is testable. [`Link`] is the seam where a real transport (QUIC
//! native / WebRTC browser) drops in later — the loop above is unchanged.

use opcusdb_core::Rng;

/// Field width/height (positions clamp to `0..SIZE`).
pub const SIZE: i32 = 1000;
/// Maximum move magnitude the server allows per axis per input.
pub const MAX_STEP: i32 = 10;

/// Apply a move under the authoritative rule (clamp step, clamp to field).
/// The client predicts with the *same* rule, so prediction matches the server.
pub fn apply_move(pos: (i32, i32), d: (i32, i32)) -> (i32, i32) {
    let dx = d.0.clamp(-MAX_STEP, MAX_STEP);
    let dy = d.1.clamp(-MAX_STEP, MAX_STEP);
    ((pos.0 + dx).clamp(0, SIZE - 1), (pos.1 + dy).clamp(0, SIZE - 1))
}

/// A one-directional link that delivers payloads after `latency` (+ up to
/// `jitter`) ticks, dropping `drop_pct`% of them. Deterministic via a seeded RNG.
pub struct Link<T> {
    latency: u64,
    jitter: u32,
    drop_pct: u32,
    rng: Rng,
    seq: u64,
    // (deliver_at, seq, payload)
    queue: Vec<(u64, u64, T)>,
}

impl<T: Clone> Link<T> {
    /// A link with the given latency (ticks), jitter (extra ticks 0..jitter),
    /// drop percentage (0..100), and RNG seed.
    pub fn new(latency: u64, jitter: u32, drop_pct: u32, seed: u64) -> Self {
        Self {
            latency,
            jitter,
            drop_pct,
            rng: Rng::seed(seed),
            seq: 0,
            queue: Vec::new(),
        }
    }

    /// Queue `payload` for delivery. Returns `false` if it was dropped.
    pub fn send(&mut self, now: u64, payload: T) -> bool {
        let seq = self.seq;
        self.seq += 1;
        if self.drop_pct > 0 && self.rng.below(100) < self.drop_pct {
            return false;
        }
        let extra = if self.jitter > 0 {
            self.rng.below(self.jitter) as u64
        } else {
            0
        };
        self.queue.push((now + self.latency + extra, seq, payload));
        true
    }

    /// Remove and return all payloads due at or before `now`, ordered by
    /// `(deliver_at, seq)` — a stable, deterministic delivery order.
    pub fn deliver(&mut self, now: u64) -> Vec<T> {
        let mut ready: Vec<(u64, u64, T)> = Vec::new();
        let mut kept: Vec<(u64, u64, T)> = Vec::new();
        for m in self.queue.drain(..) {
            if m.0 <= now {
                ready.push(m);
            } else {
                kept.push(m);
            }
        }
        self.queue = kept;
        ready.sort_by_key(|m| (m.0, m.1));
        ready.into_iter().map(|(_, _, p)| p).collect()
    }

    /// Whether any payloads are still in flight.
    pub fn in_flight(&self) -> bool {
        !self.queue.is_empty()
    }
}

/// A client move input, tagged with a sequence number for acknowledgement.
#[derive(Clone, Copy, Debug)]
pub struct Input {
    /// Monotonic client sequence number.
    pub seq: u64,
    /// The move delta.
    pub d: (i32, i32),
}

/// A server→client authoritative snapshot.
#[derive(Clone, Copy, Debug)]
pub struct Snapshot {
    /// Authoritative position.
    pub pos: (i32, i32),
    /// Highest client input sequence the server has applied.
    pub ack_seq: u64,
}

/// One client + one authoritative server connected by two simulated links.
pub struct Session {
    now: u64,
    // server
    server_pos: (i32, i32),
    server_ack: u64, // highest input seq applied
    has_ack: bool,
    // links
    up: Link<Input>,        // client -> server (reliable-ordered: no loss/jitter)
    down: Link<Snapshot>,   // server -> client (latest-wins: loss tolerated)
    // client
    confirmed_pos: (i32, i32),
    predicted_pos: (i32, i32),
    pending: Vec<Input>, // sent but not yet acknowledged
    next_seq: u64,
}

impl Session {
    /// Create a session. `up_latency` delays inputs; `down_latency`/`down_drop`
    /// delay/drop snapshots. `seed` drives the down-link's loss deterministically.
    pub fn new(up_latency: u64, down_latency: u64, down_drop: u32, seed: u64) -> Self {
        let start = (SIZE / 2, SIZE / 2);
        Self {
            now: 0,
            server_pos: start,
            server_ack: 0,
            has_ack: false,
            up: Link::new(up_latency, 0, 0, seed ^ 0xAA),
            down: Link::new(down_latency, 0, down_drop, seed ^ 0x55),
            confirmed_pos: start,
            predicted_pos: start,
            pending: Vec::new(),
            next_seq: 1,
        }
    }

    /// Advance one tick. If `client_move` is `Some`, the client issues that move
    /// this tick (predicting it immediately and sending it to the server).
    pub fn tick(&mut self, client_move: Option<(i32, i32)>) {
        self.now += 1;
        let now = self.now;

        // --- client issues + predicts -----------------------------------
        if let Some(d) = client_move {
            let inp = Input {
                seq: self.next_seq,
                d,
            };
            self.next_seq += 1;
            self.pending.push(inp);
            self.up.send(now, inp); // reliable up-link
            self.predicted_pos = apply_move(self.predicted_pos, d); // instant feedback
        }

        // --- server applies arrivals, then broadcasts a snapshot --------
        for inp in self.up.deliver(now) {
            self.server_pos = apply_move(self.server_pos, inp.d);
            self.server_ack = inp.seq;
            self.has_ack = true;
        }
        self.down.send(
            now,
            Snapshot {
                pos: self.server_pos,
                ack_seq: if self.has_ack { self.server_ack } else { 0 },
            },
        );

        // --- client reconciles against the latest snapshot --------------
        if let Some(snap) = self.down.deliver(now).into_iter().last() {
            self.confirmed_pos = snap.pos;
            self.pending.retain(|i| i.seq > snap.ack_seq);
            // Re-predict: confirmed truth + replay of still-unacked inputs.
            let mut p = self.confirmed_pos;
            for i in &self.pending {
                p = apply_move(p, i.d);
            }
            self.predicted_pos = p;
        }
    }

    /// The position the client renders (predicted, lag-free).
    pub fn predicted(&self) -> (i32, i32) {
        self.predicted_pos
    }
    /// The last server-confirmed position the client knows about.
    pub fn confirmed(&self) -> (i32, i32) {
        self.confirmed_pos
    }
    /// The authoritative server position (ground truth).
    pub fn server(&self) -> (i32, i32) {
        self.server_pos
    }
    /// Number of unacknowledged inputs in flight on the client.
    pub fn pending(&self) -> usize {
        self.pending.len()
    }
    /// Whether any messages are still in flight (for draining in tests).
    pub fn quiet(&self) -> bool {
        !self.up.in_flight() && !self.down.in_flight()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The authoritative result of applying a move script from center.
    fn canonical(moves: &[(i32, i32)]) -> (i32, i32) {
        let mut p = (SIZE / 2, SIZE / 2);
        for &d in moves {
            p = apply_move(p, d);
        }
        p
    }

    fn drain(s: &mut Session) {
        // Run empty ticks until the network is quiet and the client has caught up.
        for _ in 0..64 {
            s.tick(None);
            if s.quiet() && s.pending() == 0 {
                // one more so the final snapshot is consumed
                s.tick(None);
                break;
            }
        }
    }

    #[test]
    fn converges_under_latency_no_loss() {
        let mut s = Session::new(4, 4, 0, 1);
        let moves = [(7, 0), (7, 3), (-2, 9), (10, -10), (0, 5)];
        for &m in &moves {
            s.tick(Some(m));
        }
        drain(&mut s);
        let truth = canonical(&moves);
        assert_eq!(s.server(), truth);
        assert_eq!(s.confirmed(), truth, "client confirmed catches up to server");
        assert_eq!(s.predicted(), truth, "no pending left, prediction == truth");
        assert_eq!(s.pending(), 0);
    }

    #[test]
    fn client_prediction_is_instant() {
        // With up-latency, the server hasn't seen the move yet, but the client's
        // predicted position already reflects it on the issuing tick.
        let mut s = Session::new(5, 5, 0, 2);
        let start = s.predicted();
        s.tick(Some((MAX_STEP, 0)));
        assert_eq!(s.predicted(), apply_move(start, (MAX_STEP, 0)), "instant");
        assert_eq!(s.server(), start, "server hasn't received it yet");
        assert!(s.pending() >= 1, "the input is in flight, unacknowledged");
    }

    #[test]
    fn converges_despite_snapshot_loss() {
        // Heavy down-link loss: the client just waits for the next snapshot and
        // still converges to authoritative truth.
        let mut s = Session::new(3, 3, 60, 7);
        let moves = [(6, 6), (9, -4), (-10, 2), (3, 8)];
        for &m in &moves {
            s.tick(Some(m));
        }
        drain(&mut s);
        assert_eq!(s.server(), canonical(&moves));
        assert_eq!(s.confirmed(), s.server(), "reconciles to truth despite drops");
    }

    #[test]
    fn deterministic() {
        let run = || {
            let mut s = Session::new(3, 4, 30, 99);
            for i in 0..20 {
                s.tick(Some((i % 7 - 3, 5 - i % 5)));
            }
            drain(&mut s);
            (s.server(), s.confirmed(), s.predicted())
        };
        assert_eq!(run(), run());
    }
}
