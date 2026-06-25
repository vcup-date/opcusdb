//! opcusdb Townfall — a tiny 3D MMO-style town simulation (the authoritative
//! server for the **Godot** client in `demos/godot-wow`).
//!
//! A small town with **NPC quest givers**, a pack of **wolves** to kill in the
//! wilds, **quests** ("Cull the Wolves — slay 5"), and **chat**. Multiple players
//! connect (Godot, or any WebSocket client) and **see each other** move and fight
//! in one shared world. The Rust server owns the world (player movement, wolf AI,
//! combat, quests) at a fixed tick and broadcasts it over the hand-rolled
//! WebSocket (see [`ws`]).
//!
//! Run: `cargo run -p opcusdb-server --bin opcusdb-wow` (listens on :9007), then
//! open the Godot project in `demos/godot-wow` (or run two copies for multiplayer).

use opcusdb_core::Rng;
use opcusdb_server::ws;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const PORT: u16 = 9007;
const TICK_MS: u64 = 50; // 20 Hz
const DT: f32 = 0.05;
const WORLD: f32 = 30.0;
const SPAWN: (f32, f32) = (0.0, 7.0);

const PLAYER_SPEED: f32 = 7.0;
const PLAYER_HP: f32 = 100.0;
const PLAYER_DMG: f32 = 26.0;
const ATK_RANGE: f32 = 3.2;
const ATK_CD: f32 = 0.45;

const NUM_WOLVES: usize = 9;
const WOLF_HP: f32 = 55.0;
const WOLF_DMG: f32 = 9.0;
const WOLF_SPEED: f32 = 4.6;
const AGGRO: f32 = 10.0;
const BITE: f32 = 2.0;
const WOLF_ATK_CD: f32 = 1.0;
const WOLF_RESPAWN: f32 = 6.0;
const QUEST_GOAL: u32 = 5;

// action-bar skills (keys 1/2/3): Cleave (AoE), Fireball (ranged nuke), Heal
const SKILL_CD: [f32; 3] = [3.0, 7.0, 12.0];
const CLEAVE_R: f32 = 4.6;
const CLEAVE_DMG: f32 = 42.0;
const FIRE_RANGE: f32 = 16.0;
const FIRE_DMG: f32 = 85.0;
const HEAL_AMT: f32 = 55.0;

/// (name, x, z, is_quest_giver)
const NPCS: [(&str, f32, f32, bool); 2] = [("Mayor Bram", 0.0, -3.0, true), ("Scout Lyra", 8.0, 1.0, false)];

struct Player {
    name: String,
    x: f32,
    z: f32,
    facing: f32,
    hp: f32,
    quest: u8, // 0 none, 1 active, 2 ready-to-turn-in, 3 done
    prog: u32,
    kills: u32,
    atk_cd: f32,
    skill_cd: [f32; 3],
    w: bool,
    s: bool,
    a: bool,
    d: bool,
}
impl Player {
    fn new(name: String) -> Self {
        Self {
            name,
            x: SPAWN.0,
            z: SPAWN.1,
            facing: std::f32::consts::PI,
            hp: PLAYER_HP,
            quest: 0,
            prog: 0,
            kills: 0,
            atk_cd: 0.0,
            skill_cd: [0.0; 3],
            w: false,
            s: false,
            a: false,
            d: false,
        }
    }
}

struct Wolf {
    x: f32,
    z: f32,
    facing: f32,
    hp: f32,
    state: u8, // 0 wander, 1 chase, 2 dead
    home: (f32, f32),
    wander: (f32, f32),
    wander_cd: f32,
    atk_cd: f32,
    respawn: f32,
}

struct Room {
    players: BTreeMap<u32, Player>,
    wolves: BTreeMap<u32, Wolf>,
    chat: Vec<(String, String)>,
    events: Vec<(char, u32, f32, f32)>, // (kind, id, x, z): 's' swing, 'h' wolf hit, 'k' wolf death
    rng: Rng,
    time: f32,
    snapshot: String,
}

struct World {
    rooms: BTreeMap<String, Room>,
    next_id: u32,
}

fn main() {
    let world = Arc::new(Mutex::new(World { rooms: BTreeMap::new(), next_id: 1 }));
    {
        let world = world.clone();
        thread::spawn(move || loop {
            thread::sleep(Duration::from_millis(TICK_MS));
            let mut w = world.lock().unwrap();
            let codes: Vec<String> = w.rooms.keys().cloned().collect();
            for code in codes {
                let mut room = w.rooms.remove(&code).unwrap();
                tick_room(&mut room);
                room.snapshot = build_snapshot(&room);
                room.events.clear(); // events were just baked into the snapshot
                if !room.players.is_empty() {
                    w.rooms.insert(code, room);
                }
            }
        });
    }
    let listener = TcpListener::bind(("0.0.0.0", PORT)).expect("bind");
    println!("opcusdb Townfall on :{PORT}  — Godot MMO town (quests, wolves, chat, multiplayer)");
    for stream in listener.incoming().flatten() {
        let world = world.clone();
        thread::spawn(move || handle(stream, world));
    }
}

fn new_room(seed: u64) -> Room {
    let mut rng = Rng::seed(seed);
    let mut wolves = BTreeMap::new();
    for i in 0..NUM_WOLVES as u32 {
        // wolves roam the wilds to the north (negative z)
        let x = (rng.below(320) as f32) / 10.0 - 16.0;
        let z = -6.0 - (rng.below(170) as f32) / 10.0;
        wolves.insert(
            i + 1,
            Wolf {
                x,
                z,
                facing: 0.0,
                hp: WOLF_HP,
                state: 0,
                home: (x, z),
                wander: (x, z),
                wander_cd: 0.0,
                atk_cd: 0.0,
                respawn: 0.0,
            },
        );
    }
    Room { players: BTreeMap::new(), wolves, chat: Vec::new(), events: Vec::new(), rng, time: 0.0, snapshot: String::new() }
}

fn d2(ax: f32, az: f32, bx: f32, bz: f32) -> f32 {
    let (dx, dz) = (ax - bx, az - bz);
    dx * dx + dz * dz
}

// --- simulation ------------------------------------------------------------

fn tick_room(room: &mut Room) {
    room.time += DT;
    move_players(room);
    update_wolves(room);
}

fn move_players(room: &mut Room) {
    for p in room.players.values_mut() {
        if p.atk_cd > 0.0 {
            p.atk_cd -= DT;
        }
        for c in p.skill_cd.iter_mut() {
            if *c > 0.0 {
                *c = (*c - DT).max(0.0);
            }
        }
        let dx = (p.d as i32 - p.a as i32) as f32;
        let dz = (p.s as i32 - p.w as i32) as f32; // w = north = -z
        let len = (dx * dx + dz * dz).sqrt();
        if len > 0.0 {
            p.x = (p.x + dx / len * PLAYER_SPEED * DT).clamp(-WORLD, WORLD);
            p.z = (p.z + dz / len * PLAYER_SPEED * DT).clamp(-WORLD, WORLD);
            p.facing = dx.atan2(-dz);
        }
        if p.hp < PLAYER_HP {
            p.hp = (p.hp + 4.0 * DT).min(PLAYER_HP); // slow regen
        }
    }
}

fn update_wolves(room: &mut Room) {
    let pteam: Vec<(u32, f32, f32)> = room.players.values().zip(room.players.keys()).map(|(p, id)| (*id, p.x, p.z)).collect();
    let ids: Vec<u32> = room.wolves.keys().copied().collect();
    let mut bites: Vec<(u32, f32)> = Vec::new(); // (player_id, dmg)
    for id in ids {
        let w = room.wolves.get_mut(&id).unwrap();
        if w.state == 2 {
            w.respawn -= DT;
            if w.respawn <= 0.0 {
                w.x = w.home.0;
                w.z = w.home.1;
                w.hp = WOLF_HP;
                w.state = 0;
            }
            continue;
        }
        if w.atk_cd > 0.0 {
            w.atk_cd -= DT;
        }
        // nearest player
        let near = pteam.iter().min_by(|a, b| d2(w.x, w.z, a.1, a.2).total_cmp(&d2(w.x, w.z, b.1, b.2)));
        if let Some(&(pid, px, pz)) = near {
            if d2(w.x, w.z, px, pz) < AGGRO * AGGRO {
                w.state = 1;
                let dist = d2(w.x, w.z, px, pz).sqrt().max(0.001);
                w.facing = (px - w.x).atan2(-(pz - w.z));
                if dist > BITE {
                    w.x += (px - w.x) / dist * WOLF_SPEED * DT;
                    w.z += (pz - w.z) / dist * WOLF_SPEED * DT;
                } else if w.atk_cd <= 0.0 {
                    w.atk_cd = WOLF_ATK_CD;
                    bites.push((pid, WOLF_DMG));
                }
                continue;
            }
        }
        // wander near home
        w.state = 0;
        w.wander_cd -= DT;
        if w.wander_cd <= 0.0 {
            w.wander_cd = 2.0 + (room.rng.below(300) as f32) / 100.0;
            w.wander = (w.home.0 + (room.rng.below(120) as f32) / 10.0 - 6.0, w.home.1 + (room.rng.below(120) as f32) / 10.0 - 6.0);
        }
        let dist = d2(w.x, w.z, w.wander.0, w.wander.1).sqrt();
        if dist > 0.4 {
            w.facing = (w.wander.0 - w.x).atan2(-(w.wander.1 - w.z));
            w.x += (w.wander.0 - w.x) / dist * WOLF_SPEED * 0.5 * DT;
            w.z += (w.wander.1 - w.z) / dist * WOLF_SPEED * 0.5 * DT;
        }
    }
    for (pid, dmg) in bites {
        if let Some(p) = room.players.get_mut(&pid) {
            p.hp -= dmg;
            if p.hp <= 0.0 {
                // respawn in town
                p.hp = PLAYER_HP;
                p.x = SPAWN.0;
                p.z = SPAWN.1;
            }
        }
    }
}

/// Melee swing: hit the nearest living wolf in range; advances the quest.
fn player_attack(room: &mut Room, id: u32) {
    let (px, pz, ready) = match room.players.get(&id) {
        Some(p) => (p.x, p.z, p.atk_cd <= 0.0),
        None => return,
    };
    if !ready {
        return;
    }
    if let Some(p) = room.players.get_mut(&id) {
        p.atk_cd = ATK_CD;
    }
    room.events.push(('s', id, px, pz)); // swing (shown even on a miss)
    let target = room
        .wolves
        .iter()
        .filter(|(_, w)| w.state != 2 && d2(px, pz, w.x, w.z) < ATK_RANGE * ATK_RANGE)
        .min_by(|a, b| d2(px, pz, a.1.x, a.1.z).total_cmp(&d2(px, pz, b.1.x, b.1.z)))
        .map(|(wid, _)| *wid);
    if let Some(wid) = target {
        hurt_wolf(room, wid, PLAYER_DMG, id);
    }
}

/// Apply damage to a wolf; emits hit/death events and credits the caster's quest.
fn hurt_wolf(room: &mut Room, wid: u32, dmg: f32, caster: u32) {
    if room.wolves.get(&wid).map_or(true, |w| w.state == 2) {
        return;
    }
    let (wx, wz, dead) = {
        let w = room.wolves.get_mut(&wid).unwrap();
        w.hp -= dmg;
        (w.x, w.z, w.hp <= 0.0)
    };
    room.events.push(('h', wid, wx, wz));
    if dead {
        {
            let w = room.wolves.get_mut(&wid).unwrap();
            w.state = 2;
            w.respawn = WOLF_RESPAWN;
        }
        room.events.push(('k', wid, wx, wz));
        if let Some(p) = room.players.get_mut(&caster) {
            p.kills += 1;
            if p.quest == 1 {
                p.prog += 1;
                if p.prog >= QUEST_GOAL {
                    p.quest = 2;
                }
            }
        }
    }
}

/// Cast action-bar skill `n` (0 Cleave, 1 Fireball, 2 Heal) if off cooldown.
fn player_skill(room: &mut Room, id: u32, n: usize) {
    if n >= 3 {
        return;
    }
    let (px, pz, facing, ready) = match room.players.get(&id) {
        Some(p) => (p.x, p.z, p.facing, p.skill_cd[n] <= 0.0),
        None => return,
    };
    if !ready {
        return;
    }
    if let Some(p) = room.players.get_mut(&id) {
        p.skill_cd[n] = SKILL_CD[n];
    }
    match n {
        0 => {
            // Cleave — AoE around the player
            room.events.push(('C', id, px, pz));
            let hits: Vec<u32> = room
                .wolves
                .iter()
                .filter(|(_, w)| w.state != 2 && d2(px, pz, w.x, w.z) < CLEAVE_R * CLEAVE_R)
                .map(|(wid, _)| *wid)
                .collect();
            for wid in hits {
                hurt_wolf(room, wid, CLEAVE_DMG, id);
            }
        }
        1 => {
            // Fireball — nuke the nearest wolf in range
            let target = room
                .wolves
                .iter()
                .filter(|(_, w)| w.state != 2 && d2(px, pz, w.x, w.z) < FIRE_RANGE * FIRE_RANGE)
                .min_by(|a, b| d2(px, pz, a.1.x, a.1.z).total_cmp(&d2(px, pz, b.1.x, b.1.z)))
                .map(|(wid, _)| *wid);
            match target {
                Some(wid) => {
                    let (wx, wz) = {
                        let w = &room.wolves[&wid];
                        (w.x, w.z)
                    };
                    room.events.push(('F', id, wx, wz));
                    hurt_wolf(room, wid, FIRE_DMG, id);
                }
                None => {
                    room.events.push(('F', id, px + facing.sin() * 8.0, pz - facing.cos() * 8.0));
                }
            }
        }
        _ => {
            // Heal — restore HP
            if let Some(p) = room.players.get_mut(&id) {
                p.hp = (p.hp + HEAL_AMT).min(PLAYER_HP);
            }
            room.events.push(('L', id, px, pz));
        }
    }
}

/// Talk to the nearest NPC: accept / turn in / re-accept the quest.
fn player_interact(room: &mut Room, id: u32) -> Option<String> {
    let (px, pz) = room.players.get(&id).map(|p| (p.x, p.z))?;
    // nearest quest-giver NPC in range
    let giver = NPCS
        .iter()
        .filter(|(_, x, z, q)| *q && d2(px, pz, *x, *z) < 9.0)
        .min_by(|a, b| d2(px, pz, a.1, a.2).total_cmp(&d2(px, pz, b.1, b.2)));
    let (name, _, _, _) = giver?;
    let p = room.players.get_mut(&id)?;
    let line = match p.quest {
        0 | 3 => {
            p.quest = 1;
            p.prog = 0;
            format!("{name}: Wolves plague our wilds — slay {QUEST_GOAL} of them!")
        }
        1 => format!("{name}: You've felled {}/{QUEST_GOAL}. Keep at it!", p.prog),
        2 => {
            p.quest = 3;
            p.hp = PLAYER_HP;
            format!("{name}: The town thanks you, hero! Quest complete. (talk again to repeat)")
        }
        _ => return None,
    };
    room.chat.push(("System".to_string(), line.clone()));
    Some(line)
}

fn build_snapshot(room: &Room) -> String {
    let mut s = String::new();
    s.push_str(&format!("t\t{:.2}\n", room.time));
    for (i, (name, x, z, q)) in NPCS.iter().enumerate() {
        s.push_str(&format!("n\t{i}\t{name}\t{x:.2}\t{z:.2}\t{}\n", u8::from(*q)));
    }
    for (id, p) in &room.players {
        s.push_str(&format!(
            "p\t{id}\t{:.2}\t{:.2}\t{:.2}\t{:.0}\t{:.0}\t{}\t{}\t{}\t{}\t{:.1}\t{:.1}\t{:.1}\n",
            p.x, p.z, p.facing, p.hp, PLAYER_HP, p.quest, p.prog, p.kills, p.name, p.skill_cd[0], p.skill_cd[1], p.skill_cd[2]
        ));
    }
    for (id, w) in &room.wolves {
        s.push_str(&format!(
            "m\t{id}\t{:.2}\t{:.2}\t{:.2}\t{}\t{:.2}\n",
            w.x,
            w.z,
            w.facing,
            w.state,
            (w.hp / WOLF_HP).max(0.0)
        ));
    }
    let ev = room
        .events
        .iter()
        .map(|(k, id, x, z)| format!("{k}:{id}:{x:.2}:{z:.2}"))
        .collect::<Vec<_>>()
        .join(";");
    s.push_str(&format!("x\t{ev}\n"));
    s
}

// --- connections -----------------------------------------------------------

fn handle(mut stream: TcpStream, world: Arc<Mutex<World>>) {
    let Some(head) = read_http_head(&mut stream) else { return };
    if !head.to_ascii_lowercase().contains("upgrade: websocket") {
        let _ = stream.write_all(b"HTTP/1.1 426 Upgrade Required\r\nContent-Length: 0\r\n\r\n");
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
        let mut w = world.lock().unwrap();
        let id = w.next_id;
        w.next_id += 1;
        id
    };
    let my_room: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    let mut writer = stream.try_clone().expect("clone");
    let wworld = world.clone();
    let wroom = my_room.clone();
    let writer_handle = thread::spawn(move || {
        let mut chat_idx = 0usize;
        loop {
            thread::sleep(Duration::from_millis(TICK_MS));
            let (snap, chats) = {
                let code = wroom.lock().unwrap().clone();
                let w = wworld.lock().unwrap();
                match code.and_then(|c| w.rooms.get(&c).map(|r| (r.snapshot.clone(), r.chat.clone()))) {
                    Some(x) => x,
                    None => continue,
                }
            };
            if ws::write_text(&mut writer, &snap).is_err() {
                return;
            }
            if chats.len() > chat_idx {
                for (auth, text) in &chats[chat_idx..] {
                    if ws::write_text(&mut writer, &format!("c\t{auth}\t{text}")).is_err() {
                        return;
                    }
                }
                chat_idx = chats.len();
            }
        }
    });

    loop {
        match ws::read_frame(&mut stream) {
            Ok(Some(ws::Msg::Text(t))) => {
                let mut it = t.splitn(2, ' ');
                let cmd = it.next().unwrap_or("");
                let arg = it.next().unwrap_or("");
                match cmd {
                    "join" => {
                        let nick = clean_nick(arg, id);
                        let mut w = world.lock().unwrap();
                        let seed = now_nanos();
                        let room = w.rooms.entry("TOWN".to_string()).or_insert_with(|| new_room(seed));
                        room.players.insert(id, Player::new(nick.clone()));
                        room.chat.push(("System".to_string(), format!("{nick} entered Townfall")));
                        *my_room.lock().unwrap() = Some("TOWN".to_string());
                        let _ = ws::write_text(&mut stream, &format!("w\t{id}"));
                    }
                    "keys" => {
                        let b: Vec<&str> = arg.split_whitespace().collect();
                        if b.len() == 4 {
                            if let Some(code) = my_room.lock().unwrap().clone() {
                                let mut w = world.lock().unwrap();
                                if let Some(p) = w.rooms.get_mut(&code).and_then(|r| r.players.get_mut(&id)) {
                                    p.w = b[0] == "1";
                                    p.s = b[1] == "1";
                                    p.a = b[2] == "1";
                                    p.d = b[3] == "1";
                                }
                            }
                        }
                    }
                    "attack" => {
                        if let Some(code) = my_room.lock().unwrap().clone() {
                            let mut w = world.lock().unwrap();
                            if let Some(r) = w.rooms.get_mut(&code) {
                                player_attack(r, id);
                            }
                        }
                    }
                    "interact" => {
                        if let Some(code) = my_room.lock().unwrap().clone() {
                            let mut w = world.lock().unwrap();
                            if let Some(r) = w.rooms.get_mut(&code) {
                                player_interact(r, id);
                            }
                        }
                    }
                    "skill" => {
                        if let Ok(n) = arg.trim().parse::<usize>() {
                            if let Some(code) = my_room.lock().unwrap().clone() {
                                let mut w = world.lock().unwrap();
                                if let Some(r) = w.rooms.get_mut(&code) {
                                    player_skill(r, id, n);
                                }
                            }
                        }
                    }
                    "say" => {
                        let text = arg.trim();
                        if !text.is_empty() {
                            if let Some(code) = my_room.lock().unwrap().clone() {
                                let mut w = world.lock().unwrap();
                                if let Some(r) = w.rooms.get_mut(&code) {
                                    let name = r.players.get(&id).map(|p| p.name.clone()).unwrap_or_default();
                                    let text: String = text.chars().take(160).collect();
                                    r.chat.push((name, text));
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
        let mut w = world.lock().unwrap();
        if let Some(r) = w.rooms.get_mut(&code) {
            if let Some(p) = r.players.remove(&id) {
                r.chat.push(("System".to_string(), format!("{} left Townfall", p.name)));
            }
        }
    }
    drop(stream);
    let _ = writer_handle.join();
}

fn clean_nick(s: &str, id: u32) -> String {
    let n: String = s.chars().filter(|c| !c.is_whitespace()).take(14).collect();
    if n.is_empty() { format!("Hero{id}") } else { n }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn room1() -> Room {
        let mut r = new_room(1);
        r.wolves.clear();
        r.players.insert(1, Player::new("hero".into()));
        r
    }

    #[test]
    fn attack_kills_wolf_and_advances_quest() {
        let mut r = room1();
        r.players.get_mut(&1).unwrap().quest = 1; // active
        r.wolves.insert(
            5,
            Wolf { x: 1.0, z: 7.0, facing: 0.0, hp: 10.0, state: 0, home: (1.0, 7.0), wander: (1.0, 7.0), wander_cd: 0.0, atk_cd: 0.0, respawn: 0.0 },
        );
        player_attack(&mut r, 1);
        assert_eq!(r.wolves[&5].state, 2, "wolf is dead");
        assert_eq!(r.players[&1].kills, 1, "credited the kill");
        assert_eq!(r.players[&1].prog, 1, "quest progressed");
    }

    #[test]
    fn quest_completes_after_goal_then_turn_in() {
        let mut r = room1();
        {
            let p = r.players.get_mut(&1).unwrap();
            p.quest = 1;
            p.prog = QUEST_GOAL - 1;
            p.x = NPCS[0].1;
            p.z = NPCS[0].2; // stand on Mayor Bram
        }
        r.wolves.insert(
            5,
            Wolf { x: NPCS[0].1, z: NPCS[0].2, facing: 0.0, hp: 5.0, state: 0, home: (0.0, 0.0), wander: (0.0, 0.0), wander_cd: 0.0, atk_cd: 0.0, respawn: 0.0 },
        );
        player_attack(&mut r, 1);
        assert_eq!(r.players[&1].quest, 2, "ready to turn in after reaching the goal");
        player_interact(&mut r, 1);
        assert_eq!(r.players[&1].quest, 3, "turned in -> done");
    }

    #[test]
    fn interact_accepts_quest_near_npc() {
        let mut r = room1();
        {
            let p = r.players.get_mut(&1).unwrap();
            p.x = NPCS[0].1;
            p.z = NPCS[0].2;
        }
        player_interact(&mut r, 1);
        assert_eq!(r.players[&1].quest, 1, "accepted the quest from the giver");
    }

    #[test]
    fn cleave_skill_hits_nearby_wolves_and_sets_cooldown() {
        let mut r = room1();
        r.players.get_mut(&1).unwrap().quest = 1;
        for i in 0..3u32 {
            r.wolves.insert(
                10 + i,
                Wolf { x: SPAWN.0 + i as f32 * 0.6, z: SPAWN.1, facing: 0.0, hp: 12.0, state: 0, home: (0.0, 0.0), wander: (0.0, 0.0), wander_cd: 0.0, atk_cd: 0.0, respawn: 0.0 },
            );
        }
        player_skill(&mut r, 1, 0); // Cleave
        assert_eq!(r.wolves.values().filter(|w| w.state != 2).count(), 0, "cleave killed all nearby wolves");
        assert!(r.players[&1].skill_cd[0] > 0.0, "cleave went on cooldown");
        // recasting while on cooldown does nothing
        r.wolves.insert(99, Wolf { x: SPAWN.0, z: SPAWN.1, facing: 0.0, hp: 12.0, state: 0, home: (0.0, 0.0), wander: (0.0, 0.0), wander_cd: 0.0, atk_cd: 0.0, respawn: 0.0 });
        player_skill(&mut r, 1, 0);
        assert_eq!(r.wolves[&99].state, 0, "on cooldown: cleave did not fire");
    }

    #[test]
    fn wolf_bites_player_when_adjacent() {
        let mut r = room1();
        r.players.get_mut(&1).unwrap().x = 0.0;
        r.players.get_mut(&1).unwrap().z = 0.0;
        r.wolves.insert(
            5,
            Wolf { x: 1.0, z: 0.0, facing: 0.0, hp: WOLF_HP, state: 1, home: (1.0, 0.0), wander: (1.0, 0.0), wander_cd: 0.0, atk_cd: 0.0, respawn: 0.0 },
        );
        update_wolves(&mut r);
        assert!(r.players[&1].hp < PLAYER_HP, "wolf bit the player");
    }
}
