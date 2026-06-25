//! opcusdb Arena, a real-time multiplayer **Snake** game.
//!
//! Players create/join a **room** by code, steer with the arrow keys, eat food to
//! grow and score, and die on collisions (then auto-respawn). The server is
//! authoritative: it runs every room's grid at a fixed tick and broadcasts state
//! to that room's clients over the hand-rolled WebSocket (see [`ws`]).
//!
//! Scores persist: the all-time top-10 leaderboard is stored in a small **local
//! DB file** (`leaderboard.db`, gitignored), loaded on boot and saved on change.
//!
//! Run: `cargo run -p opcusdb-server --bin opcusdb-arena` then open
//! http://localhost:9003, create a room, share the code, race your friends.

use opcusdb_core::Rng;
use opcusdb_server::ws;
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const PORT: u16 = 9003;
const GRID: i32 = 32;
const TICK_MS: u64 = 130; // ~7.7 ticks/sec, classic snake feel
const FOOD_PER_ROOM: usize = 4;
const RESPAWN_TICKS: u64 = 12; // ~1.5s
const LB_PATH: &str = "leaderboard.db";
const LB_MAX: usize = 10;

const DIRS: [(i32, i32); 4] = [(0, -1), (0, 1), (-1, 0), (1, 0)]; // u d l r

struct Snake {
    nick: String,
    color: u8,
    body: VecDeque<(i32, i32)>, // front = head
    dir: (i32, i32),
    pending: (i32, i32),
    alive: bool,
    score: u32,
    respawn_at: u64,
}

struct Room {
    snakes: BTreeMap<u32, Snake>,
    food: Vec<(i32, i32)>,
    rng: Rng,
    tick: u64,
    snapshot: String,
}

struct Arena {
    rooms: BTreeMap<String, Room>,
    leaderboard: Vec<(String, u32)>, // sorted desc, top LB_MAX
    next_id: u32,
}

fn main() {
    let arena = Arc::new(Mutex::new(Arena {
        rooms: BTreeMap::new(),
        leaderboard: load_leaderboard(),
        next_id: 1,
    }));

    // Game loop: tick every room, rebuild its broadcast snapshot.
    {
        let arena = arena.clone();
        thread::spawn(move || loop {
            thread::sleep(Duration::from_millis(TICK_MS));
            let mut a = arena.lock().unwrap();
            let codes: Vec<String> = a.rooms.keys().cloned().collect();
            let mut lb_changed = false;
            for code in codes {
                // borrow split: take room out, tick, put back
                let mut room = a.rooms.remove(&code).unwrap();
                lb_changed |= tick_room(&mut room, &mut a.leaderboard);
                room.snapshot = build_snapshot(&room, &code, &a.leaderboard);
                if room.snakes.is_empty() {
                    continue; // drop empty rooms
                }
                a.rooms.insert(code, room);
            }
            if lb_changed {
                save_leaderboard(&a.leaderboard);
            }
        });
    }

    let listener = TcpListener::bind(("0.0.0.0", PORT)).expect("bind");
    println!("opcusdb Arena (snake) on http://localhost:{PORT} , create/join a room and play");
    for stream in listener.incoming().flatten() {
        let arena = arena.clone();
        thread::spawn(move || handle(stream, arena));
    }
}

// --- game rules ------------------------------------------------------------

fn spawn_snake(room: &mut Room, id: u32, nick: String, color: u8) {
    let occ = occupied(room);
    let mut head = (GRID / 2, GRID / 2);
    for _ in 0..200 {
        let c = (room.rng.range(3, (GRID - 3) as u32) as i32, room.rng.range(3, (GRID - 3) as u32) as i32);
        if !occ.contains(&c) {
            head = c;
            break;
        }
    }
    let dir = DIRS[room.rng.below(4) as usize];
    let mut body = VecDeque::new();
    for i in 0..3 {
        body.push_back((head.0 - dir.0 * i, head.1 - dir.1 * i));
    }
    room.snakes.insert(
        id,
        Snake { nick, color, body, dir, pending: dir, alive: true, score: 0, respawn_at: 0 },
    );
}

fn occupied(room: &Room) -> HashSet<(i32, i32)> {
    let mut s = HashSet::new();
    for sn in room.snakes.values() {
        if sn.alive {
            s.extend(sn.body.iter().copied());
        }
    }
    s
}

/// Advance a room one tick. Returns whether the leaderboard changed.
fn tick_room(room: &mut Room, leaderboard: &mut Vec<(String, u32)>) -> bool {
    room.tick += 1;
    let mut lb_changed = false;

    // auto-respawn the dead whose timer elapsed
    let dead: Vec<u32> = room
        .snakes
        .iter()
        .filter(|(_, s)| !s.alive && room.tick >= s.respawn_at)
        .map(|(id, _)| *id)
        .collect();
    for id in dead {
        let (nick, color) = {
            let s = &room.snakes[&id];
            (s.nick.clone(), s.color)
        };
        room.snakes.remove(&id);
        spawn_snake(room, id, nick, color);
    }

    // bodies that block movement this tick (current alive bodies)
    let blocked = occupied(room);

    // compute intended new heads for alive snakes
    let mut new_heads: BTreeMap<u32, (i32, i32)> = BTreeMap::new();
    for (id, s) in room.snakes.iter_mut() {
        if !s.alive {
            continue;
        }
        // apply buffered turn unless it's a 180° reversal
        if s.pending != (-s.dir.0, -s.dir.1) {
            s.dir = s.pending;
        }
        let head = *s.body.front().unwrap();
        new_heads.insert(*id, (head.0 + s.dir.0, head.1 + s.dir.1));
    }

    // resolve collisions
    let mut killed: Vec<u32> = Vec::new();
    for (id, &nh) in &new_heads {
        let oob = nh.0 < 0 || nh.1 < 0 || nh.0 >= GRID || nh.1 >= GRID;
        let hit_body = blocked.contains(&nh);
        // head-to-head: another snake moving into the same cell
        let head_clash = new_heads.iter().any(|(oid, &oh)| oid != id && oh == nh);
        if oob || hit_body || head_clash {
            killed.push(*id);
        }
    }

    // apply: kill, or move/grow survivors
    for (id, nh) in new_heads {
        if killed.contains(&id) {
            let s = room.snakes.get_mut(&id).unwrap();
            s.alive = false;
            s.respawn_at = room.tick + RESPAWN_TICKS;
            if record_score(leaderboard, &s.nick, s.score) {
                lb_changed = true;
            }
            s.body.clear();
            continue;
        }
        let ate = room.food.iter().position(|&f| f == nh);
        let s = room.snakes.get_mut(&id).unwrap();
        s.body.push_front(nh);
        if let Some(fi) = ate {
            room.food.swap_remove(fi);
            s.score += 1;
        } else {
            s.body.pop_back();
        }
    }

    // replenish food
    while room.food.len() < FOOD_PER_ROOM {
        let occ = occupied(room);
        let mut placed = false;
        for _ in 0..100 {
            let c = (room.rng.below(GRID as u32) as i32, room.rng.below(GRID as u32) as i32);
            if !occ.contains(&c) && !room.food.contains(&c) {
                room.food.push(c);
                placed = true;
                break;
            }
        }
        if !placed {
            break;
        }
    }
    lb_changed
}

fn record_score(lb: &mut Vec<(String, u32)>, nick: &str, score: u32) -> bool {
    if score == 0 {
        return false;
    }
    // keep each player's best
    if let Some(e) = lb.iter_mut().find(|(n, _)| n == nick) {
        if score > e.1 {
            e.1 = score;
        } else {
            return false;
        }
    } else {
        lb.push((nick.to_string(), score));
    }
    lb.sort_by_key(|e| std::cmp::Reverse(e.1));
    lb.truncate(LB_MAX);
    true
}

fn build_snapshot(room: &Room, code: &str, lb: &[(String, u32)]) -> String {
    let mut s = String::new();
    s.push_str(&format!("g\t{GRID}\t{GRID}\t{code}\n"));
    let food = room.food.iter().map(|(x, y)| format!("{x},{y}")).collect::<Vec<_>>().join(";");
    s.push_str(&format!("f\t{food}\n"));
    for (id, sn) in &room.snakes {
        let body = sn.body.iter().map(|(x, y)| format!("{x},{y}")).collect::<Vec<_>>().join(";");
        s.push_str(&format!(
            "s\t{id}\t{}\t{}\t{}\t{}\t{}\n",
            sn.color,
            u8::from(sn.alive),
            sn.score,
            sn.nick,
            body
        ));
    }
    let lb_s = lb.iter().map(|(n, sc)| format!("{n}:{sc}")).collect::<Vec<_>>().join(",");
    s.push_str(&format!("l\t{lb_s}\n"));
    s
}

// --- persistence (the "local DB") ------------------------------------------

fn load_leaderboard() -> Vec<(String, u32)> {
    std::fs::read_to_string(LB_PATH)
        .map(|t| {
            t.lines()
                .filter_map(|l| {
                    let (n, s) = l.rsplit_once(' ')?;
                    Some((n.to_string(), s.trim().parse().ok()?))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn save_leaderboard(lb: &[(String, u32)]) {
    let body: String = lb.iter().map(|(n, s)| format!("{n} {s}\n")).collect();
    let _ = std::fs::write(LB_PATH, body);
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
    let _ = ws::write_text(&mut stream, &format!("w {id}"));

    // this client's current room (set on join), shared with the writer thread
    let my_room: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    // Writer: stream this client's room snapshot.
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

    // Reader: join + steering.
    loop {
        match ws::read_frame(&mut stream) {
            Ok(Some(ws::Msg::Text(t))) => {
                let parts: Vec<&str> = t.split_whitespace().collect();
                match parts.as_slice() {
                    ["join", code, nick] => {
                        let code = clean_code(code);
                        let nick = clean_nick(nick, id);
                        let mut a = arena.lock().unwrap();
                        let seed = now_nanos() ^ (id as u64);
                        let room = a.rooms.entry(code.clone()).or_insert_with(|| Room {
                            snakes: BTreeMap::new(),
                            food: Vec::new(),
                            rng: Rng::seed(seed),
                            tick: 0,
                            snapshot: String::new(),
                        });
                        let color = (id % 6) as u8;
                        spawn_snake(room, id, nick, color);
                        *my_room.lock().unwrap() = Some(code);
                    }
                    ["dir", d] => {
                        let nd = match *d {
                            "u" => DIRS[0],
                            "d" => DIRS[1],
                            "l" => DIRS[2],
                            "r" => DIRS[3],
                            _ => continue,
                        };
                        if let Some(code) = my_room.lock().unwrap().clone() {
                            let mut a = arena.lock().unwrap();
                            if let Some(s) = a.rooms.get_mut(&code).and_then(|r| r.snakes.get_mut(&id)) {
                                s.pending = nd;
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

    // disconnect: remove snake
    if let Some(code) = my_room.lock().unwrap().clone() {
        let mut a = arena.lock().unwrap();
        if let Some(r) = a.rooms.get_mut(&code) {
            r.snakes.remove(&id);
        }
    }
    drop(stream);
    let _ = writer_handle.join();
}

fn clean_code(s: &str) -> String {
    let c: String = s.chars().filter(|c| c.is_ascii_alphanumeric()).take(6).collect::<String>().to_uppercase();
    if c.is_empty() {
        "LOBBY".to_string()
    } else {
        c
    }
}

fn clean_nick(s: &str, id: u32) -> String {
    let n: String = s.chars().filter(|c| !c.is_whitespace()).take(14).collect();
    if n.is_empty() {
        format!("p{id}")
    } else {
        n
    }
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
        "/" | "/index.html" => ("text/html; charset=utf-8", include_str!("../web/arena.html").as_bytes()),
        "/arena.js" => ("application/javascript; charset=utf-8", include_str!("../web/arena.js").as_bytes()),
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

    fn empty_room() -> Room {
        Room { snakes: BTreeMap::new(), food: Vec::new(), rng: Rng::seed(1), tick: 0, snapshot: String::new() }
    }

    #[test]
    fn snake_moves_and_eats() {
        let mut room = empty_room();
        spawn_snake(&mut room, 1, "a".into(), 0);
        // force a known state: head right at (5,5), food directly ahead
        let s = room.snakes.get_mut(&1).unwrap();
        s.body = VecDeque::from([(5, 5), (4, 5), (3, 5)]);
        s.dir = (1, 0);
        s.pending = (1, 0);
        room.food = vec![(6, 5)];
        let mut lb = Vec::new();
        tick_room(&mut room, &mut lb);
        let s = &room.snakes[&1];
        assert_eq!(*s.body.front().unwrap(), (6, 5), "moved onto the food");
        assert_eq!(s.score, 1, "ate -> scored");
        assert_eq!(s.body.len(), 4, "grew by one");
    }

    #[test]
    fn wall_collision_kills_and_records_score() {
        let mut room = empty_room();
        spawn_snake(&mut room, 1, "wally".into(), 0);
        let s = room.snakes.get_mut(&1).unwrap();
        s.body = VecDeque::from([(GRID - 1, 5), (GRID - 2, 5), (GRID - 3, 5)]);
        s.dir = (1, 0);
        s.pending = (1, 0);
        s.score = 7;
        let mut lb = Vec::new();
        tick_room(&mut room, &mut lb);
        assert!(!room.snakes[&1].alive, "ran into the wall");
        assert_eq!(lb.first(), Some(&("wally".to_string(), 7)), "score recorded to leaderboard");
    }

    #[test]
    fn leaderboard_keeps_personal_best_sorted() {
        let mut lb = Vec::new();
        assert!(record_score(&mut lb, "a", 5));
        assert!(record_score(&mut lb, "b", 9));
        assert!(!record_score(&mut lb, "a", 3), "lower than personal best -> no change");
        assert!(record_score(&mut lb, "a", 12), "new personal best");
        assert_eq!(lb[0], ("a".into(), 12));
        assert_eq!(lb[1], ("b".into(), 9));
    }
}
