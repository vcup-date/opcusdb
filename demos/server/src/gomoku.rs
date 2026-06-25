//! opcusdb Gomoku — online **Five-in-a-Row** (五子棋) on a Go board.
//!
//! Two players join a **room** by code (black moves first, then white), take turns
//! placing stones on a 15×15 board; the first to get **five in a row**
//! (horizontal, vertical, or diagonal) wins. The Rust server is authoritative:
//! it validates every move, detects the win, and broadcasts the board to the
//! room over the hand-rolled WebSocket (see [`ws`]). Spectators may watch.
//!
//! Win counts persist to a small **local DB file** (`gomoku.db`, gitignored),
//! shown as an all-time leaderboard.
//!
//! Run: `cargo run -p opcusdb-server --bin opcusdb-gomoku` then open
//! http://localhost:9004 — create a room, share the code, play.

use opcusdb_server::ws;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const PORT: u16 = 9004;
const N: i32 = 15; // board size
const NEED: usize = 5; // in a row to win
const LB_PATH: &str = "gomoku.db";
const LB_MAX: usize = 10;

#[derive(Clone, Default)]
struct Seat {
    id: Option<u32>,
    nick: String,
}

struct Room {
    board: Vec<u8>, // 0 empty, 1 black, 2 white; len N*N
    black: Seat,
    white: Seat,
    to_move: u8,        // 1 or 2
    winner: u8,         // 0 none, 1 black, 2 white, 3 draw
    last: i32,          // last move index, -1 none
    win_line: Vec<i32>, // winning stone indices
    snapshot: String,
}

impl Room {
    fn new() -> Self {
        Self {
            board: vec![0; (N * N) as usize],
            black: Seat::default(),
            white: Seat::default(),
            to_move: 1,
            winner: 0,
            last: -1,
            win_line: Vec::new(),
            snapshot: String::new(),
        }
    }
    fn reset(&mut self) {
        self.board = vec![0; (N * N) as usize];
        self.to_move = 1;
        self.winner = 0;
        self.last = -1;
        self.win_line.clear();
    }
    fn role_of(&self, id: u32) -> char {
        if self.black.id == Some(id) {
            'b'
        } else if self.white.id == Some(id) {
            'w'
        } else {
            's'
        }
    }
}

struct Arena {
    rooms: BTreeMap<String, Room>,
    wins: Vec<(String, u32)>,
    next_id: u32,
}

fn main() {
    let arena = Arc::new(Mutex::new(Arena {
        rooms: BTreeMap::new(),
        wins: load_lb(),
        next_id: 1,
    }));
    let listener = TcpListener::bind(("0.0.0.0", PORT)).expect("bind");
    println!("opcusdb Gomoku (five-in-a-row) on http://localhost:{PORT}  — create/join a room");
    for stream in listener.incoming().flatten() {
        let arena = arena.clone();
        thread::spawn(move || handle(stream, arena));
    }
}

// --- rules -----------------------------------------------------------------

fn idx(x: i32, y: i32) -> usize {
    (y * N + x) as usize
}

/// If placing `color` at (x,y) makes 5+ in a row, return the winning line.
fn winning_line(board: &[u8], x: i32, y: i32, color: u8) -> Option<Vec<i32>> {
    for (dx, dy) in [(1, 0), (0, 1), (1, 1), (1, -1)] {
        let mut line = vec![idx(x, y) as i32];
        for dir in [1, -1] {
            let (mut cx, mut cy) = (x + dx * dir, y + dy * dir);
            while (0..N).contains(&cx) && (0..N).contains(&cy) && board[idx(cx, cy)] == color {
                line.push(idx(cx, cy) as i32);
                cx += dx * dir;
                cy += dy * dir;
            }
        }
        if line.len() >= NEED {
            return Some(line);
        }
    }
    None
}

fn record_win(lb: &mut Vec<(String, u32)>, nick: &str) {
    if let Some(e) = lb.iter_mut().find(|(n, _)| n == nick) {
        e.1 += 1;
    } else {
        lb.push((nick.to_string(), 1));
    }
    lb.sort_by_key(|e| std::cmp::Reverse(e.1));
    lb.truncate(LB_MAX);
}

fn rebuild_snapshot(room: &mut Room, lb: &[(String, u32)]) {
    let board: String = room.board.iter().map(|c| (b'0' + c) as char).collect();
    let win_line = if room.win_line.is_empty() {
        "-".to_string()
    } else {
        room.win_line.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",")
    };
    let mut s = String::new();
    s.push_str(&format!(
        "s\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
        N,
        board,
        room.to_move,
        room.winner,
        room.last,
        if room.black.id.is_some() { &room.black.nick } else { "—" },
        if room.white.id.is_some() { &room.white.nick } else { "—" },
        win_line,
    ));
    let lb_s = lb.iter().map(|(n, w)| format!("{n}:{w}")).collect::<Vec<_>>().join(",");
    s.push_str(&format!("l\t{lb_s}\n"));
    room.snapshot = s;
}

// --- persistence (local DB) ------------------------------------------------

fn load_lb() -> Vec<(String, u32)> {
    std::fs::read_to_string(LB_PATH)
        .map(|t| {
            t.lines()
                .filter_map(|l| {
                    let (n, w) = l.rsplit_once(' ')?;
                    Some((n.to_string(), w.trim().parse().ok()?))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn save_lb(lb: &[(String, u32)]) {
    let body: String = lb.iter().map(|(n, w)| format!("{n} {w}\n")).collect();
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

    let my_room: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    // writer: push this client's room snapshot when it changes
    let mut writer = stream.try_clone().expect("clone");
    let warena = arena.clone();
    let wroom = my_room.clone();
    let writer_handle = thread::spawn(move || {
        let mut last = String::new();
        loop {
            thread::sleep(Duration::from_millis(120));
            let snap = {
                let code = wroom.lock().unwrap().clone();
                match code {
                    Some(c) => warena.lock().unwrap().rooms.get(&c).map(|r| r.snapshot.clone()),
                    None => None,
                }
            };
            if let Some(s) = snap {
                if s != last {
                    if ws::write_text(&mut writer, &s).is_err() {
                        return;
                    }
                    last = s;
                }
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
                        let lb = a.wins.clone();
                        let room = a.rooms.entry(code.clone()).or_insert_with(Room::new);
                        // seat assignment: black, then white, else spectator
                        if room.black.id.is_none() {
                            room.black = Seat { id: Some(id), nick: nick.clone() };
                        } else if room.white.id.is_none() {
                            room.white = Seat { id: Some(id), nick: nick.clone() };
                        }
                        let role = room.role_of(id);
                        rebuild_snapshot(room, &lb);
                        *my_room.lock().unwrap() = Some(code);
                        let _ = ws::write_text(&mut stream, &format!("w\t{id}\t{role}"));
                    }
                    ["place", x, y] => {
                        if let (Some(code), Ok(x), Ok(y)) =
                            (my_room.lock().unwrap().clone(), x.parse::<i32>(), y.parse::<i32>())
                        {
                            let mut a = arena.lock().unwrap();
                            let mut win_nick: Option<String> = None;
                            if let Some(room) = a.rooms.get_mut(&code) {
                                let both = room.black.id.is_some() && room.white.id.is_some();
                                let my = room.role_of(id);
                                let my_color = if my == 'b' { 1 } else if my == 'w' { 2 } else { 0 };
                                let on_board = (0..N).contains(&x) && (0..N).contains(&y);
                                if both
                                    && room.winner == 0
                                    && my_color == room.to_move
                                    && on_board
                                    && room.board[idx(x, y)] == 0
                                {
                                    room.board[idx(x, y)] = my_color;
                                    room.last = idx(x, y) as i32;
                                    if let Some(line) = winning_line(&room.board, x, y, my_color) {
                                        room.winner = my_color;
                                        room.win_line = line;
                                        win_nick = Some(if my_color == 1 {
                                            room.black.nick.clone()
                                        } else {
                                            room.white.nick.clone()
                                        });
                                    } else if room.board.iter().all(|&c| c != 0) {
                                        room.winner = 3; // draw
                                    } else {
                                        room.to_move = if room.to_move == 1 { 2 } else { 1 };
                                    }
                                }
                            }
                            if let Some(n) = win_nick {
                                record_win(&mut a.wins, &n);
                                save_lb(&a.wins);
                            }
                            let lb = a.wins.clone();
                            if let Some(room) = a.rooms.get_mut(&code) {
                                rebuild_snapshot(room, &lb);
                            }
                        }
                    }
                    ["rematch"] => {
                        if let Some(code) = my_room.lock().unwrap().clone() {
                            let mut a = arena.lock().unwrap();
                            let lb = a.wins.clone();
                            if let Some(room) = a.rooms.get_mut(&code) {
                                room.reset();
                                rebuild_snapshot(room, &lb);
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

    // disconnect: vacate seat, reset to waiting
    if let Some(code) = my_room.lock().unwrap().clone() {
        let mut a = arena.lock().unwrap();
        let lb = a.wins.clone();
        let mut drop_room = false;
        if let Some(room) = a.rooms.get_mut(&code) {
            if room.black.id == Some(id) {
                room.black = Seat::default();
                room.reset();
            } else if room.white.id == Some(id) {
                room.white = Seat::default();
                room.reset();
            }
            if room.black.id.is_none() && room.white.id.is_none() {
                drop_room = true;
            } else {
                rebuild_snapshot(room, &lb);
            }
        }
        if drop_room {
            a.rooms.remove(&code);
        }
    }
    drop(stream);
    let _ = writer_handle.join();
}

fn clean_code(s: &str) -> String {
    let c: String = s.chars().filter(|c| c.is_ascii_alphanumeric()).take(6).collect::<String>().to_uppercase();
    if c.is_empty() { "GO".to_string() } else { c }
}

fn clean_nick(s: &str, id: u32) -> String {
    let n: String = s.chars().filter(|c| !c.is_whitespace()).take(14).collect();
    if n.is_empty() { format!("p{id}") } else { n }
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
        "/" | "/index.html" => ("text/html; charset=utf-8", include_str!("../web/gomoku.html").as_bytes()),
        "/gomoku.js" => ("application/javascript; charset=utf-8", include_str!("../web/gomoku.js").as_bytes()),
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

    fn place(b: &mut [u8], x: i32, y: i32, c: u8) {
        b[idx(x, y)] = c;
    }

    #[test]
    fn detects_horizontal_five() {
        let mut b = vec![0u8; (N * N) as usize];
        for x in 3..7 {
            place(&mut b, x, 7, 1);
        }
        assert!(winning_line(&b, 6, 7, 1).is_none(), "only four so far");
        place(&mut b, 7, 7, 1);
        let line = winning_line(&b, 7, 7, 1).expect("five in a row");
        assert_eq!(line.len(), 5);
    }

    #[test]
    fn detects_diagonal_five() {
        let mut b = vec![0u8; (N * N) as usize];
        for i in 0..5 {
            place(&mut b, 2 + i, 2 + i, 2);
        }
        assert!(winning_line(&b, 6, 6, 2).is_some());
        // a different colour on the diagonal breaks it
        let mut b2 = b.clone();
        place(&mut b2, 4, 4, 1);
        assert!(winning_line(&b2, 6, 6, 2).is_none());
    }

    #[test]
    fn win_counts_accumulate_and_sort() {
        let mut lb = Vec::new();
        record_win(&mut lb, "alice");
        record_win(&mut lb, "bob");
        record_win(&mut lb, "alice");
        assert_eq!(lb[0], ("alice".into(), 2));
        assert_eq!(lb[1], ("bob".into(), 1));
    }
}
