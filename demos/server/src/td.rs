//! opcusdb Rampart — a **tower defense**. Creeps march along a winding path in
//! escalating waves; you spend gold to build towers (arrow / cannon / frost) that
//! auto-target and fire. Let a creep reach your keep and you lose a life; survive
//! every wave to win. The Rust server is the authoritative simulation (fixed tick,
//! broadcast over WebSocket); **each browser gets its own private game** so nobody
//! else's actions touch yours. All mouse: click a tower, click a tile.
//!
//! Run: `cargo run -p opcusdb-server --bin opcusdb-td` then open http://localhost:9010

use opcusdb_server::ws;
use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const PORT: u16 = 9010;
const COLS: i32 = 20;
const ROWS: i32 = 12;
const TILE: f32 = 48.0;
const TICK_MS: u64 = 33; // ~30 Hz
const DT: f32 = 0.033;
const MAX_WAVE: i32 = 12;

// tower kinds: 0 arrow, 1 cannon, 2 frost
const T_COST: [i32; 3] = [50, 110, 75];
const T_RANGE: [f32; 3] = [120.0, 135.0, 105.0];
const T_DMG: [f32; 3] = [6.0, 20.0, 3.0];
const T_CD: [f32; 3] = [0.40, 1.25, 0.55];
const T_SPLASH: [f32; 3] = [0.0, 48.0, 0.0];
const PROJ_SPD: [f32; 3] = [520.0, 320.0, 460.0];

struct Enemy {
    x: f32,
    y: f32,
    hp: f32,
    max: f32,
    speed: f32,
    seg: usize,
    kind: u8,
    slow: f32,
    bounty: i32,
}
struct Tower {
    x: f32,
    y: f32,
    tx: i32,
    ty: i32,
    kind: u8,
    cd: f32,
}
struct Proj {
    x: f32,
    y: f32,
    target: u32,
    kind: u8,
    dmg: f32,
}

struct Game {
    enemies: BTreeMap<u32, Enemy>,
    towers: BTreeMap<u32, Tower>,
    projs: BTreeMap<u32, Proj>,
    path: Vec<(f32, f32)>, // waypoint pixel centres
    road: Vec<bool>,       // road[r*COLS+c]
    base: (i32, i32),
    gold: i32,
    lives: i32,
    wave: i32,
    state: u8, // 0 build, 1 wave, 2 win, 3 lose
    to_spawn: Vec<(u8, f32, f32, i32)>, // (kind, hp, speed, bounty) queued for this wave
    spawn_cd: f32,
    next: u32,
}

fn idx(c: i32, r: i32) -> usize {
    (r * COLS + c) as usize
}
fn center(c: i32, r: i32) -> (f32, f32) {
    ((c as f32 + 0.5) * TILE, (r as f32 + 0.5) * TILE)
}

fn new_game() -> Game {
    // winding path in tile coords (axis-aligned segments)
    let wp = [(0, 1), (17, 1), (17, 4), (2, 4), (2, 7), (17, 7), (17, 10), (19, 10)];
    let mut road = vec![false; (COLS * ROWS) as usize];
    let mut path = Vec::new();
    for w in &wp {
        path.push(center(w.0, w.1));
    }
    for pair in wp.windows(2) {
        let (c0, r0) = pair[0];
        let (c1, r1) = pair[1];
        let (dc, dr) = ((c1 - c0).signum(), (r1 - r0).signum());
        let (mut c, mut r) = (c0, r0);
        loop {
            if (0..COLS).contains(&c) && (0..ROWS).contains(&r) {
                road[idx(c, r)] = true;
            }
            if c == c1 && r == r1 {
                break;
            }
            c += dc;
            r += dr;
        }
    }
    Game {
        enemies: BTreeMap::new(),
        towers: BTreeMap::new(),
        projs: BTreeMap::new(),
        path,
        road,
        base: (19, 10),
        gold: 220,
        lives: 20,
        wave: 0,
        state: 0,
        to_spawn: Vec::new(),
        spawn_cd: 0.0,
        next: 1,
    }
}

fn start_wave(g: &mut Game) {
    // can start the next wave whenever the game is live and not all waves are out —
    // including DURING a wave (call the next one early), so the button is never stuck.
    if g.state >= 2 || g.wave >= MAX_WAVE {
        return;
    }
    g.wave += 1;
    let w = g.wave;
    let count = 6 + w * 2;
    let base_hp = 18.0 + w as f32 * 7.0;
    let mut q = Vec::new();
    for i in 0..count {
        let (kind, hp, speed) = if w % 4 == 0 && i % 5 == 0 {
            (2u8, base_hp * 3.2, 34.0) // tank
        } else if i % 3 == 0 {
            (1u8, base_hp * 0.6, 98.0) // fast
        } else {
            (0u8, base_hp, 56.0) // normal
        };
        q.push((kind, hp, speed, 5 + w));
    }
    if g.to_spawn.is_empty() {
        g.spawn_cd = 0.3;
    }
    g.to_spawn.extend(q);
    g.state = 1;
}

fn place_tower(g: &mut Game, kind: u8, c: i32, r: i32) {
    if kind > 2 || !(0..COLS).contains(&c) || !(0..ROWS).contains(&r) {
        return;
    }
    if g.road[idx(c, r)] || g.gold < T_COST[kind as usize] {
        return;
    }
    if g.towers.values().any(|t| t.tx == c && t.ty == r) {
        return;
    }
    let (x, y) = center(c, r);
    g.gold -= T_COST[kind as usize];
    let id = g.next;
    g.next += 1;
    g.towers.insert(id, Tower { x, y, tx: c, ty: r, kind, cd: 0.0 });
}

fn tick(g: &mut Game) {
    if g.state >= 2 {
        return;
    }
    // spawn
    if g.state == 1 && !g.to_spawn.is_empty() {
        g.spawn_cd -= DT;
        if g.spawn_cd <= 0.0 {
            let (kind, hp, speed, bounty) = g.to_spawn.remove(0);
            let (x, y) = g.path[0];
            let id = g.next;
            g.next += 1;
            g.enemies.insert(id, Enemy { x, y, hp, max: hp, speed, seg: 1, kind, slow: 0.0, bounty });
            g.spawn_cd = 0.55;
        }
    }
    // move enemies along the path
    let mut leaked = Vec::new();
    for (id, e) in g.enemies.iter_mut() {
        if e.slow > 0.0 {
            e.slow -= DT;
        }
        let spd = if e.slow > 0.0 { e.speed * 0.45 } else { e.speed };
        let mut step = spd * DT;
        while step > 0.0 && e.seg < g.path.len() {
            let (tx, ty) = g.path[e.seg];
            let (dx, dy) = (tx - e.x, ty - e.y);
            let d = (dx * dx + dy * dy).sqrt();
            if d <= step {
                e.x = tx;
                e.y = ty;
                e.seg += 1;
                step -= d;
            } else {
                e.x += dx / d * step;
                e.y += dy / d * step;
                step = 0.0;
            }
        }
        if e.seg >= g.path.len() {
            leaked.push(*id);
        }
    }
    for id in leaked {
        g.enemies.remove(&id);
        g.lives -= 1;
    }
    // towers fire
    let mut new_projs = Vec::new();
    for t in g.towers.values_mut() {
        t.cd -= DT;
        if t.cd > 0.0 {
            continue;
        }
        let range = T_RANGE[t.kind as usize];
        // target the enemy furthest along the path within range
        let mut best: Option<(u32, usize)> = None;
        for (eid, e) in &g.enemies {
            let (dx, dy) = (e.x - t.x, e.y - t.y);
            if dx * dx + dy * dy <= range * range && best.map_or(true, |(_, bs)| e.seg > bs) {
                best = Some((*eid, e.seg));
            }
        }
        if let Some((eid, _)) = best {
            t.cd = T_CD[t.kind as usize];
            new_projs.push(Proj { x: t.x, y: t.y, target: eid, kind: t.kind, dmg: T_DMG[t.kind as usize] });
        }
    }
    for p in new_projs {
        let id = g.next;
        g.next += 1;
        g.projs.insert(id, p);
    }
    // move projectiles; resolve hits
    let mut hits: Vec<(f32, f32, u8, f32)> = Vec::new(); // x,y,kind,dmg
    let mut done = Vec::new();
    for (pid, p) in g.projs.iter_mut() {
        let tgt = g.enemies.get(&p.target);
        let (tx, ty) = match tgt {
            Some(e) => (e.x, e.y),
            None => {
                done.push(*pid);
                continue;
            }
        };
        let (dx, dy) = (tx - p.x, ty - p.y);
        let d = (dx * dx + dy * dy).sqrt();
        let step = PROJ_SPD[p.kind as usize] * DT;
        if d <= step + 6.0 {
            hits.push((tx, ty, p.kind, p.dmg));
            done.push(*pid);
        } else {
            p.x += dx / d * step;
            p.y += dy / d * step;
        }
    }
    for pid in done {
        g.projs.remove(&pid);
    }
    // apply hits (splash + slow)
    let mut killed = Vec::new();
    for (hx, hy, kind, dmg) in hits {
        let splash = T_SPLASH[kind as usize];
        for (eid, e) in g.enemies.iter_mut() {
            let near = if splash > 0.0 {
                let (dx, dy) = (e.x - hx, e.y - hy);
                dx * dx + dy * dy <= splash * splash
            } else {
                (e.x - hx).abs() < 0.5 && (e.y - hy).abs() < 0.5
            };
            if near {
                e.hp -= dmg;
                if kind == 2 {
                    e.slow = 1.5;
                }
                if e.hp <= 0.0 {
                    killed.push((*eid, e.bounty));
                }
            }
        }
    }
    for (id, bounty) in killed {
        if g.enemies.remove(&id).is_some() {
            g.gold += bounty;
        }
    }
    // wave / game state
    if g.lives <= 0 {
        g.state = 3;
    } else if g.state == 1 && g.to_spawn.is_empty() && g.enemies.is_empty() {
        g.state = if g.wave >= MAX_WAVE { 2 } else { 0 };
    }
}

fn map_line(g: &Game) -> String {
    let mut roads = String::new();
    for r in 0..ROWS {
        for c in 0..COLS {
            if g.road[idx(c, r)] {
                roads.push_str(&format!("{c},{r};"));
            }
        }
    }
    let wps = g.path.iter().map(|(x, y)| format!("{x:.0},{y:.0}")).collect::<Vec<_>>().join(";");
    format!("map\t{COLS}\t{ROWS}\t{}\t{roads}\t{},{}\t{wps}\n", TILE as i32, g.base.0, g.base.1)
}

fn snapshot(g: &Game) -> String {
    let mut s = format!("s\t{}\t{}\t{}\t{}\t{}\n", g.gold, g.lives, g.wave, MAX_WAVE, g.state);
    let e: String = g
        .enemies
        .iter()
        .map(|(id, e)| format!("{id},{:.0},{:.0},{},{},{}", e.x, e.y, (e.hp / e.max * 10.0).ceil() as i32, e.kind, if e.slow > 0.0 { 1 } else { 0 }))
        .collect::<Vec<_>>()
        .join(";");
    s.push_str(&format!("e\t{e}\n"));
    let t: String = g.towers.values().map(|t| format!("{},{},{}", t.tx, t.ty, t.kind)).collect::<Vec<_>>().join(";");
    s.push_str(&format!("t\t{t}\n"));
    let p: String = g.projs.values().map(|p| format!("{:.0},{:.0},{}", p.x, p.y, p.kind)).collect::<Vec<_>>().join(";");
    s.push_str(&format!("p\t{p}\n"));
    s
}

/// A room is one shared game plus a live client count.
struct Room {
    game: Arc<Mutex<Game>>,
    clients: usize,
}
type Rooms = Arc<Mutex<HashMap<String, Room>>>;
static PRIV: AtomicU64 = AtomicU64::new(1);

fn main() {
    let rooms: Rooms = Arc::new(Mutex::new(HashMap::new()));
    let listener = TcpListener::bind(("0.0.0.0", PORT)).expect("bind");
    println!("opcusdb Rampart (tower defense) on http://localhost:{PORT}");
    for stream in listener.incoming().flatten() {
        let rooms = rooms.clone();
        thread::spawn(move || handle(stream, rooms));
    }
}

/// Pull a `?room=CODE` out of the request line, sanitised.
fn room_code(head: &str) -> Option<String> {
    let path = head.lines().next()?.split_whitespace().nth(1)?;
    let code: String = path
        .split_once("room=")?
        .1
        .split('&')
        .next()
        .unwrap_or("")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(12)
        .collect();
    (!code.is_empty()).then_some(code)
}

fn handle(mut stream: TcpStream, rooms: Rooms) {
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

    // shared room if a ?room=CODE was given, otherwise a fresh private room
    let rkey = match room_code(&head) {
        Some(c) => format!("r_{c}"),
        None => format!("_p{}", PRIV.fetch_add(1, Ordering::Relaxed)),
    };
    let game = {
        let mut rs = rooms.lock().unwrap();
        let new_room = !rs.contains_key(&rkey);
        let room = rs.entry(rkey.clone()).or_insert_with(|| Room { game: Arc::new(Mutex::new(new_game())), clients: 0 });
        room.clients += 1;
        let game = room.game.clone();
        if new_room {
            // one ticker per room; it removes the room (and stops) once empty
            let rooms = rooms.clone();
            let key = rkey.clone();
            thread::spawn(move || loop {
                thread::sleep(Duration::from_millis(TICK_MS));
                let g = {
                    let mut rs = rooms.lock().unwrap();
                    match rs.get(&key) {
                        Some(r) if r.clients > 0 => Some(r.game.clone()),
                        _ => {
                            rs.remove(&key);
                            None
                        }
                    }
                };
                match g {
                    Some(g) => tick(&mut g.lock().unwrap()),
                    None => break,
                }
            });
        }
        game
    };

    // writer thread: map once, then state + player-count each tick (does NOT tick the sim)
    let mut writer = stream.try_clone().expect("clone");
    let wgame = game.clone();
    let wrooms = rooms.clone();
    let wkey = rkey.clone();
    let writer_handle = thread::spawn(move || {
        if ws::write_text(&mut writer, &map_line(&wgame.lock().unwrap())).is_err() {
            return;
        }
        loop {
            thread::sleep(Duration::from_millis(TICK_MS));
            let players = wrooms.lock().unwrap().get(&wkey).map_or(1, |r| r.clients);
            let snap = format!("{}n\t{players}\n", snapshot(&wgame.lock().unwrap()));
            if ws::write_text(&mut writer, &snap).is_err() {
                return;
            }
        }
    });

    loop {
        match ws::read_frame(&mut stream) {
            Ok(Some(ws::Msg::Text(t))) => {
                let (cmd, rest) = t.split_once(' ').unwrap_or((t.as_str(), ""));
                match cmd {
                    "place" => {
                        let v: Vec<i32> = rest.split_whitespace().filter_map(|s| s.parse().ok()).collect();
                        if v.len() == 3 {
                            place_tower(&mut game.lock().unwrap(), v[0] as u8, v[1], v[2]);
                        }
                    }
                    "wave" => start_wave(&mut game.lock().unwrap()),
                    "reset" => *game.lock().unwrap() = new_game(),
                    _ => {}
                }
            }
            Ok(Some(ws::Msg::Other)) => {}
            _ => break,
        }
    }
    // leave the room
    if let Some(r) = rooms.lock().unwrap().get_mut(&rkey) {
        r.clients = r.clients.saturating_sub(1);
    }
    drop(stream);
    let _ = writer_handle.join();
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
    // Serve ONE self-contained document: the JS is inlined into the HTML so a
    // browser can never end up with a mismatched cached html+js pair.
    let (ctype, body): (&str, Vec<u8>) = match path {
        "/" | "/index.html" => {
            let html = include_str!("../web/td.html")
                .replace("<script src=\"/td.js\"></script>", &format!("<script>\n{}\n</script>", include_str!("../web/td.js")));
            ("text/html; charset=utf-8", html.into_bytes())
        }
        "/td.js" => ("application/javascript; charset=utf-8", include_str!("../web/td.js").as_bytes().to_vec()),
        _ => {
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
            return;
        }
    };
    let resp = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {ctype}\r\nCache-Control: no-store, no-cache, must-revalidate\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(resp.as_bytes());
    let _ = stream.write_all(&body);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placing_a_tower_costs_gold_and_rejects_the_road() {
        let mut g = new_game();
        let g0 = g.gold;
        place_tower(&mut g, 0, 5, 0); // row 0 is off the path -> buildable
        assert_eq!(g.towers.len(), 1);
        assert_eq!(g.gold, g0 - T_COST[0]);
        // tile (0,1) is on the road -> rejected
        place_tower(&mut g, 0, 0, 1);
        assert_eq!(g.towers.len(), 1, "cannot build on the path");
    }

    #[test]
    fn a_tower_kills_a_creep_and_pays_bounty() {
        let mut g = new_game();
        // a weak creep right next to a tower
        let (ex, ey) = center(5, 1);
        g.enemies.insert(1, Enemy { x: ex, y: ey, hp: 5.0, max: 5.0, speed: 0.0, seg: 1, kind: 0, slow: 0.0, bounty: 9 });
        place_tower(&mut g, 0, 5, 0); // arrow tower one tile above, within range
        let gold = g.gold;
        for _ in 0..120 {
            tick(&mut g);
        }
        assert!(g.enemies.is_empty(), "the creep was shot down");
        assert!(g.gold > gold, "killing a creep paid a bounty");
    }

    #[test]
    fn a_leaked_creep_costs_a_life() {
        let mut g = new_game();
        let lives = g.lives;
        let (lx, ly) = *g.path.last().unwrap();
        // creep sitting on the final waypoint with one more seg to go -> leaks
        g.enemies.insert(1, Enemy { x: lx, y: ly, hp: 100.0, max: 100.0, speed: 60.0, seg: g.path.len(), kind: 0, slow: 0.0, bounty: 0 });
        tick(&mut g);
        assert_eq!(g.lives, lives - 1, "a creep reaching the keep costs a life");
        assert!(g.enemies.is_empty());
    }

    #[test]
    fn waves_advance_and_cap() {
        let mut g = new_game();
        assert_eq!(g.wave, 0);
        start_wave(&mut g);
        assert_eq!(g.wave, 1);
        assert_eq!(g.state, 1);
        assert!(!g.to_spawn.is_empty());
    }
}
