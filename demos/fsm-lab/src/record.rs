//! Record / replay / scrub for the intersection scene (`CORE_SPEC.md` §12 CLI).
//!
//! A *record* is the seed + a per-tick golden trace of the observable state.
//! *Replay* re-runs the sim from the seed and asserts every frame reproduces the
//! stored trace byte-for-byte, a determinism check you can run against a file.
//! The format is a tiny std-only text format (no external serializer); the
//! framework-wide rkyv-vs-bitcode decision (§14 Q1) is reserved for World
//! snapshots, which actually need it.

use crate::{Intersection, Light};
use opcusdb_time::Timeline;

const HEADER: &str = "opcusdb-fsm-lab v1";

/// One tick of observable intersection state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    /// Tick number.
    pub tick: u64,
    /// North-south light.
    pub ns: Light,
    /// East-west light.
    pub ew: Light,
    /// Pedestrian walk active.
    pub walk: bool,
    /// North-south queue length.
    pub qns: u32,
    /// East-west queue length.
    pub qew: u32,
}

/// A recorded run: the inputs needed to reproduce it, plus the golden trace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    /// RNG seed for car arrivals.
    pub seed: u64,
    /// Number of ticks recorded.
    pub ticks: u64,
    /// Per-tick observable state.
    pub trace: Vec<Frame>,
}

fn light_str(l: Light) -> &'static str {
    match l {
        Light::Red => "R",
        Light::Green => "G",
        Light::Yellow => "Y",
    }
}

fn light_parse(s: &str) -> Option<Light> {
    match s {
        "R" => Some(Light::Red),
        "G" => Some(Light::Green),
        "Y" => Some(Light::Yellow),
        _ => None,
    }
}

/// Run the sim and capture its golden trace.
pub fn capture(seed: u64, ticks: u64) -> Record {
    let mut tl = Timeline::new(Intersection::new_seeded(seed), 8, 4);
    let mut trace = Vec::with_capacity(ticks as usize);
    for _ in 0..ticks {
        tl.advance(vec![]);
        let s = tl.state();
        let c = s.ctx();
        let t = s.traffic();
        trace.push(Frame {
            tick: tl.tick().get(),
            ns: c.ns,
            ew: c.ew,
            walk: c.walk,
            qns: t.ns.waiting,
            qew: t.ew.waiting,
        });
    }
    Record { seed, ticks, trace }
}

impl Record {
    /// Serialize to the text format.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str(HEADER);
        out.push('\n');
        out.push_str(&format!("seed {}\n", self.seed));
        out.push_str(&format!("ticks {}\n", self.ticks));
        out.push_str("trace\n");
        for f in &self.trace {
            out.push_str(&format!(
                "{} {} {} {} {} {}\n",
                f.tick,
                light_str(f.ns),
                light_str(f.ew),
                u8::from(f.walk),
                f.qns,
                f.qew
            ));
        }
        out
    }

    /// Parse the text format. Returns a human-readable error on malformed input.
    pub fn parse(text: &str) -> Result<Record, String> {
        let mut lines = text.lines();
        if lines.next() != Some(HEADER) {
            return Err(format!("bad header (expected `{HEADER}`)"));
        }
        let seed = parse_kv(lines.next(), "seed")?;
        let ticks = parse_kv(lines.next(), "ticks")?;
        if lines.next() != Some("trace") {
            return Err("missing `trace` marker".into());
        }
        let mut trace = Vec::new();
        for (i, line) in lines.enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let p: Vec<&str> = line.split_whitespace().collect();
            if p.len() != 6 {
                return Err(format!("frame {i}: expected 6 fields, got {}", p.len()));
            }
            let parse_u = |s: &str, what: &str| s.parse().map_err(|_| format!("frame {i}: bad {what}"));
            trace.push(Frame {
                tick: parse_u(p[0], "tick")?,
                ns: light_parse(p[1]).ok_or(format!("frame {i}: bad ns light"))?,
                ew: light_parse(p[2]).ok_or(format!("frame {i}: bad ew light"))?,
                walk: parse_u(p[3], "walk")? != 0u64,
                qns: parse_u(p[4], "qns")? as u32,
                qew: parse_u(p[5], "qew")? as u32,
            });
        }
        Ok(Record { seed, ticks, trace })
    }

    /// Re-run from the seed and assert the recomputed trace matches the stored
    /// one frame-for-frame. `Ok(n)` reports the number of frames verified.
    pub fn verify(&self) -> Result<u64, String> {
        let fresh = capture(self.seed, self.ticks);
        for (i, (stored, got)) in self.trace.iter().zip(&fresh.trace).enumerate() {
            if stored != got {
                return Err(format!(
                    "determinism broken at frame {i}: stored {stored:?} != recomputed {got:?}"
                ));
            }
        }
        if self.trace.len() != fresh.trace.len() {
            return Err(format!(
                "length mismatch: stored {} vs recomputed {}",
                self.trace.len(),
                fresh.trace.len()
            ));
        }
        Ok(self.trace.len() as u64)
    }
}

fn parse_kv(line: Option<&str>, key: &str) -> Result<u64, String> {
    let line = line.ok_or(format!("missing `{key}` line"))?;
    let rest = line
        .strip_prefix(key)
        .and_then(|s| s.strip_prefix(' '))
        .ok_or(format!("expected `{key} <value>`"))?;
    rest.trim().parse().map_err(|_| format!("bad `{key}` value"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_text() {
        let rec = capture(7, 14);
        let text = rec.to_text();
        let parsed = Record::parse(&text).expect("parses");
        assert_eq!(parsed, rec);
    }

    #[test]
    fn verify_succeeds_for_genuine_record() {
        let rec = capture(123, 40);
        assert_eq!(rec.verify().unwrap(), 40);
    }

    #[test]
    fn verify_detects_tampering() {
        let mut rec = capture(5, 20);
        // Corrupt a frame: replay must catch it.
        rec.trace[10].qns = rec.trace[10].qns.wrapping_add(99);
        assert!(rec.verify().is_err());
    }

    #[test]
    fn parse_rejects_bad_header() {
        assert!(Record::parse("nope\nseed 1\nticks 0\ntrace\n").is_err());
    }
}
