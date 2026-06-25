//! opcusdb Boomborn — a co-op multiplayer **survivor** (Vampire-Survivors-style
//! "bullet heaven") where you play a **Bomberman**: you only move, your **bombs
//! auto-fire**, and you mow down hordes of **vampires**.
//!
//! Everyone who opens the page auto-joins the shared field. Move with WASD/arrows;
//! your weapons fire automatically (lob bombs, a Bomberman cross-blast, homing
//! rockets, a nova pulse). Killed vampires drop **XP gems** — collect them to
//! level up and stack/upgrade weapons. Survive the escalating waves.
//!
//! The Rust server owns the whole simulation (hundreds of enemies, projectiles,
//! explosions) at a fixed tick and broadcasts it over the hand-rolled WebSocket
//! (see [`ws`]); the browser renders (PixiJS) + plays sound. Best kill counts
//! persist to a small local DB file (`survivors.db`, gitignored).
//!
//! Run: `cargo run -p opcusdb-server --bin opcusdb-survivors` then open
//! http://localhost:9006 (open several tabs to play co-op).

use opcusdb_core::Rng;
use opcusdb_server::ws;
use std::collections::{BTreeMap, HashSet};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const PORT: u16 = 9006;
const TICK_MS: u64 = 33; // ~30 Hz
const DT: f32 = 0.033;
const ARENA: f32 = 3000.0;
const MAX_ENEMIES: usize = 220;
const LB_PATH: &str = "survivors.db";
const LB_MAX: usize = 10;

// weapon kinds
const W_BOMB: u8 = 0;
const W_CROSS: u8 = 1;
const W_ROCKET: u8 = 2;
const W_NOVA: u8 = 3;

// enemy kinds: (speed, hp, dmg, xp, radius)
struct EKind {
    speed: f32,
    hp: f32,
    dmg: f32,
    xp: u32,
    r: f32,
}
const EK: [EKind; 4] = [
    EKind { speed: 95.0, hp: 6.0, dmg: 6.0, xp: 1, r: 13.0 },   // 0 bat
    EKind { speed: 46.0, hp: 26.0, dmg: 13.0, xp: 3, r: 18.0 }, // 1 ghoul
    EKind { speed: 70.0, hp: 15.0, dmg: 10.0, xp: 2, r: 15.0 }, // 2 vampire
    EKind { speed: 80.0, hp: 48.0, dmg: 18.0, xp: 7, r: 24.0 }, // 3 bat-lord (elite)
];

struct Weapon {
    kind: u8,
    level: u8,
    cd: f32,
}
impl Weapon {
    fn period(&self) -> f32 {
        let l = self.level as f32;
        match self.kind {
            W_BOMB => (1.05 - l * 0.1).max(0.4),
            W_CROSS => (3.0 - l * 0.25).max(1.2),
            W_ROCKET => (2.0 - l * 0.18).max(0.7),
            _ => (3.2 - l * 0.25).max(1.4),
        }
    }
}

struct Player {
    name: String,
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    facing: i8,
    hp: f32,
    maxhp: f32,
    level: u32,
    xp: u32,
    xpneed: u32,
    kills: u32,
    weapons: Vec<Weapon>,
    iframe: f32,
    down: f32, // >0 = downed, counts to respawn
    li: bool,
    ri: bool,
    ui: bool,
    di: bool,
}
impl Player {
    fn new(name: String) -> Self {
        Self {
            name,
            x: ARENA / 2.0,
            y: ARENA / 2.0,
            vx: 0.0,
            vy: 0.0,
            facing: 1,
            hp: 100.0,
            maxhp: 100.0,
            level: 1,
            xp: 0,
            xpneed: 5,
            kills: 0,
            weapons: vec![Weapon { kind: W_BOMB, level: 1, cd: 0.5 }],
            iframe: 0.0,
            down: 0.0,
            li: false,
            ri: false,
            ui: false,
            di: false,
        }
    }
}

struct Enemy {
    x: f32,
    y: f32,
    kind: u8,
    hp: f32,
    maxhp: f32,
}
struct Proj {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    kind: u8, // 0 lob-bomb (fuse), 1 homing rocket
    life: f32,
    owner: u32,
}
struct Boom {
    x: f32,
    y: f32,
    r: f32,
    max_r: f32,
    t: f32,
    dmg: f32,
    owner: u32,
    hit: HashSet<u32>,
}
struct Gem {
    x: f32,
    y: f32,
    val: u32,
}

struct Room {
    players: BTreeMap<u32, Player>,
    enemies: BTreeMap<u32, Enemy>,
    projs: Vec<Proj>,
    booms: Vec<Boom>,
    gems: Vec<Gem>,
    events: Vec<(char, f32, f32)>,
    rng: Rng,
    time: f32,
    spawn_cd: f32,
    next_eid: u32,
    snapshot: String,
}
impl Room {
    fn new(seed: u64) -> Self {
        Self {
            players: BTreeMap::new(),
            enemies: BTreeMap::new(),
            projs: Vec::new(),
            booms: Vec::new(),
            gems: Vec::new(),
            events: Vec::new(),
            rng: Rng::seed(seed),
            time: 0.0,
            spawn_cd: 1.0,
            next_eid: 1,
            snapshot: String::new(),
        }
    }
}

struct Arena {
    rooms: BTreeMap<String, Room>,
    lb: Vec<(String, u32)>,
    next_id: u32,
}

fn main() {
    let arena = Arc::new(Mutex::new(Arena { rooms: BTreeMap::new(), lb: load_lb(), next_id: 1 }));
    {
        let arena = arena.clone();
        thread::spawn(move || loop {
            thread::sleep(Duration::from_millis(TICK_MS));
            let mut a = arena.lock().unwrap();
            let codes: Vec<String> = a.rooms.keys().cloned().collect();
            for code in codes {
                let mut room = a.rooms.remove(&code).unwrap();
                tick_room(&mut room);
                room.snapshot = build_snapshot(&room);
                if !room.players.is_empty() {
                    a.rooms.insert(code, room);
                }
            }
        });
    }
    let listener = TcpListener::bind(("0.0.0.0", PORT)).expect("bind");
    println!("opcusdb Boomborn on http://localhost:{PORT}  — survive the vampire horde (WASD/arrows)");
    for stream in listener.incoming().flatten() {
        let arena = arena.clone();
        thread::spawn(move || handle(stream, arena));
    }
}

fn dist2(ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (dx, dy) = (ax - bx, ay - by);
    dx * dx + dy * dy
}

fn nearest_enemy(room: &Room, x: f32, y: f32) -> Option<(f32, f32)> {
    room.enemies
        .values()
        .min_by(|a, b| dist2(x, y, a.x, a.y).total_cmp(&dist2(x, y, b.x, b.y)))
        .map(|e| (e.x, e.y))
}

fn nearest_alive_player(room: &Room, x: f32, y: f32) -> Option<(f32, f32)> {
    room.players
        .values()
        .filter(|p| p.down <= 0.0)
        .min_by(|a, b| dist2(x, y, a.x, a.y).total_cmp(&dist2(x, y, b.x, b.y)))
        .map(|p| (p.x, p.y))
}

// --- simulation ------------------------------------------------------------

fn tick_room(room: &mut Room) {
    room.events.clear();
    room.time += DT;

    spawn_wave(room);
    move_players(room);
    fire_weapons(room);
    update_projectiles(room);
    update_booms(room);
    move_enemies(room);
    update_gems(room);
}

fn spawn_wave(room: &mut Room) {
    room.spawn_cd -= DT;
    if room.spawn_cd > 0.0 || room.enemies.len() >= MAX_ENEMIES {
        return;
    }
    let t = room.time;
    room.spawn_cd = (0.8 - t / 130.0).max(0.12);
    let batch = (3.0 + t / 16.0).min(11.0) as u32;
    // pick a player to swarm
    let anchors: Vec<(f32, f32)> = room.players.values().map(|p| (p.x, p.y)).collect();
    if anchors.is_empty() {
        return;
    }
    for _ in 0..batch {
        if room.enemies.len() >= MAX_ENEMIES {
            break;
        }
        let (ax, ay) = anchors[room.rng.below(anchors.len() as u32) as usize];
        let ang = (room.rng.below(628) as f32) / 100.0;
        let rad = 760.0 + room.rng.below(180) as f32;
        let x = (ax + ang.cos() * rad).clamp(20.0, ARENA - 20.0);
        let y = (ay + ang.sin() * rad).clamp(20.0, ARENA - 20.0);
        // type distribution shifts harder over time
        let roll = room.rng.below(100);
        let kind = if t < 25.0 {
            if roll < 80 { 0 } else { 2 }
        } else if t < 70.0 {
            if roll < 45 { 0 } else if roll < 75 { 2 } else if roll < 95 { 1 } else { 3 }
        } else if roll < 30 { 0 } else if roll < 55 { 2 } else if roll < 85 { 1 } else { 3 };
        let scale = 1.0 + t / 110.0;
        let hp = EK[kind as usize].hp * scale;
        let id = room.next_eid;
        room.next_eid += 1;
        room.enemies.insert(id, Enemy { x, y, kind, hp, maxhp: hp });
    }
}

fn move_players(room: &mut Room) {
    for p in room.players.values_mut() {
        if p.down > 0.0 {
            p.down -= DT;
            if p.down <= 0.0 {
                p.hp = p.maxhp * 0.6;
                p.iframe = 2.0;
            }
            continue;
        }
        let dx = (p.ri as i32 - p.li as i32) as f32;
        let dy = (p.di as i32 - p.ui as i32) as f32;
        let len = (dx * dx + dy * dy).sqrt();
        let sp = 230.0;
        if len > 0.0 {
            p.vx = dx / len * sp;
            p.vy = dy / len * sp;
            p.facing = if dx != 0.0 { dx.signum() as i8 } else { p.facing };
        } else {
            p.vx = 0.0;
            p.vy = 0.0;
        }
        p.x = (p.x + p.vx * DT).clamp(16.0, ARENA - 16.0);
        p.y = (p.y + p.vy * DT).clamp(16.0, ARENA - 16.0);
        if p.iframe > 0.0 {
            p.iframe -= DT;
        }
        if p.hp < p.maxhp {
            p.hp = (p.hp + 1.5 * DT).min(p.maxhp); // slow regen
        }
    }
}

fn fire_weapons(room: &mut Room) {
    let ids: Vec<u32> = room.players.keys().copied().collect();
    for id in ids {
        let (px, py, down) = {
            let p = &room.players[&id];
            (p.x, p.y, p.down > 0.0)
        };
        if down {
            continue;
        }
        let target = nearest_enemy(room, px, py);
        let n = room.players[&id].weapons.len();
        for wi in 0..n {
            let (kind, level, ready) = {
                let w = &mut room.players.get_mut(&id).unwrap().weapons[wi];
                w.cd -= DT;
                if w.cd > 0.0 {
                    continue;
                }
                w.cd = w.period();
                (w.kind, w.level, true)
            };
            if !ready {
                continue;
            }
            match kind {
                W_BOMB => {
                    let count = 1 + level as usize / 2;
                    for k in 0..count {
                        let (tx, ty) = target.unwrap_or((px + 200.0, py));
                        let ang = (tx - px).atan2(ty - py) + (k as f32 - count as f32 / 2.0) * 0.25;
                        let sp = 360.0;
                        room.projs.push(Proj {
                            x: px,
                            y: py,
                            vx: ang.sin() * sp,
                            vy: ang.cos() * sp,
                            kind: 0,
                            life: 0.55,
                            owner: id,
                        });
                    }
                    room.events.push(('t', px, py));
                }
                W_CROSS => {
                    // Bomberman cross: expanding booms along the 4 axes
                    let dmg = 9.0 + level as f32 * 3.0;
                    let reach = 90.0 + level as f32 * 22.0;
                    room.booms.push(mk_boom(px, py, 56.0, dmg, id));
                    for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                        for step in 1..=2 {
                            let d = reach * step as f32 / 2.0;
                            room.booms.push(mk_boom(px + dx * d, py + dy * d, 48.0, dmg, id));
                        }
                    }
                    room.events.push(('x', px, py));
                }
                W_ROCKET => {
                    let count = 1 + (level as usize).div_ceil(2);
                    for _ in 0..count {
                        room.projs.push(Proj {
                            x: px,
                            y: py,
                            vx: 0.0,
                            vy: -260.0,
                            kind: 1,
                            life: 2.2,
                            owner: id,
                        });
                    }
                    room.events.push(('t', px, py));
                }
                _ => {
                    // nova: ring blast around the player
                    let dmg = 7.0 + level as f32 * 2.5;
                    room.booms.push(mk_boom(px, py, 130.0 + level as f32 * 16.0, dmg, id));
                    room.events.push(('x', px, py));
                }
            }
        }
    }
}

fn mk_boom(x: f32, y: f32, max_r: f32, dmg: f32, owner: u32) -> Boom {
    Boom { x, y, r: 8.0, max_r, t: 0.28, dmg, owner, hit: HashSet::new() }
}

fn update_projectiles(room: &mut Room) {
    let mut spawned: Vec<Boom> = Vec::new();
    let mut keep: Vec<Proj> = Vec::with_capacity(room.projs.len());
    let projs = std::mem::take(&mut room.projs);
    for mut pr in projs {
        pr.life -= DT;
        if pr.kind == 1 {
            // homing rocket
            if let Some((tx, ty)) = nearest_enemy(room, pr.x, pr.y) {
                let ang = (tx - pr.x).atan2(ty - pr.y);
                let sp = 420.0;
                pr.vx = pr.vx * 0.86 + ang.sin() * sp * 0.14;
                pr.vy = pr.vy * 0.86 + ang.cos() * sp * 0.14;
            }
        }
        pr.x += pr.vx * DT;
        pr.y += pr.vy * DT;
        // contact with an enemy?
        let hit = room
            .enemies
            .values()
            .any(|e| dist2(pr.x, pr.y, e.x, e.y) < (EK[e.kind as usize].r + 8.0).powi(2));
        if pr.life <= 0.0 || hit {
            let (r, dmg) = if pr.kind == 1 { (62.0, 16.0) } else { (78.0, 13.0) };
            spawned.push(mk_boom(pr.x, pr.y, r, dmg, pr.owner));
            room.events.push(('e', pr.x, pr.y));
        } else {
            keep.push(pr);
        }
    }
    room.projs = keep;
    room.booms.extend(spawned);
}

fn update_booms(room: &mut Room) {
    let mut kills: Vec<(u32, f32, f32, u32, u8)> = Vec::new(); // (owner, x, y, eid, kind)
    for b in room.booms.iter_mut() {
        b.t -= DT;
        b.r = b.max_r * (1.0 - (b.t / 0.28).max(0.0)).min(1.0); // expand
        for (eid, e) in room.enemies.iter_mut() {
            if b.hit.contains(eid) {
                continue;
            }
            if dist2(b.x, b.y, e.x, e.y) < (b.r + EK[e.kind as usize].r).powi(2) {
                b.hit.insert(*eid);
                e.hp -= b.dmg;
                if e.hp <= 0.0 {
                    kills.push((b.owner, e.x, e.y, *eid, e.kind));
                }
            }
        }
    }
    room.booms.retain(|b| b.t > 0.0);
    for (owner, x, y, eid, kind) in kills {
        room.enemies.remove(&eid);
        room.gems.push(Gem { x, y, val: EK[kind as usize].xp });
        room.events.push(('k', x, y));
        if let Some(p) = room.players.get_mut(&owner) {
            p.kills += 1;
        }
    }
}

fn move_enemies(room: &mut Room) {
    let ids: Vec<u32> = room.enemies.keys().copied().collect();
    for id in ids {
        let (ex, ey, kind) = {
            let e = &room.enemies[&id];
            (e.x, e.y, e.kind)
        };
        if let Some((tx, ty)) = nearest_alive_player(room, ex, ey) {
            let ang = (tx - ex).atan2(ty - ey);
            let sp = EK[kind as usize].speed;
            let e = room.enemies.get_mut(&id).unwrap();
            e.x += ang.sin() * sp * DT;
            e.y += ang.cos() * sp * DT;
        }
    }
    // contact damage to players
    let er = |k: u8| EK[k as usize].r;
    let elist: Vec<(f32, f32, u8)> = room.enemies.values().map(|e| (e.x, e.y, e.kind)).collect();
    for p in room.players.values_mut() {
        if p.down > 0.0 || p.iframe > 0.0 {
            continue;
        }
        for &(ex, ey, k) in &elist {
            if dist2(p.x, p.y, ex, ey) < (er(k) + 15.0).powi(2) {
                p.hp -= EK[k as usize].dmg;
                p.iframe = 0.6;
                room.events.push(('h', p.x, p.y));
                if p.hp <= 0.0 {
                    p.hp = 0.0;
                    p.down = 5.0;
                    room.events.push(('d', p.x, p.y));
                }
                break;
            }
        }
    }
}

fn update_gems(room: &mut Room) {
    let mut gained: Vec<(u32, u32)> = Vec::new(); // (player, xp)
    let mut keep: Vec<Gem> = Vec::with_capacity(room.gems.len());
    let gems = std::mem::take(&mut room.gems);
    'gem: for mut g in gems {
        // find nearest player; magnetize within radius, collect when close
        let mut best: Option<(u32, f32, f32, f32)> = None;
        for (id, p) in room.players.iter() {
            if p.down > 0.0 {
                continue;
            }
            let d = dist2(g.x, g.y, p.x, p.y);
            if best.map_or(true, |(_, bd, _, _)| d < bd) {
                best = Some((*id, d, p.x, p.y));
            }
        }
        if let Some((pid, d, pxp, pyp)) = best {
            if d < 26.0_f32.powi(2) {
                gained.push((pid, g.val));
                continue 'gem;
            }
            if d < 150.0_f32.powi(2) {
                let ang = (pxp - g.x).atan2(pyp - g.y);
                g.x += ang.sin() * 300.0 * DT;
                g.y += ang.cos() * 300.0 * DT;
            }
        }
        keep.push(g);
    }
    room.gems = keep;
    for (pid, xp) in gained {
        if let Some(p) = room.players.get_mut(&pid) {
            p.xp += xp;
            while p.xp >= p.xpneed {
                p.xp -= p.xpneed;
                p.level += 1;
                p.xpneed = (p.xpneed as f32 * 1.3) as u32 + 2;
                level_up(p, &mut room.rng);
                room.events.push(('l', p.x, p.y));
            }
        }
    }
}

/// On level up: learn a new weapon, or upgrade an owned one; small heal.
fn level_up(p: &mut Player, rng: &mut Rng) {
    p.maxhp += 6.0;
    p.hp = (p.hp + 20.0).min(p.maxhp);
    let all = [W_BOMB, W_CROSS, W_ROCKET, W_NOVA];
    let missing: Vec<u8> = all.iter().copied().filter(|k| !p.weapons.iter().any(|w| w.kind == *k)).collect();
    let learn_new = !missing.is_empty() && (p.weapons.iter().all(|w| w.level >= 2) || rng.below(2) == 0);
    if learn_new {
        let k = missing[rng.below(missing.len() as u32) as usize];
        p.weapons.push(Weapon { kind: k, level: 1, cd: 0.3 });
    } else {
        // upgrade the lowest-level weapon
        if let Some(w) = p.weapons.iter_mut().filter(|w| w.level < 5).min_by_key(|w| w.level) {
            w.level += 1;
        } else if let Some(w) = p.weapons.iter_mut().min_by_key(|w| w.level) {
            w.level = (w.level + 1).min(5);
        }
    }
}

fn build_snapshot(room: &Room) -> String {
    let mut s = String::new();
    s.push_str(&format!("a\t{}\t{}\t{}\n", ARENA as i32, ARENA as i32, room.time as i32));
    for (id, p) in &room.players {
        s.push_str(&format!(
            "p\t{id}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            p.x as i32,
            p.y as i32,
            p.hp as i32,
            p.maxhp as i32,
            p.level,
            p.xp,
            p.xpneed,
            p.facing,
            p.kills,
            if p.down > 0.0 { 1 } else { 0 },
            p.name,
        ));
    }
    let e = room
        .enemies
        .iter()
        .map(|(id, e)| format!("{id},{},{},{},{}", e.x as i32, e.y as i32, e.kind, (e.hp / e.maxhp * 9.0) as i32))
        .collect::<Vec<_>>()
        .join(";");
    s.push_str(&format!("e\t{e}\n"));
    let j = room.projs.iter().map(|p| format!("{},{},{}", p.x as i32, p.y as i32, p.kind)).collect::<Vec<_>>().join(";");
    s.push_str(&format!("j\t{j}\n"));
    let o = room.booms.iter().map(|b| format!("{},{},{}", b.x as i32, b.y as i32, b.r as i32)).collect::<Vec<_>>().join(";");
    s.push_str(&format!("o\t{o}\n"));
    let m = room.gems.iter().map(|g| format!("{},{}", g.x as i32, g.y as i32)).collect::<Vec<_>>().join(";");
    s.push_str(&format!("m\t{m}\n"));
    let ev = room.events.iter().map(|(t, x, y)| format!("{t}:{}:{}", *x as i32, *y as i32)).collect::<Vec<_>>().join(";");
    s.push_str(&format!("x\t{ev}\n"));
    s
}

// --- persistence -----------------------------------------------------------

fn load_lb() -> Vec<(String, u32)> {
    std::fs::read_to_string(LB_PATH)
        .map(|t| {
            t.lines()
                .filter_map(|l| {
                    let (n, k) = l.rsplit_once(' ')?;
                    Some((n.to_string(), k.trim().parse().ok()?))
                })
                .collect()
        })
        .unwrap_or_default()
}
fn save_lb(lb: &[(String, u32)]) {
    let body: String = lb.iter().map(|(n, k)| format!("{n} {k}\n")).collect();
    let _ = std::fs::write(LB_PATH, body);
}
fn record_kills(lb: &mut Vec<(String, u32)>, name: &str, kills: u32) {
    if kills == 0 {
        return;
    }
    if let Some(e) = lb.iter_mut().find(|(n, _)| n == name) {
        if kills > e.1 {
            e.1 = kills;
        }
    } else {
        lb.push((name.to_string(), kills));
    }
    lb.sort_by_key(|e| std::cmp::Reverse(e.1));
    lb.truncate(LB_MAX);
}

// --- connections -----------------------------------------------------------

fn handle(mut stream: TcpStream, arena: Arc<Mutex<Arena>>) {
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
        let mut a = arena.lock().unwrap();
        let id = a.next_id;
        a.next_id += 1;
        id
    };
    let my_room: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    let mut writer = stream.try_clone().expect("clone");
    let warena = arena.clone();
    let wroom = my_room.clone();
    let writer_handle = thread::spawn(move || {
        let mut lb_sent = String::new();
        loop {
            thread::sleep(Duration::from_millis(TICK_MS));
            let (snap, lbline) = {
                let code = wroom.lock().unwrap().clone();
                let a = warena.lock().unwrap();
                let lb = a.lb.iter().map(|(n, k)| format!("{n}:{k}")).collect::<Vec<_>>().join(",");
                (code.and_then(|c| a.rooms.get(&c).map(|r| r.snapshot.clone())), format!("l\t{lb}\n"))
            };
            if let Some(s) = snap {
                if ws::write_text(&mut writer, &s).is_err() {
                    return;
                }
                if lbline != lb_sent {
                    let _ = ws::write_text(&mut writer, &lbline);
                    lb_sent = lbline;
                }
            }
        }
    });

    loop {
        match ws::read_frame(&mut stream) {
            Ok(Some(ws::Msg::Text(t))) => {
                let parts: Vec<&str> = t.split_whitespace().collect();
                match parts.as_slice() {
                    ["join", nick] => {
                        let nick = clean_nick(nick, id);
                        let mut a = arena.lock().unwrap();
                        let seed = now_nanos() ^ id as u64;
                        let room = a.rooms.entry("FIELD".to_string()).or_insert_with(|| Room::new(seed));
                        room.players.insert(id, Player::new(nick));
                        *my_room.lock().unwrap() = Some("FIELD".to_string());
                        let _ = ws::write_text(&mut stream, &format!("w\t{id}"));
                    }
                    ["keys", l, r, u, d] => {
                        if let Some(code) = my_room.lock().unwrap().clone() {
                            let mut a = arena.lock().unwrap();
                            if let Some(p) = a.rooms.get_mut(&code).and_then(|rm| rm.players.get_mut(&id)) {
                                p.li = *l == "1";
                                p.ri = *r == "1";
                                p.ui = *u == "1";
                                p.di = *d == "1";
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

    if let Some(code) = my_room.lock().unwrap().clone() {
        let mut a = arena.lock().unwrap();
        // remove the player and grab its name+kills in one step (no double borrow)
        let info = a.rooms.get_mut(&code).and_then(|r| r.players.remove(&id)).map(|p| (p.name, p.kills));
        if let Some((name, kills)) = info {
            record_kills(&mut a.lb, &name, kills);
            save_lb(&a.lb);
        }
    }
    drop(stream);
    let _ = writer_handle.join();
}

fn clean_nick(s: &str, id: u32) -> String {
    let n: String = s.chars().filter(|c| !c.is_whitespace()).take(12).collect();
    if n.is_empty() { format!("P{id}") } else { n }
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
    let path = head.lines().next().and_then(|l| l.split_whitespace().nth(1)).unwrap_or("/");
    let (ctype, body): (&str, &[u8]) = match path {
        "/" | "/index.html" => ("text/html; charset=utf-8", include_str!("../web/survivors.html").as_bytes()),
        "/survivors.js" => ("application/javascript; charset=utf-8", include_str!("../web/survivors.js").as_bytes()),
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

    fn room1() -> Room {
        let mut r = Room::new(1);
        r.players.insert(1, Player::new("hero".into()));
        r
    }

    #[test]
    fn explosion_kills_enemy_drops_gem_and_scores() {
        let mut r = room1();
        r.enemies.insert(9, Enemy { x: 1500.0, y: 1500.0, kind: 0, hp: 6.0, maxhp: 6.0 });
        r.booms.push(mk_boom(1500.0, 1500.0, 80.0, 50.0, 1));
        update_booms(&mut r);
        assert!(r.enemies.is_empty(), "enemy died");
        assert_eq!(r.gems.len(), 1, "dropped an xp gem");
        assert_eq!(r.players[&1].kills, 1, "owner scored the kill");
    }

    #[test]
    fn enemy_moves_toward_player() {
        let mut r = room1();
        r.players.get_mut(&1).unwrap().x = 1000.0;
        r.players.get_mut(&1).unwrap().y = 1000.0;
        r.enemies.insert(9, Enemy { x: 1300.0, y: 1000.0, kind: 0, hp: 6.0, maxhp: 6.0 });
        move_enemies(&mut r);
        assert!(r.enemies[&9].x < 1300.0, "enemy stepped toward the player");
    }

    #[test]
    fn level_up_grants_weapon_or_upgrade() {
        let mut r = room1();
        let before = r.players[&1].weapons.len();
        let mut rng = Rng::seed(5);
        level_up(r.players.get_mut(&1).unwrap(), &mut rng);
        let p = &r.players[&1];
        let total: u8 = p.weapons.iter().map(|w| w.level).sum();
        assert!(p.weapons.len() > before || total > 1, "gained a weapon or a level");
        assert!(p.maxhp > 100.0, "max hp increased on level up");
    }

    #[test]
    fn gems_are_collected_when_close() {
        let mut r = room1();
        let (px, py) = (r.players[&1].x, r.players[&1].y);
        r.gems.push(Gem { x: px + 5.0, y: py, val: 3 });
        update_gems(&mut r);
        assert!(r.gems.is_empty(), "gem picked up");
        assert!(r.players[&1].xp >= 3 || r.players[&1].level > 1, "xp gained");
    }
}
