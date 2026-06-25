//! opcusdb authoritative game server.
//!
//! Runs the **real** opcusdb ECS engine as the single source of truth and lets
//! many browsers connect over WebSocket and share one live world — answering
//! "how do multiple people see the same thing?": they don't simulate locally,
//! they send inputs to this server, which simulates authoritatively and
//! broadcasts state to everyone.
//!
//! Dependency-free: HTTP file serving and the WebSocket protocol are hand-rolled
//! (see [`ws`]); the world is an `opcusdb_core::World`. The `World` lives only on
//! the simulation thread; clients reach it through an `mpsc` channel (inputs) and
//! a shared `RwLock<String>` (the latest broadcast snapshot).
//!
//! Run: `cargo run -p opcusdb-server` then open http://localhost:9001 in 2+ tabs.

use opcusdb_core::{EntityId, Rng, World};
use opcusdb_server::ws;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

const PORT: u16 = 9001;
const FIELD: i32 = 1000;
const TICK_MS: u64 = 40; // 25 Hz
const MAX_BOIDS: usize = 600;

#[derive(Clone, Copy)]
struct Pos {
    x: i32,
    y: i32,
}
#[derive(Clone, Copy)]
struct Vel {
    x: i32,
    y: i32,
}
#[derive(Clone, Copy)]
struct Cursor {
    owner: u32,
}

/// Messages from client threads to the single simulation thread.
enum Cmd {
    Connect(Sender<u32>), // reply with the assigned player id
    Cursor { player: u32, x: i32, y: i32 },
    Spawn { x: i32, y: i32 },
    Disconnect(u32),
}

/// The authoritative shared world. Owned solely by the simulation thread.
struct Game {
    world: World,
    rng: Rng,
    next_player: u32,
    cursors: BTreeMap<u32, EntityId>,
    target: BTreeMap<u32, (i32, i32)>,
    spawns: Vec<(i32, i32)>,
}

impl Game {
    fn new() -> Self {
        Self {
            world: World::new(),
            rng: Rng::seed(0xA11CE),
            next_player: 1,
            cursors: BTreeMap::new(),
            target: BTreeMap::new(),
            spawns: Vec::new(),
        }
    }

    fn add_player(&mut self) -> u32 {
        let id = self.next_player;
        self.next_player += 1;
        let e = self.world.spawn();
        self.world.insert(e, Pos { x: FIELD / 2, y: FIELD / 2 });
        self.world.insert(e, Cursor { owner: id });
        self.cursors.insert(id, e);
        self.target.insert(id, (FIELD / 2, FIELD / 2));
        id
    }

    fn remove_player(&mut self, id: u32) {
        if let Some(e) = self.cursors.remove(&id) {
            self.world.despawn(e);
        }
        self.target.remove(&id);
    }

    fn tick(&mut self) {
        // 1. spawn pending dots (server-simulated), bounded.
        let boids = self.world.matching_without::<(Pos, Vel), Cursor>().len();
        let room = MAX_BOIDS.saturating_sub(boids);
        let pending: Vec<_> = self.spawns.drain(..).take(room).collect();
        for (x, y) in pending {
            let e = self.world.spawn();
            self.world.insert(e, Pos { x, y });
            self.world.insert(
                e,
                Vel {
                    x: self.rng.range(0, 9) as i32 - 4,
                    y: self.rng.range(0, 9) as i32 - 4,
                },
            );
        }
        self.spawns.clear();

        // 2. each player's cursor entity tracks their latest reported position.
        let targets = self.target.clone();
        for (id, (x, y)) in targets {
            if let Some(&e) = self.cursors.get(&id) {
                if let Some(p) = self.world.get_mut::<Pos>(e) {
                    p.x = x;
                    p.y = y;
                }
            }
        }

        // 3. move the server-owned dots (toroidal wrap).
        for id in self.world.matching_without::<(Pos, Vel), Cursor>() {
            let v = *self.world.get::<Vel>(id).unwrap();
            let p = self.world.get_mut::<Pos>(id).unwrap();
            p.x = (p.x + v.x).rem_euclid(FIELD);
            p.y = (p.y + v.y).rem_euclid(FIELD);
        }
    }

    /// Broadcast frame: `c <owner> <x> <y>;d 0 <x> <y>;...`
    fn snapshot(&self) -> String {
        let mut out = String::new();
        for (id, p) in self.world.query::<Pos>() {
            if let Some(c) = self.world.get::<Cursor>(id) {
                out.push_str(&format!("c {} {} {};", c.owner, p.x, p.y));
            } else {
                out.push_str(&format!("d 0 {} {};", p.x, p.y));
            }
        }
        out
    }
}

fn main() {
    let (tx, rx) = mpsc::channel::<Cmd>();
    let snapshot = Arc::new(RwLock::new(String::new()));

    // Simulation thread: owns the World; applies queued inputs, ticks, publishes.
    {
        let snapshot = snapshot.clone();
        thread::spawn(move || {
            let mut game = Game::new();
            loop {
                thread::sleep(Duration::from_millis(TICK_MS));
                while let Ok(cmd) = rx.try_recv() {
                    match cmd {
                        Cmd::Connect(reply) => {
                            let _ = reply.send(game.add_player());
                        }
                        Cmd::Cursor { player, x, y } => {
                            game.target.insert(player, (x, y));
                        }
                        Cmd::Spawn { x, y } => game.spawns.push((x, y)),
                        Cmd::Disconnect(id) => game.remove_player(id),
                    }
                }
                game.tick();
                *snapshot.write().unwrap() = game.snapshot();
            }
        });
    }

    let listener = TcpListener::bind(("0.0.0.0", PORT)).expect("bind");
    println!("opcusdb server on http://localhost:{PORT}  (open it in 2+ tabs)");
    for stream in listener.incoming().flatten() {
        let tx = tx.clone();
        let snapshot = snapshot.clone();
        thread::spawn(move || handle(stream, tx, snapshot));
    }
}

fn handle(mut stream: TcpStream, tx: Sender<Cmd>, snapshot: Arc<RwLock<String>>) {
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

    // Ask the sim thread for a player id, then welcome the client.
    let (rtx, rrx) = mpsc::channel();
    if tx.send(Cmd::Connect(rtx)).is_err() {
        return;
    }
    let Ok(player) = rrx.recv() else { return };
    let _ = ws::write_text(&mut stream, &format!("w {player} {FIELD} {FIELD}"));

    // Writer thread: stream the latest authoritative snapshot to this client.
    let mut writer = stream.try_clone().expect("clone");
    let snap = snapshot.clone();
    let writer_handle = thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(TICK_MS));
        let s = snap.read().unwrap().clone();
        if ws::write_text(&mut writer, &s).is_err() {
            break;
        }
    });

    // Read this client's inputs until it disconnects.
    loop {
        match ws::read_frame(&mut stream) {
            Ok(Some(ws::Msg::Text(t))) => apply_input(&tx, player, &t),
            Ok(Some(ws::Msg::Other)) => {}
            _ => break,
        }
    }
    let _ = tx.send(Cmd::Disconnect(player));
    drop(stream);
    let _ = writer_handle.join();
    println!("player {player} disconnected");
}

fn apply_input(tx: &Sender<Cmd>, player: u32, text: &str) {
    let parts: Vec<&str> = text.split_whitespace().collect();
    match parts.as_slice() {
        ["c", x, y] => {
            if let (Ok(x), Ok(y)) = (x.parse::<i32>(), y.parse::<i32>()) {
                let _ = tx.send(Cmd::Cursor { player, x: clamp(x), y: clamp(y) });
            }
        }
        ["s", x, y] => {
            if let (Ok(x), Ok(y)) = (x.parse::<i32>(), y.parse::<i32>()) {
                let _ = tx.send(Cmd::Spawn { x: clamp(x), y: clamp(y) });
            }
        }
        _ => {}
    }
}

fn clamp(v: i32) -> i32 {
    v.clamp(0, FIELD - 1)
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
    let path = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .unwrap_or("/");
    let (ctype, body): (&str, &[u8]) = match path {
        "/" | "/index.html" => ("text/html; charset=utf-8", include_str!("../web/index.html").as_bytes()),
        "/app.js" => ("application/javascript; charset=utf-8", include_str!("../web/app.js").as_bytes()),
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

    #[test]
    fn shared_world_holds_all_players_and_spawns() {
        let mut g = Game::new();
        let p1 = g.add_player();
        let p2 = g.add_player();
        g.target.insert(p1, (123, 456));
        g.target.insert(p2, (700, 200));
        g.spawns.push((50, 60));
        g.tick();
        let snap = g.snapshot();
        // every connected player's cursor is in the one shared snapshot...
        assert!(snap.contains(&format!("c {p1} 123 456")), "p1 cursor: {snap}");
        assert!(snap.contains(&format!("c {p2} 700 200")), "p2 cursor: {snap}");
        // ...alongside the server-simulated dot.
        assert!(snap.split(';').any(|e| e.starts_with("d 0")), "a dot exists");
    }

    #[test]
    fn disconnect_removes_player_from_world() {
        let mut g = Game::new();
        let p1 = g.add_player();
        let p2 = g.add_player();
        g.remove_player(p1);
        g.tick();
        let snap = g.snapshot();
        assert!(!snap.contains(&format!("c {p1} ")), "p1 gone");
        assert!(snap.contains(&format!("c {p2} ")), "p2 remains");
    }

    #[test]
    fn boid_count_is_bounded() {
        let mut g = Game::new();
        g.add_player();
        for _ in 0..(MAX_BOIDS + 200) {
            g.spawns.push((10, 10));
            g.tick();
        }
        let dots = g.world.matching_without::<(Pos, Vel), Cursor>().len();
        assert!(dots <= MAX_BOIDS, "dots capped at {MAX_BOIDS}, got {dots}");
    }
}
