//! opcusdb Co-Board — a collaborative whiteboard built on the engine's **CRDT**.
//!
//! Many people draw on one shared canvas; the document is an **`OrSet`**
//! (add-wins observed-remove set) of strokes from `opcusdb-algebra`. Because adds
//! and removes commute and are idempotent, you can **keep drawing offline** and
//! your strokes **merge cleanly on reconnect** — no conflicts, no lost work. Live
//! presence cursors show where everyone is.
//!
//! Run: `cargo run -p opcusdb-server --bin opcusdb-board` then open
//! http://localhost:9009 (open several tabs; try the "Go offline" toggle).

use opcusdb_algebra::OrSet;
use opcusdb_server::ws;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const PORT: u16 = 9009;

struct Board {
    ids: OrSet<String>,                // CRDT membership of stroke ids
    content: BTreeMap<String, String>, // id -> "color width x,y;x,y;..."
    op_log: Vec<String>,               // broadcastable ops: "d\t<id>\t<payload>", "e\t<id>", "x"
    users: BTreeMap<u32, (String, String)>, // id -> (name, color)
    cursors: BTreeMap<u32, (f32, f32)>,
    next_id: u32,
}

impl Board {
    fn new() -> Self {
        Self {
            ids: OrSet::new(),
            content: BTreeMap::new(),
            op_log: Vec::new(),
            users: BTreeMap::new(),
            cursors: BTreeMap::new(),
            next_id: 1,
        }
    }
}

/// Apply a stroke add to the CRDT (id = "peer:seq").
fn apply_draw(b: &mut Board, id: &str, payload: &str) {
    let tag = parse_tag(id);
    b.ids.add(id.to_string(), tag);
    b.content.insert(id.to_string(), payload.to_string());
    b.op_log.push(format!("d\t{id}\t{payload}"));
}

fn apply_erase(b: &mut Board, id: &str) {
    let key = id.to_string();
    b.ids.remove(&key);
    b.op_log.push(format!("e\t{id}"));
}

fn apply_clear(b: &mut Board) {
    let live: Vec<String> = b.ids.iter().cloned().collect();
    for id in live {
        b.ids.remove(&id);
    }
    b.op_log.push("x".to_string());
}

fn parse_tag(id: &str) -> (u64, u64) {
    let mut it = id.split(':');
    let peer = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let seq = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (peer, seq)
}

/// per-connection writer state
struct CState {
    joined: bool,
    need_full: bool,
    last_op: usize,
}

fn main() {
    let board = Arc::new(Mutex::new(Board::new()));
    let listener = TcpListener::bind(("0.0.0.0", PORT)).expect("bind");
    println!("opcusdb Co-Board (CRDT whiteboard) on http://localhost:{PORT}");
    for stream in listener.incoming().flatten() {
        let board = board.clone();
        thread::spawn(move || handle(stream, board));
    }
}

fn handle(mut stream: TcpStream, board: Arc<Mutex<Board>>) {
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
        let mut b = board.lock().unwrap();
        let id = b.next_id;
        b.next_id += 1;
        id
    };
    let cstate = Arc::new(Mutex::new(CState { joined: false, need_full: false, last_op: 0 }));

    // single-writer thread: full-state on (re)join, then op stream + presence
    let mut writer = stream.try_clone().expect("clone");
    let wboard = board.clone();
    let wcs = cstate.clone();
    let writer_handle = thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(70));
        let mut out: Vec<String> = Vec::new();
        {
            let b = wboard.lock().unwrap();
            let mut cs = wcs.lock().unwrap();
            if !cs.joined {
                continue;
            }
            if cs.need_full {
                cs.need_full = false;
                // full state as upserts (merge into the client's local set — no clear,
                // so a reconnecting client keeps its offline strokes too)
                for sid in b.ids.iter() {
                    if let Some(p) = b.content.get(sid) {
                        out.push(format!("d\t{sid}\t{p}"));
                    }
                }
                cs.last_op = b.op_log.len();
            } else if b.op_log.len() > cs.last_op {
                out.extend_from_slice(&b.op_log[cs.last_op..]);
                cs.last_op = b.op_log.len();
            }
            // presence snapshot
            let pres = b
                .users
                .iter()
                .map(|(uid, (name, col))| {
                    let (cx, cy) = b.cursors.get(uid).copied().unwrap_or((-1.0, -1.0));
                    format!("{uid}:{cx:.3}:{cy:.3}:{col}:{name}")
                })
                .collect::<Vec<_>>()
                .join(";");
            out.push(format!("p\t{pres}"));
        }
        for m in out {
            if ws::write_text(&mut writer, &m).is_err() {
                return;
            }
        }
    });

    let _ = ws::write_text(&mut stream, &format!("w\t{id}"));

    loop {
        match ws::read_frame(&mut stream) {
            Ok(Some(ws::Msg::Text(t))) => {
                // command + opaque rest (payloads may be JSON with spaces)
                let (cmd, rest) = t.split_once(' ').unwrap_or((t.as_str(), ""));
                match cmd {
                    "join" => {
                        if let Some((name, col)) = rest.split_once(' ') {
                            let mut b = board.lock().unwrap();
                            b.users.insert(id, (clean(name, 14), clean(col, 9)));
                            let mut cs = cstate.lock().unwrap();
                            cs.joined = true;
                            cs.need_full = true;
                        }
                    }
                    "draw" => {
                        // "draw <id> <opaque payload>"
                        if let Some((sid, payload)) = rest.split_once(' ') {
                            apply_draw(&mut board.lock().unwrap(), sid, payload);
                        }
                    }
                    "erase" => apply_erase(&mut board.lock().unwrap(), rest),
                    "clear" => apply_clear(&mut board.lock().unwrap()),
                    "cursor" => {
                        if let Some((x, y)) = rest.split_once(' ') {
                            if let (Ok(x), Ok(y)) = (x.parse(), y.parse()) {
                                board.lock().unwrap().cursors.insert(id, (x, y));
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

    {
        let mut b = board.lock().unwrap();
        b.users.remove(&id);
        b.cursors.remove(&id);
    }
    drop(stream);
    let _ = writer_handle.join();
}

fn clean(s: &str, n: usize) -> String {
    s.chars().filter(|c| !c.is_control() && *c != '\t').take(n).collect()
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
        "/" | "/index.html" => ("text/html; charset=utf-8", include_str!("../web/board.html").as_bytes()),
        "/board.js" => ("application/javascript; charset=utf-8", include_str!("../web/board.js").as_bytes()),
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
    fn concurrent_adds_all_survive_and_erase_removes_one() {
        let mut b = Board::new();
        // two peers draw "at the same time" — both must survive (add-wins CRDT)
        apply_draw(&mut b, "1:1", "#f00 3 0,0;1,1");
        apply_draw(&mut b, "2:1", "#00f 3 2,2;3,3");
        assert_eq!(b.ids.len(), 2, "concurrent strokes from different peers both present");
        apply_erase(&mut b, "1:1");
        assert_eq!(b.ids.len(), 1, "erase removes exactly one");
        assert!(b.ids.contains(&"2:1".to_string()));
    }

    #[test]
    fn late_offline_stroke_merges_after_an_erase() {
        // simulates: peer 2 was offline, drew 2:7, reconnects and replays it later;
        // it still merges in regardless of arrival order.
        let mut b = Board::new();
        apply_draw(&mut b, "1:1", "a");
        apply_erase(&mut b, "1:1");
        apply_draw(&mut b, "2:7", "b"); // arrives late
        assert!(b.ids.contains(&"2:7".to_string()), "late offline stroke is present");
        assert!(!b.ids.contains(&"1:1".to_string()), "erased stroke stays gone");
    }

    #[test]
    fn clear_removes_all_then_new_strokes_still_work() {
        let mut b = Board::new();
        apply_draw(&mut b, "1:1", "a");
        apply_draw(&mut b, "1:2", "b");
        apply_clear(&mut b);
        assert_eq!(b.ids.len(), 0, "clear empties the board");
        apply_draw(&mut b, "1:3", "c");
        assert_eq!(b.ids.len(), 1, "drawing after clear works");
    }
}
