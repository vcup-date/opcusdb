//! opcusdb Overlode — an Overwatch-style team FPS (hero: **Tracer**), authoritative
//! over the hand-rolled WebSocket. Demonstrates the engine's netcode model:
//! a 60 Hz authoritative server with **lag-compensated hitscan** (it rewinds
//! targets into the shooter's view for fair hits) and **Recall** (rewind your own
//! position/health/ammo 3s — opcusdb's timeline, as a hero ability).
//!
//! Humans are team A; **AI bots** fill team B so you can test solo. Three.js client
//! in `demos/server/web/ow.{html,js}`.
//!
//! Run: `cargo run -p opcusdb-server --bin opcusdb-ow` then open http://localhost:9008

use opcusdb_core::Rng;
use opcusdb_server::ws;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const PORT: u16 = 9008;
const TICK_MS: u64 = 16;
const DT: f32 = 0.016;
const ARENA: f32 = 28.0;
const EYE: f32 = 1.55;
const P_RADIUS: f32 = 0.45;

const GRAVITY: f32 = 24.0;
const MOVE: f32 = 7.0;
const JUMP_V: f32 = 8.2;

const MAX_HP: f32 = 150.0;
const MAG: i32 = 20;
const FIRE_CD: f32 = 0.11;
const RELOAD_T: f32 = 1.0;
const DMG_NEAR: f32 = 6.0;
const DMG_FAR: f32 = 3.0;
const RANGE: f32 = 60.0;
const HEAD_MULT: f32 = 2.0;

const BLINK_DIST: f32 = 7.0;
const BLINK_MAX: i32 = 3;
const BLINK_RECHARGE: f32 = 3.0;
const RECALL_CD: f32 = 12.0;
const RECALL_SECS: f32 = 3.0;
const RESPAWN: f32 = 2.5;

const NUM_BOTS: usize = 3;
const HIST: usize = 70; // lag-comp position history (~1.1s)
const RECALL_HIST: usize = 200; // ~3.2s of self-state for Recall

// health pickups: positions (must match the client), heal amount, respawn cd
const PACKS: [(f32, f32); 4] = [(15.0, 0.0), (-15.0, 0.0), (0.0, 18.0), (0.0, -18.0)];
const PACK_HEAL: f32 = 75.0;
const PACK_CD: f32 = 10.0;
const PACK_R: f32 = 1.8;

// ultimate: Pulse Bomb (charge by dealing damage)
const ULT_MAX: f32 = 600.0;
const PB_SPEED: f32 = 24.0;
const PB_GRAV: f32 = 16.0;
const PB_FUSE: f32 = 1.3;
const PB_RADIUS: f32 = 5.0;
const PB_DMG: f32 = 350.0;
const PB_DMG_MIN: f32 = 130.0;
const SCORE_WIN: u32 = 25; // elims for a team to win a round
const INTERMISSION: f32 = 6.0;

// cover boxes: (cx, cz, half_x, half_z, height)
const COVER: [(f32, f32, f32, f32, f32); 7] = [
    (0.0, 0.0, 2.0, 2.0, 2.4),
    (10.0, 8.0, 1.5, 1.5, 2.0),
    (-10.0, 8.0, 1.5, 1.5, 2.0),
    (10.0, -8.0, 1.5, 1.5, 2.0),
    (-10.0, -8.0, 1.5, 1.5, 2.0),
    (0.0, 14.0, 4.0, 1.0, 3.0),
    (0.0, -14.0, 4.0, 1.0, 3.0),
];

#[derive(Clone, Copy, Default)]
struct V3 {
    x: f32,
    y: f32,
    z: f32,
}
impl V3 {
    fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
    fn sub(self, o: V3) -> V3 {
        V3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
    fn add(self, o: V3) -> V3 {
        V3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
    fn scale(self, s: f32) -> V3 {
        V3::new(self.x * s, self.y * s, self.z * s)
    }
    fn dot(self, o: V3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    fn len(self) -> f32 {
        self.dot(self).sqrt()
    }
}

/// look direction from yaw/pitch (yaw around Y, 0 = -Z forward)
fn dir_from(yaw: f32, pitch: f32) -> V3 {
    let cp = pitch.cos();
    V3::new(-yaw.sin() * cp, pitch.sin(), -yaw.cos() * cp)
}

struct Player {
    name: String,
    bot: bool,
    team: u8,
    pos: V3, // feet
    vel: V3,
    yaw: f32,
    pitch: f32,
    hp: f32,
    ammo: i32,
    fire_cd: f32,
    reload_t: f32,
    blink: i32,
    blink_recharge: f32,
    recall_cd: f32,
    on_ground: bool,
    alive: bool,
    respawn: f32,
    elims: u32,
    deaths: u32,
    ult: f32, // ultimate charge (0..ULT_MAX)
    lat: f32, // shooter one-way latency (s)
    // inputs
    inf: bool,
    inb: bool,
    inl: bool,
    inr: bool,
    inj: bool,
    firing: bool,
    // histories
    hist: Vec<(f32, V3)>,                       // (time, eye pos) for lag comp
    self_hist: Vec<(V3, f32, i32)>,             // (pos, hp, ammo) for Recall
    // bot ai
    ai_t: f32,
    ai_strafe: f32,
}

impl Player {
    fn new(name: String, bot: bool, team: u8, pos: V3) -> Self {
        Self {
            name,
            bot,
            team,
            pos,
            vel: V3::default(),
            yaw: 0.0,
            pitch: 0.0,
            hp: MAX_HP,
            ammo: MAG,
            fire_cd: 0.0,
            reload_t: 0.0,
            blink: BLINK_MAX,
            blink_recharge: 0.0,
            recall_cd: 0.0,
            on_ground: false,
            alive: true,
            respawn: 0.0,
            elims: 0,
            deaths: 0,
            ult: 0.0,
            lat: 0.0,
            inf: false,
            inb: false,
            inl: false,
            inr: false,
            inj: false,
            firing: false,
            hist: Vec::new(),
            self_hist: Vec::new(),
            ai_t: 0.0,
            ai_strafe: 1.0,
        }
    }
    fn eye(&self) -> V3 {
        V3::new(self.pos.x, self.pos.y + EYE, self.pos.z)
    }
}

struct PulseBomb {
    pos: V3,
    vel: V3,
    fuse: f32,
    owner: u32,
    team: u8,
}

struct Match {
    players: BTreeMap<u32, Player>,
    bombs: Vec<PulseBomb>,
    events: Vec<String>,
    rng: Rng,
    time: f32,
    score: [u32; 2],
    winner: u8, // 0 none, 1 team A, 2 team B
    intermission: f32,
    pack_cd: Vec<f32>,
    snapshot: String,
    next_bot: u32,
}

struct Server {
    matches: BTreeMap<String, Match>,
    next_id: u32,
}

fn spawn_point(rng: &mut Rng, team: u8) -> V3 {
    let z = if team == 0 { 22.0 } else { -22.0 };
    let x = (rng.below(360) as f32) / 10.0 - 18.0;
    V3::new(x, 0.0, z)
}

fn main() {
    let server = Arc::new(Mutex::new(Server { matches: BTreeMap::new(), next_id: 1 }));
    {
        let server = server.clone();
        thread::spawn(move || loop {
            thread::sleep(Duration::from_millis(TICK_MS));
            let mut s = server.lock().unwrap();
            let codes: Vec<String> = s.matches.keys().cloned().collect();
            for code in codes {
                let mut m = s.matches.remove(&code).unwrap();
                tick(&mut m);
                m.snapshot = build_snapshot(&m);
                m.events.clear();
                // keep the match alive while any human is present
                if m.players.values().any(|p| !p.bot) {
                    s.matches.insert(code, m);
                }
            }
        });
    }
    let listener = TcpListener::bind(("0.0.0.0", PORT)).expect("bind");
    println!("opcusdb Overlode (Tracer FPS) on http://localhost:{PORT}");
    for stream in listener.incoming().flatten() {
        let server = server.clone();
        thread::spawn(move || handle(stream, server));
    }
}

fn new_match(seed: u64) -> Match {
    let mut m = Match {
        players: BTreeMap::new(),
        bombs: Vec::new(),
        events: Vec::new(),
        rng: Rng::seed(seed),
        time: 0.0,
        score: [0, 0],
        winner: 0,
        intermission: 0.0,
        pack_cd: vec![0.0; PACKS.len()],
        snapshot: String::new(),
        next_bot: 100000,
    };
    for _ in 0..NUM_BOTS {
        let id = m.next_bot;
        m.next_bot += 1;
        let sp = spawn_point(&mut m.rng, 1);
        m.players.insert(id, Player::new(format!("Bot-{}", id - 99999), true, 1, sp));
    }
    m
}

// --- simulation ------------------------------------------------------------

fn tick(m: &mut Match) {
    m.time += DT;
    bot_ai(m);
    let ids: Vec<u32> = m.players.keys().copied().collect();
    for id in &ids {
        step_player(m, *id);
    }
    // shooting (after movement so aim/pos are current)
    for id in &ids {
        try_fire(m, *id);
    }
    update_bombs(m);
    check_packs(m);
    check_match(m);
    // record histories
    let t = m.time;
    for p in m.players.values_mut() {
        p.hist.push((t, p.eye()));
        if p.hist.len() > HIST {
            p.hist.remove(0);
        }
        if p.alive {
            p.self_hist.push((p.pos, p.hp, p.ammo));
            if p.self_hist.len() > RECALL_HIST {
                p.self_hist.remove(0);
            }
        }
    }
}

fn step_player(m: &mut Match, id: u32) {
    let respawn_to;
    {
        let p = m.players.get_mut(&id).unwrap();
        if !p.alive {
            p.respawn -= DT;
            if p.respawn > 0.0 {
                return;
            }
            p.alive = true;
            p.hp = MAX_HP;
            p.ammo = MAG;
            p.blink = BLINK_MAX;
            p.self_hist.clear();
            respawn_to = Some(p.team);
        } else {
            respawn_to = None;
        }
        // cooldowns
        if p.fire_cd > 0.0 {
            p.fire_cd -= DT;
        }
        if p.recall_cd > 0.0 {
            p.recall_cd -= DT;
        }
        if p.blink < BLINK_MAX {
            p.blink_recharge -= DT;
            if p.blink_recharge <= 0.0 {
                p.blink += 1;
                p.blink_recharge = BLINK_RECHARGE;
            }
        }
        if p.reload_t > 0.0 {
            p.reload_t -= DT;
            if p.reload_t <= 0.0 {
                p.ammo = MAG;
            }
        }
    }
    if let Some(team) = respawn_to {
        let sp = spawn_point(&mut m.rng, team);
        let p = m.players.get_mut(&id).unwrap();
        p.pos = sp;
        p.vel = V3::default();
        return;
    }

    let p = m.players.get_mut(&id).unwrap();
    // horizontal movement relative to yaw
    let (sy, cy) = (p.yaw.sin(), p.yaw.cos());
    let fwd = V3::new(-sy, 0.0, -cy);
    let right = V3::new(cy, 0.0, -sy);
    let mut wish = V3::default();
    if p.inf {
        wish = wish.add(fwd);
    }
    if p.inb {
        wish = wish.sub(fwd);
    }
    if p.inr {
        wish = wish.add(right);
    }
    if p.inl {
        wish = wish.sub(right);
    }
    let wl = wish.len();
    if wl > 0.0 {
        wish = wish.scale(1.0 / wl);
    }
    p.vel.x = wish.x * MOVE;
    p.vel.z = wish.z * MOVE;
    if p.inj && p.on_ground {
        p.vel.y = JUMP_V;
        p.on_ground = false;
    }
    p.vel.y -= GRAVITY * DT;
    p.pos = p.pos.add(p.vel.scale(DT));
    // ground
    if p.pos.y <= 0.0 {
        p.pos.y = 0.0;
        p.vel.y = 0.0;
        p.on_ground = true;
    }
    // arena bounds
    p.pos.x = p.pos.x.clamp(-ARENA, ARENA);
    p.pos.z = p.pos.z.clamp(-ARENA, ARENA);
    // resolve cover (XZ circle vs AABB)
    for (cx, cz, hx, hz, _h) in COVER {
        let nx = p.pos.x.clamp(cx - hx, cx + hx);
        let nz = p.pos.z.clamp(cz - hz, cz + hz);
        let (dx, dz) = (p.pos.x - nx, p.pos.z - nz);
        let d = (dx * dx + dz * dz).sqrt();
        if d < P_RADIUS {
            if d > 0.0001 {
                let push = P_RADIUS - d;
                p.pos.x += dx / d * push;
                p.pos.z += dz / d * push;
            } else {
                p.pos.x = cx + hx + P_RADIUS; // degenerate: shove out +x
            }
        }
    }
}

/// Cover blocks line of sight between two points (ray vs AABB before target).
fn blocked(a: V3, b: V3) -> bool {
    let d = b.sub(a);
    let len = d.len();
    if len < 0.001 {
        return false;
    }
    let dir = d.scale(1.0 / len);
    for (cx, cz, hx, hz, h) in COVER {
        let (mn, mx) = (V3::new(cx - hx, 0.0, cz - hz), V3::new(cx + hx, h, cz + hz));
        if let Some(t) = ray_aabb(a, dir, mn, mx) {
            if t > 0.05 && t < len - 0.05 {
                return true;
            }
        }
    }
    false
}

fn ray_aabb(o: V3, d: V3, mn: V3, mx: V3) -> Option<f32> {
    let inv = V3::new(1.0 / d.x, 1.0 / d.y, 1.0 / d.z);
    let t1 = (mn.x - o.x) * inv.x;
    let t2 = (mx.x - o.x) * inv.x;
    let t3 = (mn.y - o.y) * inv.y;
    let t4 = (mx.y - o.y) * inv.y;
    let t5 = (mn.z - o.z) * inv.z;
    let t6 = (mx.z - o.z) * inv.z;
    let tmin = t1.min(t2).max(t3.min(t4)).max(t5.min(t6));
    let tmax = t1.max(t2).min(t3.max(t4)).min(t5.max(t6));
    if tmax < 0.0 || tmin > tmax {
        None
    } else {
        Some(tmin)
    }
}

fn try_fire(m: &mut Match, id: u32) {
    let (eye, dir, team, lat, can) = {
        let p = match m.players.get(&id) {
            Some(p) => p,
            None => return,
        };
        let can = p.alive && p.firing && p.fire_cd <= 0.0 && p.reload_t <= 0.0 && p.ammo > 0;
        (p.eye(), dir_from(p.yaw, p.pitch), p.team, p.lat, can)
    };
    if !can {
        return;
    }
    {
        let p = m.players.get_mut(&id).unwrap();
        p.ammo -= 1;
        p.fire_cd = FIRE_CD;
        if p.ammo == 0 {
            p.reload_t = RELOAD_T;
        }
    }
    // lag compensation: rewind targets to the shooter's view (~interp + latency)
    let rewind = 0.1 + lat.clamp(0.0, 0.25);
    let aim_t = m.time - rewind;
    let mut best: Option<(u32, f32, bool)> = None; // (target, dist, head)
    let shooter_id = id;
    for (tid, t) in m.players.iter() {
        if *tid == shooter_id || !t.alive || t.team == team {
            continue;
        }
        let tpos = hist_at(&t.hist, aim_t).unwrap_or_else(|| t.eye());
        // body sphere at chest, head sphere above
        let chest = V3::new(tpos.x, tpos.y - 0.25, tpos.z);
        let head = V3::new(tpos.x, tpos.y + 0.35, tpos.z);
        for (center, r, is_head) in [(chest, 0.55, false), (head, 0.32, true)] {
            let rel = center.sub(eye);
            let along = rel.dot(dir);
            if along <= 0.0 || along > RANGE {
                continue;
            }
            let perp = rel.sub(dir.scale(along)).len();
            if perp <= r {
                let better = best.map_or(true, |(_, bd, _)| along < bd);
                if better && !blocked(eye, center) {
                    best = Some((*tid, along, is_head));
                }
            }
        }
    }
    // tracer beam for everyone
    let end = eye.add(dir.scale(best.map_or(RANGE, |(_, d, _)| d)));
    m.events.push(format!("t:{:.2}:{:.2}:{:.2}:{:.2}:{:.2}:{:.2}", eye.x, eye.y, eye.z, end.x, end.y, end.z));
    if let Some((tid, dist, head)) = best {
        let dmg = (DMG_NEAR - (DMG_NEAR - DMG_FAR) * (dist / RANGE).min(1.0)) * if head { HEAD_MULT } else { 1.0 };
        let dead = {
            let t = m.players.get_mut(&tid).unwrap();
            t.hp -= dmg;
            t.hp <= 0.0
        };
        if let Some(k) = m.players.get_mut(&shooter_id) {
            k.ult = (k.ult + dmg).min(ULT_MAX);
        }
        m.events.push(format!("h:{shooter_id}:{:.2}:{:.2}:{:.2}:{:.0}", end.x, end.y, end.z, dmg));
        if dead {
            {
                let t = m.players.get_mut(&tid).unwrap();
                t.alive = false;
                t.respawn = RESPAWN;
                t.deaths += 1;
            }
            let vteam = m.players[&tid].team;
            if let Some(k) = m.players.get_mut(&shooter_id) {
                k.elims += 1;
            }
            m.score[team as usize] += 1;
            let _ = vteam;
            m.events.push(format!("k:{shooter_id}:{tid}"));
        }
    }
}

/// Throw the Pulse Bomb ultimate if fully charged.
fn do_ult(m: &mut Match, id: u32) {
    let (eye, dir, team, ready) = match m.players.get(&id) {
        Some(p) => (p.eye(), dir_from(p.yaw, p.pitch), p.team, p.alive && p.ult >= ULT_MAX),
        None => return,
    };
    if !ready {
        return;
    }
    if let Some(p) = m.players.get_mut(&id) {
        p.ult = 0.0;
    }
    let vel = dir.scale(PB_SPEED).add(V3::new(0.0, 3.0, 0.0));
    m.bombs.push(PulseBomb { pos: eye, vel, fuse: PB_FUSE, owner: id, team });
    m.events.push(format!("u:{id}"));
}

/// Simulate thrown Pulse Bombs and resolve their AoE explosions.
fn update_bombs(m: &mut Match) {
    let mut explode: Vec<(V3, u32, u8)> = Vec::new();
    let mut keep: Vec<PulseBomb> = Vec::new();
    for mut bz in std::mem::take(&mut m.bombs) {
        bz.vel.y -= PB_GRAV * DT;
        bz.pos = bz.pos.add(bz.vel.scale(DT));
        bz.fuse -= DT;
        let mut boom = bz.fuse <= 0.0 || bz.pos.y <= 0.25;
        for (cx, cz, hx, hz, h) in COVER {
            if bz.pos.x > cx - hx && bz.pos.x < cx + hx && bz.pos.z > cz - hz && bz.pos.z < cz + hz && bz.pos.y < h {
                boom = true;
            }
        }
        for t in m.players.values() {
            if t.alive && t.team != bz.team && t.eye().sub(bz.pos).len() < 1.4 {
                boom = true;
            }
        }
        if boom {
            explode.push((bz.pos, bz.owner, bz.team));
        } else {
            keep.push(bz);
        }
    }
    m.bombs = keep;
    for (pos, owner, team) in explode {
        m.events.push(format!("X:{:.2}:{:.2}:{:.2}", pos.x, pos.y, pos.z));
        let tids: Vec<u32> = m.players.iter().filter(|(_, t)| t.alive && t.team != team).map(|(id, _)| *id).collect();
        for tid in tids {
            let tpos = m.players[&tid].pos;
            let d = V3::new(tpos.x, tpos.y + 0.9, tpos.z).sub(pos).len();
            if d < PB_RADIUS {
                let dmg = PB_DMG - (PB_DMG - PB_DMG_MIN) * (d / PB_RADIUS);
                let dead = {
                    let t = m.players.get_mut(&tid).unwrap();
                    t.hp -= dmg;
                    t.hp <= 0.0
                };
                m.events.push(format!("h:{owner}:{:.2}:{:.2}:{:.2}:{:.0}", tpos.x, tpos.y + 1.0, tpos.z, dmg));
                if dead {
                    {
                        let t = m.players.get_mut(&tid).unwrap();
                        t.alive = false;
                        t.respawn = RESPAWN;
                        t.deaths += 1;
                    }
                    if let Some(k) = m.players.get_mut(&owner) {
                        k.elims += 1;
                    }
                    m.score[team as usize] += 1;
                    m.events.push(format!("k:{owner}:{tid}"));
                }
            }
        }
    }
}

/// Round flow: declare a winner at the score target, then reset after intermission.
fn check_match(m: &mut Match) {
    if m.winner == 0 {
        for t in 0..2 {
            if m.score[t] >= SCORE_WIN {
                m.winner = (t + 1) as u8;
                m.intermission = INTERMISSION;
                m.events.push(format!("V:{}", t + 1));
            }
        }
    } else {
        m.intermission -= DT;
        if m.intermission <= 0.0 {
            reset_match(m);
        }
    }
}

fn reset_match(m: &mut Match) {
    m.score = [0, 0];
    m.winner = 0;
    m.bombs.clear();
    let teams: Vec<(u32, u8)> = m.players.iter().map(|(id, p)| (*id, p.team)).collect();
    for (id, team) in teams {
        let sp = spawn_point(&mut m.rng, team);
        let p = m.players.get_mut(&id).unwrap();
        p.pos = sp;
        p.vel = V3::default();
        p.hp = MAX_HP;
        p.alive = true;
        p.ammo = MAG;
        p.blink = BLINK_MAX;
        p.ult = 0.0;
        p.respawn = 0.0;
        p.self_hist.clear();
    }
}

fn check_packs(m: &mut Match) {
    for c in m.pack_cd.iter_mut() {
        if *c > 0.0 {
            *c = (*c - DT).max(0.0);
        }
    }
    let ids: Vec<u32> = m.players.keys().copied().collect();
    for (i, (px, pz)) in PACKS.iter().enumerate() {
        if m.pack_cd[i] > 0.0 {
            continue;
        }
        for id in &ids {
            let p = m.players.get_mut(id).unwrap();
            if !p.alive || p.hp >= MAX_HP {
                continue;
            }
            if ((p.pos.x - px).powi(2) + (p.pos.z - pz).powi(2)).sqrt() < PACK_R {
                p.hp = (p.hp + PACK_HEAL).min(MAX_HP);
                m.pack_cd[i] = PACK_CD;
                m.events.push(format!("m:{i}:{px:.2}:{pz:.2}"));
                break;
            }
        }
    }
}

fn hist_at(hist: &[(f32, V3)], t: f32) -> Option<V3> {
    if hist.is_empty() {
        return None;
    }
    // nearest sample at-or-before t (fallback to first)
    let mut chosen = hist[0].1;
    for (ht, hp) in hist {
        if *ht <= t {
            chosen = *hp;
        } else {
            break;
        }
    }
    Some(chosen)
}

fn do_blink(m: &mut Match, id: u32) {
    let p = match m.players.get_mut(&id) {
        Some(p) => p,
        None => return,
    };
    if !p.alive || p.blink <= 0 {
        return;
    }
    p.blink -= 1;
    if p.blink == BLINK_MAX - 1 {
        p.blink_recharge = BLINK_RECHARGE;
    }
    // dash in current movement direction (or facing if standing still)
    let (sy, cy) = (p.yaw.sin(), p.yaw.cos());
    let fwd = V3::new(-sy, 0.0, -cy);
    let right = V3::new(cy, 0.0, -sy);
    let mut d = V3::default();
    if p.inf {
        d = d.add(fwd);
    }
    if p.inb {
        d = d.sub(fwd);
    }
    if p.inr {
        d = d.add(right);
    }
    if p.inl {
        d = d.sub(right);
    }
    if d.len() < 0.01 {
        d = fwd;
    }
    let dl = d.len();
    d = d.scale(1.0 / dl);
    p.pos = p.pos.add(d.scale(BLINK_DIST));
    p.pos.x = p.pos.x.clamp(-ARENA, ARENA);
    p.pos.z = p.pos.z.clamp(-ARENA, ARENA);
    let pos = p.pos;
    m.events.push(format!("b:{id}:{:.2}:{:.2}:{:.2}", pos.x, pos.y, pos.z));
}

fn do_recall(m: &mut Match, id: u32) {
    let p = match m.players.get_mut(&id) {
        Some(p) => p,
        None => return,
    };
    if !p.alive || p.recall_cd > 0.0 || p.self_hist.is_empty() {
        return;
    }
    p.recall_cd = RECALL_CD;
    // rewind ~3s: pick the sample that many ticks back
    let back = (RECALL_SECS / DT) as usize;
    let idx = p.self_hist.len().saturating_sub(back);
    let (pos, hp, ammo) = p.self_hist[idx];
    p.pos = pos;
    p.hp = hp.max(p.hp); // recall never hurts you
    p.ammo = ammo;
    p.vel = V3::default();
    m.events.push(format!("r:{id}:{:.2}:{:.2}:{:.2}", pos.x, pos.y, pos.z));
}

fn bot_ai(m: &mut Match) {
    let humans: Vec<(u32, V3, u8, bool)> =
        m.players.iter().map(|(id, p)| (*id, p.pos, p.team, p.alive)).collect();
    let bot_ids: Vec<u32> = m.players.iter().filter(|(_, p)| p.bot).map(|(id, _)| *id).collect();
    for id in bot_ids {
        // find nearest living enemy
        let (bpos, bteam, alive) = {
            let b = &m.players[&id];
            (b.pos, b.team, b.alive)
        };
        if !alive {
            continue;
        }
        let target = humans
            .iter()
            .filter(|(tid, _, tt, ta)| *tid != id && *tt != bteam && *ta)
            .min_by(|a, b| a.1.sub(bpos).len().total_cmp(&b.1.sub(bpos).len()));
        let b = m.players.get_mut(&id).unwrap();
        b.ai_t -= DT;
        if b.ai_t <= 0.0 {
            b.ai_t = 1.0 + (id % 5) as f32 * 0.3;
            b.ai_strafe = if (id % 2) == 0 { 1.0 } else { -1.0 };
        }
        match target {
            Some(&(_, tpos, _, _)) => {
                if b.hp < 55.0 {
                    // low HP: break off and run to the nearest health pack
                    let pk = PACKS.iter().min_by(|a, c| {
                        ((bpos.x - a.0).powi(2) + (bpos.z - a.1).powi(2))
                            .total_cmp(&((bpos.x - c.0).powi(2) + (bpos.z - c.1).powi(2)))
                    });
                    if let Some(&(px, pz)) = pk {
                        b.yaw = (-(px - bpos.x)).atan2(-(pz - bpos.z));
                        b.pitch = 0.0;
                        b.inf = true;
                        b.inb = false;
                        b.inl = false;
                        b.inr = false;
                        b.firing = false;
                    }
                } else {
                    let to = tpos.sub(bpos);
                    let dist = to.len().max(0.01);
                    let horiz = (to.x * to.x + to.z * to.z).sqrt().max(0.01);
                    b.yaw = (-to.x).atan2(-to.z);
                    b.pitch = ((tpos.y + 1.0) - (bpos.y + EYE)).atan2(horiz); // aim at the chest
                    b.inf = dist > 12.0;
                    b.inb = dist < 6.0;
                    b.inl = b.ai_strafe > 0.0;
                    b.inr = b.ai_strafe < 0.0;
                    b.inj = false;
                    b.firing = dist < RANGE && !blocked(b.eye(), V3::new(tpos.x, tpos.y + 1.0, tpos.z));
                }
            }
            None => {
                b.inf = true;
                b.inl = false;
                b.inr = false;
                b.firing = false;
                b.yaw += DT; // idle spin to wander
            }
        }
    }
}

fn build_snapshot(m: &Match) -> String {
    let mut s = String::new();
    s.push_str(&format!("g\t{:.2}\t{}\t{}\t{}\t{}\t{:.1}\n", m.time, m.score[0], m.score[1], m.winner, SCORE_WIN, m.intermission));
    for (id, p) in &m.players {
        s.push_str(&format!(
            "p\t{id}\t{:.2}\t{:.2}\t{:.2}\t{:.3}\t{:.3}\t{:.0}\t{}\t{}\t{}\t{}\t{:.1}\t{:.1}\t{}\t{}\t{}\t{}\t{}\n",
            p.pos.x,
            p.pos.y,
            p.pos.z,
            p.yaw,
            p.pitch,
            p.hp.max(0.0),
            p.team,
            u8::from(p.alive),
            p.ammo,
            p.blink,
            p.blink_recharge,
            p.recall_cd,
            u8::from(p.reload_t > 0.0),
            p.elims,
            p.deaths,
            p.name,
            (p.ult / ULT_MAX * 100.0) as i32,
        ));
    }
    let bombs = m.bombs.iter().map(|b| format!("{:.2}:{:.2}:{:.2}", b.pos.x, b.pos.y, b.pos.z)).collect::<Vec<_>>().join(";");
    s.push_str(&format!("z\t{bombs}\n"));
    let packs = m.pack_cd.iter().map(|c| if *c <= 0.0 { "1" } else { "0" }).collect::<Vec<_>>().join(" ");
    s.push_str(&format!("d\t{packs}\n"));
    s.push_str(&format!("x\t{}\n", m.events.join(";")));
    s
}

// --- connections -----------------------------------------------------------

fn handle(mut stream: TcpStream, server: Arc<Mutex<Server>>) {
    let Some(head) = read_http_head(&mut stream) else { return };
    if !head.to_ascii_lowercase().contains("upgrade: websocket") {
        serve_file(&mut stream, &head);
        return;
    }
    let Some(key) = header_value(&head, "sec-websocket-key") else { return };
    let accept = ws::accept_key(&key);
    let resp = format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
    );
    if stream.write_all(resp.as_bytes()).is_err() {
        return;
    }

    let id = {
        let mut s = server.lock().unwrap();
        let id = s.next_id;
        s.next_id += 1;
        id
    };
    let mine: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let pong: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    let mut writer = stream.try_clone().expect("clone");
    let wserver = server.clone();
    let wmine = mine.clone();
    let wpong = pong.clone();
    let writer_handle = thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(TICK_MS));
        let snap = {
            let code = wmine.lock().unwrap().clone();
            code.and_then(|c| wserver.lock().unwrap().matches.get(&c).map(|m| m.snapshot.clone()))
        };
        if let Some(s) = snap {
            if ws::write_text(&mut writer, &s).is_err() {
                return;
            }
        }
        // piggyback any pong (single-writer: avoids interleaved frames)
        if let Some(pg) = wpong.lock().unwrap().take() {
            if ws::write_text(&mut writer, &pg).is_err() {
                return;
            }
        }
    });

    loop {
        match ws::read_frame(&mut stream) {
            Ok(Some(ws::Msg::Text(t))) => {
                let parts: Vec<&str> = t.split_whitespace().collect();
                match parts.as_slice() {
                    ["ping", t] => {
                        *pong.lock().unwrap() = Some(format!("P\t{t}"));
                    }
                    ["join", nick] => {
                        let nick = clean_nick(nick, id);
                        let mut s = server.lock().unwrap();
                        let seed = now_nanos();
                        let m = s.matches.entry("ARENA".to_string()).or_insert_with(|| new_match(seed));
                        let sp = spawn_point(&mut m.rng, 0);
                        m.players.insert(id, Player::new(nick, false, 0, sp));
                        *mine.lock().unwrap() = Some("ARENA".to_string());
                        let _ = ws::write_text(&mut stream, &format!("w\t{id}"));
                    }
                    ["in", f, b, l, r, j, yaw, pitch, lat] => {
                        if let Some(code) = mine.lock().unwrap().clone() {
                            let mut s = server.lock().unwrap();
                            if let Some(p) = s.matches.get_mut(&code).and_then(|m| m.players.get_mut(&id)) {
                                p.inf = *f == "1";
                                p.inb = *b == "1";
                                p.inl = *l == "1";
                                p.inr = *r == "1";
                                p.inj = *j == "1";
                                p.yaw = yaw.parse().unwrap_or(p.yaw);
                                p.pitch = pitch.parse().unwrap_or(p.pitch);
                                p.lat = (lat.parse::<f32>().unwrap_or(0.0) / 1000.0).clamp(0.0, 0.3);
                            }
                        }
                    }
                    [cmd @ ("fire" | "stop" | "blink" | "recall" | "reload" | "ult")] => {
                        if let Some(code) = mine.lock().unwrap().clone() {
                            let mut s = server.lock().unwrap();
                            if let Some(m) = s.matches.get_mut(&code) {
                                match *cmd {
                                    "fire" => {
                                        if let Some(p) = m.players.get_mut(&id) {
                                            p.firing = true;
                                        }
                                    }
                                    "stop" => {
                                        if let Some(p) = m.players.get_mut(&id) {
                                            p.firing = false;
                                        }
                                    }
                                    "blink" => do_blink(m, id),
                                    "recall" => do_recall(m, id),
                                    "ult" => do_ult(m, id),
                                    "reload" => {
                                        if let Some(p) = m.players.get_mut(&id) {
                                            if p.ammo < MAG && p.reload_t <= 0.0 {
                                                p.reload_t = RELOAD_T;
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Some(ws::Msg::Other)) => {}
            _ => break,
        }
    }

    if let Some(code) = mine.lock().unwrap().clone() {
        let mut s = server.lock().unwrap();
        if let Some(m) = s.matches.get_mut(&code) {
            m.players.remove(&id);
        }
    }
    drop(stream);
    let _ = writer_handle.join();
}

fn clean_nick(s: &str, id: u32) -> String {
    let n: String = s.chars().filter(|c| !c.is_whitespace()).take(14).collect();
    if n.is_empty() { format!("Player{id}") } else { n }
}
fn now_nanos() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1)
}

fn read_http_head(stream: &mut TcpStream) -> Option<String> {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        match stream.read(&mut byte) {
            Ok(0) => return None,
            Ok(_) => {
                buf.push(byte[0]);
                if buf.ends_with(b"\r\n\r\n") {
                    break;
                }
                if buf.len() > 16 * 1024 {
                    return None;
                }
            }
            Err(_) => return None,
        }
    }
    Some(String::from_utf8_lossy(&buf).into_owned())
}
fn header_value(head: &str, name: &str) -> Option<String> {
    head.lines()
        .find(|l| l.to_ascii_lowercase().starts_with(&format!("{name}:")))
        .and_then(|l| l.split_once(':'))
        .map(|(_, v)| v.trim().to_string())
}
fn serve_file(stream: &mut TcpStream, head: &str) {
    let raw = head.lines().next().and_then(|l| l.split_whitespace().nth(1)).unwrap_or("/");
    let path = raw.split('?').next().unwrap_or("/");
    let (ctype, body): (&str, &[u8]) = match path {
        "/" | "/index.html" => ("text/html; charset=utf-8", include_str!("../web/ow.html").as_bytes()),
        "/ow.js" => ("application/javascript; charset=utf-8", include_str!("../web/ow.js").as_bytes()),
        _ => {
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
            return;
        }
    };
    let resp = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(resp.as_bytes());
    let _ = stream.write_all(body);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m1() -> Match {
        let mut m = new_match(1);
        m.players.clear();
        m
    }

    #[test]
    fn lag_comp_hits_a_target_directly_ahead() {
        let mut m = m1();
        // offset on +x so the shot doesn't pass through the central cover pillar
        let mut shooter = Player::new("s".into(), false, 0, V3::new(8.0, 0.0, 5.0));
        shooter.yaw = 0.0; // facing -z
        shooter.pitch = 0.0;
        shooter.firing = true;
        // build some history for the target right in front (-z)
        let mut tgt = Player::new("t".into(), false, 1, V3::new(8.0, 0.0, -2.0));
        for _ in 0..HIST {
            tgt.hist.push((m.time, tgt.eye()));
        }
        m.players.insert(1, shooter);
        m.players.insert(2, tgt);
        try_fire(&mut m, 1);
        assert!(m.players[&2].hp < MAX_HP, "target took damage");
        assert!(m.players[&1].ult > 0.0, "shooter gained ult charge from damage");
        assert!(m.events.iter().any(|e| e.starts_with("t:")), "tracer emitted");
    }

    #[test]
    fn no_friendly_fire() {
        let mut m = m1();
        let mut shooter = Player::new("s".into(), false, 0, V3::new(0.0, 0.0, 5.0));
        shooter.firing = true;
        let mut mate = Player::new("m".into(), false, 0, V3::new(0.0, 0.0, -2.0)); // same team
        for _ in 0..HIST {
            mate.hist.push((m.time, mate.eye()));
        }
        m.players.insert(1, shooter);
        m.players.insert(2, mate);
        try_fire(&mut m, 1);
        assert_eq!(m.players[&2].hp, MAX_HP, "no damage to a teammate");
    }

    #[test]
    fn recall_restores_past_position() {
        let mut m = m1();
        let mut p = Player::new("p".into(), false, 0, V3::new(9.0, 0.0, 9.0));
        // history where the player was at origin ~3s ago
        let back = (RECALL_SECS / DT) as usize;
        for i in 0..back + 5 {
            let x = if i < back { 0.0 } else { 9.0 }; // 3s ago the player was at origin
            p.self_hist.push((V3::new(x, 0.0, x), 40.0, 5));
        }
        m.players.insert(1, p);
        do_recall(&mut m, 1);
        assert!((m.players[&1].pos.x - 0.0).abs() < 0.01, "recalled to the old position");
        assert!(m.players[&1].recall_cd > 0.0, "recall on cooldown");
    }

    #[test]
    fn ultimate_charges_from_damage_then_pulse_bomb_explodes() {
        let mut m = m1();
        let mut s = Player::new("s".into(), false, 0, V3::new(8.0, 0.0, 5.0));
        s.ult = ULT_MAX;
        s.yaw = 0.0;
        let mut t = Player::new("t".into(), false, 1, V3::new(8.0, 0.0, -1.0));
        for _ in 0..HIST {
            t.hist.push((m.time, t.eye()));
        }
        m.players.insert(1, s);
        m.players.insert(2, t);
        do_ult(&mut m, 1);
        assert_eq!(m.bombs.len(), 1, "pulse bomb thrown");
        assert_eq!(m.players[&1].ult, 0.0, "ult consumed");
        for _ in 0..200 {
            update_bombs(&mut m);
            if m.bombs.is_empty() {
                break;
            }
        }
        assert!(m.bombs.is_empty(), "bomb detonated");
        assert!(m.players[&2].hp < MAX_HP, "pulse bomb damaged the enemy");
    }

    #[test]
    fn reaching_score_target_declares_winner_then_resets() {
        let mut m = m1();
        m.players.insert(1, Player::new("a".into(), false, 0, V3::default()));
        m.score[0] = SCORE_WIN;
        check_match(&mut m);
        assert_eq!(m.winner, 1, "team A declared the winner");
        m.intermission = 0.0;
        check_match(&mut m); // intermission elapses -> reset
        assert_eq!(m.winner, 0, "winner cleared after reset");
        assert_eq!(m.score, [0, 0], "scores reset for the next round");
    }

    #[test]
    fn health_pack_heals_and_goes_on_cooldown() {
        let mut m = m1();
        let mut p = Player::new("p".into(), false, 0, V3::new(PACKS[0].0, 0.0, PACKS[0].1));
        p.hp = 40.0;
        m.players.insert(1, p);
        check_packs(&mut m);
        assert!(m.players[&1].hp > 40.0, "healed by the pack");
        assert!(m.pack_cd[0] > 0.0, "pack on cooldown");
        // standing on a depleted pack does nothing
        m.players.get_mut(&1).unwrap().hp = 40.0;
        check_packs(&mut m);
        assert_eq!(m.players[&1].hp, 40.0, "depleted pack gives no heal");
    }

    #[test]
    fn blink_moves_and_consumes_a_charge() {
        let mut m = m1();
        let mut p = Player::new("p".into(), false, 0, V3::new(0.0, 0.0, 0.0));
        p.inf = true; // dash forward (-z)
        m.players.insert(1, p);
        do_blink(&mut m, 1);
        assert!(m.players[&1].pos.z < -5.0, "blinked forward");
        assert_eq!(m.players[&1].blink, BLINK_MAX - 1, "consumed a blink charge");
    }
}
