//! opcusdb Smackdown — an authoritative online **platform fighter** (Smash-like).
//!
//! Everyone who opens the page auto-joins the shared **ARENA** room and is given a
//! pixel fighter. Move with the arrow keys, **Z = attack**, **X = jump** (double
//! jump). Hits add **damage %**; the higher your %, the farther you fly — knock a
//! rival off the blast zone to score a KO. The Rust server owns the physics at a
//! fixed tick and broadcasts the world over the hand-rolled WebSocket (see [`ws`]);
//! the browser only renders (PixiJS) + plays sound.
//!
//! Run: `cargo run -p opcusdb-server --bin opcusdb-smash` then open
//! http://localhost:9005 in several tabs and brawl.

use opcusdb_server::ws;
use std::collections::{BTreeMap, HashSet};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const PORT: u16 = 9005;
const TICK_MS: u64 = 33; // ~30 Hz
const MAX_PLAYERS: usize = 8;

// stage geometry (logical units; the client scales/scrolls)
const STAGE_W: f32 = 2400.0;
const STAGE_H: f32 = 540.0;
const FLOOR_Y: f32 = 470.0;
// platforms: (x0, x1, top)
const PLATFORMS: [(f32, f32, f32); 4] =
    [(200.0, 2200.0, FLOOR_Y), (520.0, 820.0, 330.0), (1090.0, 1320.0, 300.0), (1580.0, 1880.0, 330.0)];
// blast zones
const BLAST_L: f32 = -70.0;
const BLAST_R: f32 = STAGE_W + 70.0;
const BLAST_D: f32 = 660.0;
const BLAST_U: f32 = -300.0;

// physics
const GRAVITY: f32 = 0.85;
const MAX_FALL: f32 = 18.0;
const MOVE: f32 = 6.2;
const AIR_ACCEL: f32 = 0.7;
const FRICTION: f32 = 0.72;
const JUMP_V: f32 = -15.5;
const MAX_JUMPS: u8 = 2;

// combat
const BW: f32 = 34.0; // body half handled below; this is full width
const BH: f32 = 54.0;
const ATK_TOTAL: i32 = 12;
const ATK_ACTIVE: std::ops::RangeInclusive<i32> = 6..=9;
const REACH: f32 = 34.0;
const DMG: f32 = 7.0;
const KB_BASE: f32 = 7.5;
const KB_SCALE: f32 = 0.13;
const HITSTUN_F: f32 = 2.4;
const RESPAWN_TICKS: i32 = 70;
const LAST_HIT_MEMORY: i32 = 90;

const FIGHTERS: usize = 8; // palette count, client-side colours

struct Player {
    name: String,
    ch: u8,
    x: f32,
    y: f32, // x = centre, y = feet (bottom)
    vx: f32,
    vy: f32,
    facing: i8,
    on_ground: bool,
    jumps: u8,
    attack: i32,
    attack_hit: HashSet<u32>,
    hitstun: i32,
    percent: f32,
    score: u32,
    respawn: i32,
    last_hit_by: Option<u32>,
    last_hit_t: i32,
    // inputs
    left: bool,
    right: bool,
    want_jump: bool,
    want_attack: bool,
}

impl Player {
    fn new(name: String, ch: u8, x: f32) -> Self {
        Self {
            name,
            ch,
            x,
            y: 120.0,
            vx: 0.0,
            vy: 0.0,
            facing: 1,
            on_ground: false,
            jumps: MAX_JUMPS,
            attack: 0,
            attack_hit: HashSet::new(),
            hitstun: 0,
            percent: 0.0,
            score: 0,
            respawn: 0,
            last_hit_by: None,
            last_hit_t: 0,
            left: false,
            right: false,
            want_jump: false,
            want_attack: false,
        }
    }
    fn state(&self) -> u8 {
        if self.respawn > 0 {
            5
        } else if self.hitstun > 0 {
            4
        } else if self.attack > 0 {
            3
        } else if !self.on_ground {
            2
        } else if self.vx.abs() > 1.0 {
            1
        } else {
            0
        }
    }
}

struct Room {
    players: BTreeMap<u32, Player>,
    events: Vec<(char, f32, f32)>,
    snapshot: String,
    spawn_i: usize,
}

impl Room {
    fn new() -> Self {
        Self { players: BTreeMap::new(), events: Vec::new(), snapshot: String::new(), spawn_i: 0 }
    }
    fn next_spawn(&mut self) -> f32 {
        let spots = [600.0, 1200.0, 1800.0, 900.0, 1500.0];
        let s = spots[self.spawn_i % spots.len()];
        self.spawn_i += 1;
        s
    }
}

struct Arena {
    rooms: BTreeMap<String, Room>,
    next_id: u32,
}

fn main() {
    let arena = Arc::new(Mutex::new(Arena { rooms: BTreeMap::new(), next_id: 1 }));
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
    println!("opcusdb Smackdown on http://localhost:{PORT}  — open it, fight with arrows + Z/X");
    for stream in listener.incoming().flatten() {
        let arena = arena.clone();
        thread::spawn(move || handle(stream, arena));
    }
}

// --- simulation ------------------------------------------------------------

fn tick_room(room: &mut Room) {
    room.events.clear();

    // Phase 1: per-player movement & state
    for p in room.players.values_mut() {
        if p.last_hit_t > 0 {
            p.last_hit_t -= 1;
        }
        if p.respawn > 0 {
            p.respawn -= 1;
            if p.respawn == 0 {
                p.x = STAGE_W / 2.0;
                p.y = 80.0;
                p.vx = 0.0;
                p.vy = 0.0;
                p.percent = 0.0;
                p.hitstun = 0;
                p.jumps = MAX_JUMPS;
            }
            continue;
        }
        if p.hitstun > 0 {
            p.hitstun -= 1;
        }
        let control = p.hitstun <= 0;
        if control {
            let dir = (p.right as i32 - p.left as i32) as f32;
            if dir != 0.0 {
                p.facing = if dir > 0.0 { 1 } else { -1 };
                if p.on_ground {
                    p.vx = dir * MOVE;
                } else {
                    p.vx += dir * AIR_ACCEL;
                    p.vx = p.vx.clamp(-MOVE, MOVE);
                }
            } else if p.on_ground {
                p.vx *= FRICTION;
                if p.vx.abs() < 0.3 {
                    p.vx = 0.0;
                }
            }
            if p.want_jump && p.jumps > 0 {
                p.vy = JUMP_V;
                p.jumps -= 1;
                p.on_ground = false;
                room.events.push(('j', p.x, p.y));
            }
            if p.want_attack && p.attack <= 0 {
                p.attack = ATK_TOTAL;
                p.attack_hit.clear();
                room.events.push(('a', p.x + p.facing as f32 * 28.0, p.y - 30.0));
            }
        }
        p.want_jump = false;
        p.want_attack = false;

        // gravity + integrate
        p.vy = (p.vy + GRAVITY).min(MAX_FALL);
        let oldy = p.y;
        p.x += p.vx;
        p.y += p.vy;

        // platform landing (only when falling)
        p.on_ground = false;
        if p.vy >= 0.0 {
            for (x0, x1, top) in PLATFORMS {
                if oldy <= top + 1.0 && p.y >= top && p.x >= x0 && p.x <= x1 {
                    p.y = top;
                    p.vy = 0.0;
                    p.on_ground = true;
                    p.jumps = MAX_JUMPS;
                    if oldy < top - 4.0 {
                        room.events.push(('l', p.x, p.y));
                    }
                    break;
                }
            }
        }
        if p.attack > 0 {
            p.attack -= 1;
        }
    }

    // Phase 2: resolve attacks against others
    let snap: Vec<(u32, f32, f32, bool)> = room
        .players
        .iter()
        .map(|(id, p)| (*id, p.x, p.y, p.respawn > 0))
        .collect();
    let mut hits: Vec<(u32, u32, f32)> = Vec::new(); // (attacker, target, dir)
    for (aid, ap) in room.players.iter() {
        if !ATK_ACTIVE.contains(&ap.attack) {
            continue;
        }
        let hx0 = if ap.facing > 0 { ap.x + BW / 2.0 } else { ap.x - BW / 2.0 - REACH };
        let hx1 = hx0 + REACH;
        let (hy0, hy1) = (ap.y - BH * 0.95, ap.y - BH * 0.2);
        for &(tid, tx, ty, dead) in &snap {
            if tid == *aid || dead || ap.attack_hit.contains(&tid) {
                continue;
            }
            let (bx0, bx1) = (tx - BW / 2.0, tx + BW / 2.0);
            let (by0, by1) = (ty - BH, ty);
            if hx0 < bx1 && hx1 > bx0 && hy0 < by1 && hy1 > by0 {
                let dir = if tx >= ap.x { 1.0 } else { -1.0 };
                hits.push((*aid, tid, dir));
            }
        }
    }
    for (aid, tid, dir) in hits {
        if let Some(a) = room.players.get_mut(&aid) {
            a.attack_hit.insert(tid);
        }
        if let Some(t) = room.players.get_mut(&tid) {
            let kb = KB_BASE + t.percent * KB_SCALE;
            t.vx = dir * kb;
            t.vy = -(kb * 0.55) - 4.0;
            t.percent += DMG;
            t.hitstun = (kb * HITSTUN_F) as i32;
            t.last_hit_by = Some(aid);
            t.last_hit_t = LAST_HIT_MEMORY;
            t.on_ground = false;
            room.events.push(('h', t.x, t.y - BH * 0.5));
        }
    }

    // Phase 3: KO / blast zones
    let mut kos: Vec<(u32, Option<u32>, f32, f32)> = Vec::new();
    for (id, p) in room.players.iter() {
        if p.respawn > 0 {
            continue;
        }
        if p.x < BLAST_L || p.x > BLAST_R || p.y > BLAST_D || p.y < BLAST_U {
            let credit = if p.last_hit_t > 0 { p.last_hit_by } else { None };
            kos.push((*id, credit, p.x.clamp(0.0, STAGE_W), p.y.clamp(0.0, STAGE_H)));
        }
    }
    for (id, credit, kx, ky) in kos {
        if let Some(cid) = credit {
            if cid != id {
                if let Some(c) = room.players.get_mut(&cid) {
                    c.score += 1;
                }
            }
        }
        if let Some(p) = room.players.get_mut(&id) {
            p.respawn = RESPAWN_TICKS;
            p.last_hit_by = None;
        }
        room.events.push(('k', kx, ky));
    }
}

fn build_snapshot(room: &Room) -> String {
    let mut s = String::new();
    let plats = PLATFORMS
        .iter()
        .map(|(a, b, t)| format!("{},{},{}", *a as i32, *b as i32, *t as i32))
        .collect::<Vec<_>>()
        .join(";");
    s.push_str(&format!("g\t{}\t{}\t{}\t{}\n", STAGE_W as i32, STAGE_H as i32, FLOOR_Y as i32, plats));
    for (id, p) in &room.players {
        s.push_str(&format!(
            "p\t{id}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            p.ch,
            p.x as i32,
            p.y as i32,
            p.facing,
            p.state(),
            p.percent as i32,
            p.score,
            p.attack,
            p.name,
        ));
    }
    let ev = room
        .events
        .iter()
        .map(|(t, x, y)| format!("{t}:{}:{}", *x as i32, *y as i32))
        .collect::<Vec<_>>()
        .join(";");
    s.push_str(&format!("x\t{ev}\n"));
    s
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
    let writer_handle = thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(TICK_MS));
        let snap = {
            let code = wroom.lock().unwrap().clone();
            match code {
                Some(c) => warena.lock().unwrap().rooms.get(&c).map(|r| r.snapshot.clone()),
                None => None,
            }
        };
        if let Some(s) = snap {
            if ws::write_text(&mut writer, &s).is_err() {
                return;
            }
        }
    });

    loop {
        match ws::read_frame(&mut stream) {
            Ok(Some(ws::Msg::Text(t))) => {
                let parts: Vec<&str> = t.split_whitespace().collect();
                match parts.as_slice() {
                    ["join", code, nick] => {
                        let code = clean_code(code);
                        let nick = clean_nick(nick, id);
                        let mut a = arena.lock().unwrap();
                        let room = a.rooms.entry(code.clone()).or_insert_with(Room::new);
                        if room.players.len() < MAX_PLAYERS {
                            let spawn = room.next_spawn();
                            let ch = ((id as usize) % FIGHTERS) as u8;
                            room.players.insert(id, Player::new(nick, ch, spawn));
                            *my_room.lock().unwrap() = Some(code);
                            let _ = ws::write_text(&mut stream, &format!("w\t{id}\t{ch}"));
                        }
                    }
                    ["keys", l, r] => {
                        if let Some(code) = my_room.lock().unwrap().clone() {
                            let mut a = arena.lock().unwrap();
                            if let Some(p) = a.rooms.get_mut(&code).and_then(|r| r.players.get_mut(&id)) {
                                p.left = *l == "1";
                                p.right = *r == "1";
                            }
                        }
                    }
                    ["jump"] | ["atk"] => {
                        if let Some(code) = my_room.lock().unwrap().clone() {
                            let mut a = arena.lock().unwrap();
                            if let Some(p) = a.rooms.get_mut(&code).and_then(|r| r.players.get_mut(&id)) {
                                if parts[0] == "jump" {
                                    p.want_jump = true;
                                } else {
                                    p.want_attack = true;
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

    if let Some(code) = my_room.lock().unwrap().clone() {
        let mut a = arena.lock().unwrap();
        if let Some(r) = a.rooms.get_mut(&code) {
            r.players.remove(&id);
        }
    }
    drop(stream);
    let _ = writer_handle.join();
}

fn clean_code(s: &str) -> String {
    let c: String = s.chars().filter(|c| c.is_ascii_alphanumeric()).take(6).collect::<String>().to_uppercase();
    if c.is_empty() { "ARENA".to_string() } else { c }
}

fn clean_nick(s: &str, id: u32) -> String {
    let n: String = s.chars().filter(|c| !c.is_whitespace()).take(12).collect();
    if n.is_empty() { format!("P{id}") } else { n }
}

#[allow(dead_code)]
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
        "/" | "/index.html" => ("text/html; charset=utf-8", include_str!("../web/smash.html").as_bytes()),
        "/smash.js" => ("application/javascript; charset=utf-8", include_str!("../web/smash.js").as_bytes()),
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

    fn room_with(a_x: f32, t_x: f32) -> Room {
        let mut room = Room::new();
        let mut a = Player::new("a".into(), 0, a_x);
        a.y = FLOOR_Y;
        a.on_ground = true;
        a.facing = 1;
        a.attack = *ATK_ACTIVE.end(); // stays in the active window after the tick's decrement
        let mut t = Player::new("t".into(), 1, t_x);
        t.y = FLOOR_Y;
        t.on_ground = true;
        room.players.insert(1, a);
        room.players.insert(2, t);
        room
    }

    #[test]
    fn attack_in_front_damages_and_knocks_back() {
        let mut room = room_with(1000.0, 1000.0 + BW); // target just in front (right)
        tick_room(&mut room);
        let t = &room.players[&2];
        assert!(t.percent >= DMG, "target took damage");
        assert!(t.vx > 0.0, "knocked to the right (away from attacker)");
        assert!(room.events.iter().any(|(c, _, _)| *c == 'h'), "hit event emitted");
    }

    #[test]
    fn attack_misses_when_behind() {
        // attacker faces right, target is to the LEFT -> no hit
        let mut room = room_with(1000.0, 1000.0 - BW * 2.0);
        tick_room(&mut room);
        assert_eq!(room.players[&2].percent, 0.0, "behind the attacker: no damage");
    }

    #[test]
    fn falling_into_blast_zone_kos_and_credits_last_hitter() {
        let mut room = Room::new();
        let mut victim = Player::new("v".into(), 0, 1200.0);
        victim.y = BLAST_D + 10.0; // below the lower blast zone
        victim.last_hit_by = Some(7);
        victim.last_hit_t = 10;
        room.players.insert(2, victim);
        room.players.insert(7, Player::new("killer".into(), 1, 1200.0));
        tick_room(&mut room);
        assert!(room.players[&2].respawn > 0, "victim is KO'd (respawning)");
        assert_eq!(room.players[&7].score, 1, "last hitter credited the KO");
        assert!(room.events.iter().any(|(c, _, _)| *c == 'k'), "KO event emitted");
    }
}
