//! An interactive **particle field** on the ECS [`World`] — thousands of
//! particles that are attracted to (or repelled from) a moving point, with an
//! orbital swirl that turns the cursor into the center of a little galaxy.
//!
//! All physics is **fixed-point integer** (positions/velocities scaled by
//! `FP_ONE`), so the sim stays deterministic (no floats — determinism contract
//! §2) while still feeling fluid. It's exposed to the browser via the FFI crate;
//! JS only feeds the pointer position and renders.

use opcusdb_core::{Rng, World};

/// Fixed-point fractional bits (1.0 == `1 << FP_SHIFT`).
const FP_SHIFT: i32 = 8;
/// Fixed-point one.
const FP_ONE: i64 = 1 << FP_SHIFT;

/// Per-axis velocity clamp (px/tick).
const MAX_V: i64 = 14 * FP_ONE;
/// Velocity retained per tick (out of FP_ONE) — light damping.
const DAMP: i64 = 250;
/// Constant pull toward the attractor along the unit direction.
const GRAV: i64 = 40;
/// Distance-proportional gather strength (right-shift on the delta).
const SPRING_SHIFT: i64 = 12;
/// Tangential (orbit) strength.
const SWIRL: i64 = 26;

/// Interaction mode for the attractor.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// No force.
    Off,
    /// Pull particles toward the point.
    Attract,
    /// Push particles away from the point.
    Repel,
}

#[derive(Clone, Copy)]
struct Pos {
    x: i64,
    y: i64,
}
#[derive(Clone, Copy)]
struct Vel {
    x: i64,
    y: i64,
}

/// Integer square root (Newton's method) for fixed-point magnitudes.
fn isqrt(n: i64) -> i64 {
    if n <= 0 {
        return 0;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

/// The particle field.
pub struct Field {
    world: World,
    count: u32,
    /// Field bounds in pixels (positions wrap within these).
    w: i64,
    h: i64,
    attractor: (i64, i64), // fixed-point
    mode: Mode,
    /// Pixel positions `[x0, y0, ...]` refreshed each step for the renderer.
    pixels: Vec<i32>,
}

impl Field {
    /// Create `n` particles in a `width × height` field, seeded by `seed`.
    pub fn new(n: u32, seed: u64, width: i32, height: i32) -> Self {
        let w = width.max(1) as i64;
        let h = height.max(1) as i64;
        let mut world = World::new();
        let mut rng = Rng::seed(seed);
        for _ in 0..n {
            let e = world.spawn();
            world.insert(
                e,
                Pos {
                    x: (rng.range(0, w as u32) as i64) << FP_SHIFT,
                    y: (rng.range(0, h as u32) as i64) << FP_SHIFT,
                },
            );
            world.insert(
                e,
                Vel {
                    x: (rng.range(0, 5) as i64 - 2) << FP_SHIFT,
                    y: (rng.range(0, 5) as i64 - 2) << FP_SHIFT,
                },
            );
        }
        let mut f = Self {
            world,
            count: n,
            w,
            h,
            attractor: ((w << FP_SHIFT) / 2, (h << FP_SHIFT) / 2),
            mode: Mode::Off,
            pixels: Vec::with_capacity(n as usize * 2),
        };
        f.refresh();
        f
    }

    /// Number of particles.
    pub fn len(&self) -> u32 {
        self.count
    }

    /// Whether the field is empty.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Point the attractor at pixel `(x, y)` with the given `mode`.
    pub fn set_attractor(&mut self, x_px: i32, y_px: i32, mode: Mode) {
        self.attractor = ((x_px as i64) << FP_SHIFT, (y_px as i64) << FP_SHIFT);
        self.mode = mode;
    }

    /// Advance one tick.
    pub fn step(&mut self) {
        let (ax, ay) = self.attractor;
        let mode = self.mode;
        let wfp = self.w << FP_SHIFT;
        let hfp = self.h << FP_SHIFT;

        for id in self.world.matching::<(Pos, Vel)>() {
            let p = *self.world.get::<Pos>(id).expect("has Pos");
            let mut v = *self.world.get::<Vel>(id).expect("has Vel");

            if mode != Mode::Off {
                let dx = ax - p.x;
                let dy = ay - p.y;
                let len = isqrt(dx * dx + dy * dy).max(FP_ONE);
                // Unit direction in fixed-point.
                let ux = (dx << FP_SHIFT) / len;
                let uy = (dy << FP_SHIFT) / len;
                let sign = if mode == Mode::Attract { 1 } else { -1 };

                // Constant pull + distance-proportional gather, along the direction.
                v.x += sign * ((ux * GRAV) >> FP_SHIFT) + sign * (dx >> SPRING_SHIFT);
                v.y += sign * ((uy * GRAV) >> FP_SHIFT) + sign * (dy >> SPRING_SHIFT);
                // Tangential swirl (perpendicular to the direction) -> orbits.
                v.x += (-uy * SWIRL) >> FP_SHIFT;
                v.y += (ux * SWIRL) >> FP_SHIFT;
            }

            // Clamp, damp, integrate.
            v.x = v.x.clamp(-MAX_V, MAX_V);
            v.y = v.y.clamp(-MAX_V, MAX_V);
            v.x = (v.x * DAMP) >> FP_SHIFT;
            v.y = (v.y * DAMP) >> FP_SHIFT;

            let mut np = Pos {
                x: p.x + v.x,
                y: p.y + v.y,
            };
            np.x = np.x.rem_euclid(wfp);
            np.y = np.y.rem_euclid(hfp);

            *self.world.get_mut::<Pos>(id).expect("has Pos") = np;
            *self.world.get_mut::<Vel>(id).expect("has Vel") = v;
        }
        self.refresh();
    }

    fn refresh(&mut self) {
        self.pixels.clear();
        // matching() gives ascending entity order -> deterministic buffer.
        for id in self.world.matching::<(Pos,)>() {
            let p = self.world.get::<Pos>(id).expect("has Pos");
            self.pixels.push((p.x >> FP_SHIFT) as i32);
            self.pixels.push((p.y >> FP_SHIFT) as i32);
        }
    }

    /// Pixel positions `[x0, y0, x1, y1, ...]` (length `2 * len`).
    pub fn pixels(&self) -> &[i32] {
        &self.pixels
    }

    /// Mean distance (pixels) of all particles to a pixel point — a test/inspection helper.
    pub fn mean_distance_to(&self, x_px: i32, y_px: i32) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        let mut total = 0.0;
        for chunk in self.pixels.chunks_exact(2) {
            let dx = (chunk[0] - x_px) as f64;
            let dy = (chunk[1] - y_px) as f64;
            total += (dx * dx + dy * dy).sqrt();
        }
        total / self.count as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_same_seed() {
        let run = || {
            let mut f = Field::new(2_000, 7, 800, 800);
            f.set_attractor(400, 400, Mode::Attract);
            for _ in 0..60 {
                f.step();
            }
            f.pixels().to_vec()
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn attract_gathers_particles() {
        let mut f = Field::new(3_000, 1, 800, 800);
        let before = f.mean_distance_to(400, 400);
        f.set_attractor(400, 400, Mode::Attract);
        for _ in 0..120 {
            f.step();
        }
        let after = f.mean_distance_to(400, 400);
        assert!(after < before, "attract should pull inward: {before} -> {after}");
    }

    #[test]
    fn repel_scatters_relative_to_attract() {
        // From the same start, repel keeps particles further out than attract.
        let mk = || Field::new(3_000, 5, 800, 800);
        let mut att = mk();
        let mut rep = mk();
        att.set_attractor(400, 400, Mode::Attract);
        rep.set_attractor(400, 400, Mode::Repel);
        for _ in 0..60 {
            att.step();
            rep.step();
        }
        assert!(rep.mean_distance_to(400, 400) > att.mean_distance_to(400, 400));
    }

    #[test]
    fn positions_stay_in_bounds() {
        let mut f = Field::new(1_000, 9, 600, 400);
        f.set_attractor(0, 0, Mode::Repel);
        for _ in 0..100 {
            f.step();
        }
        for chunk in f.pixels().chunks_exact(2) {
            assert!((0..600).contains(&chunk[0]) && (0..400).contains(&chunk[1]));
        }
    }
}
