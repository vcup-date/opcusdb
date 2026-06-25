//! opcusdb human + AI chatroom — an IRC-style `#lobby` over WebSocket.
//!
//! Anyone can log in with a nickname; **10 AI chatters** live in the channel and
//! talk with humans and each other, powered by OpenRouter
//! (`deepseek/deepseek-v4-flash`). The server is the authoritative channel; the
//! browser is a thin client.
//!
//! The API key is read from the **`OPENROUTER_API_KEY`** environment variable and
//! is never stored in the repo. The HTTPS call is made via the system `curl`
//! (so no TLS dependency); to conserve credits the AIs only speak while at least
//! one human is connected.
//!
//! Run: `OPENROUTER_API_KEY=sk-... cargo run -p opcusdb-server --bin opcusdb-chat`
//! then open http://localhost:9002 (in several tabs / share with friends).

use opcusdb_core::Rng;
use opcusdb_server::ws;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const PORT: u16 = 9002;
const MODEL: &str = "deepseek/deepseek-v4-flash";
const HISTORY: usize = 16;

/// The 10 AI chatters. Vivid, *distinct* characters with concrete worlds so the
/// conversation has personality and stays grounded (not vague metaphor loops).
const PERSONAS: &[(&str, &str)] = &[
    ("ada", "a blunt senior backend engineer; hates hype and premature optimization; talks databases, latency, and real trade-offs"),
    ("turing", "a curious CS grad student; asks genuine 'but how does that actually work?' questions; loves puzzles"),
    ("nova", "a warm community manager; hypes people up, makes plans, checks in on how everyone's doing"),
    ("glitch", "a sarcastic gamer who playfully roasts people and never takes anything seriously"),
    ("sage", "a down-to-earth yoga teacher who gives practical, calm advice about real life (not platitudes)"),
    ("pixel", "a speedrunner obsessed with specific games, frame data, and chasing personal bests"),
    ("echo", "a music nerd who relates everything to albums, gigs, and obscure bands, with strong opinions"),
    ("byte", "a terse embedded-firmware dev who answers in short, precise, technical bursts"),
    ("luna", "an illustrator who notices color, light, and design, and mentions what she's drawing"),
    ("rex", "a competitive startup founder who turns everything into growth, hustle, and winning"),
];

struct Line {
    author: String,
    kind: char, // 'h' human, 'a' ai, 's' system
    text: String,
}

struct Chat {
    log: Vec<Line>,
    humans: BTreeMap<u32, String>,
    next_id: u32,
    typing: BTreeSet<String>, // AI chatters currently composing a reply
}

impl Chat {
    fn new() -> Self {
        Self { log: Vec::new(), humans: BTreeMap::new(), next_id: 1, typing: BTreeSet::new() }
    }

    /// Authors of the last few messages (to avoid the same bot replying twice).
    fn recent_authors(&self, n: usize) -> Vec<String> {
        let start = self.log.len().saturating_sub(n);
        self.log[start..].iter().map(|l| l.author.clone()).collect()
    }
    fn post(&mut self, author: &str, kind: char, text: &str) {
        let text = text.replace(['\t', '\n', '\r'], " ");
        println!("[#lobby] {author}: {text}");
        self.log.push(Line { author: author.to_string(), kind, text });
    }
    /// Recent transcript as `author: text` lines, for AI context.
    fn transcript(&self, n: usize) -> String {
        let start = self.log.len().saturating_sub(n);
        self.log[start..]
            .iter()
            .filter(|l| l.kind != 's')
            .map(|l| format!("{}: {}", l.author, l.text))
            .collect::<Vec<_>>()
            .join("\n")
    }
    /// `name:kind,name:kind,...` — humans (kind 0) followed by the AI bots (kind 1).
    fn userlist(&self) -> String {
        let mut parts: Vec<String> = self.humans.values().map(|n| format!("{n}:0")).collect();
        for (name, _) in PERSONAS {
            parts.push(format!("{name}:1"));
        }
        parts.join(",")
    }
}

fn main() {
    let chat = Arc::new(Mutex::new(Chat::new()));
    if std::env::var("OPENROUTER_API_KEY").map_or(true, |k| k.is_empty()) {
        eprintln!("WARNING: OPENROUTER_API_KEY not set — AI chatters will stay silent.");
    }
    run_director(chat.clone());

    let listener = TcpListener::bind(("0.0.0.0", PORT)).expect("bind");
    println!("opcusdb chatroom on http://localhost:{PORT}  (open it, pick a nick, say hi)");
    for stream in listener.incoming().flatten() {
        let chat = chat.clone();
        thread::spawn(move || handle(stream, chat));
    }
}

// --- conversation director -------------------------------------------------

/// Max concurrent OpenRouter calls in flight (keeps it snappy, bounds cost).
const MAX_INFLIGHT: usize = 3;

/// Drives the room so it feels like a real chat: when a human (or bot) speaks,
/// a couple of *different* bots reply within a couple of seconds; when it goes
/// quiet, one bot revives the conversation. Bots only speak with a human present.
fn run_director(chat: Arc<Mutex<Chat>>) {
    thread::spawn(move || {
        let inflight = Arc::new(AtomicUsize::new(0));
        let mut rng = Rng::seed(now_nanos() | 1);
        let mut handled = 0usize; // log length we've already reacted to
        let mut idle_ticks = 0u32;
        loop {
            thread::sleep(Duration::from_millis(500));
            let (len, last_kind, recent, has_human) = {
                let c = chat.lock().unwrap();
                let last = c.log.last();
                (
                    c.log.len(),
                    last.map(|l| l.kind).unwrap_or('s'),
                    c.recent_authors(3),
                    !c.humans.is_empty(),
                )
            };
            if !has_human {
                handled = len;
                idle_ticks = 0;
                continue;
            }

            let mut responders: Vec<usize> = Vec::new();
            if len > handled && last_kind != 's' {
                idle_ticks = 0;
                // Someone just spoke. A human draws 1-2 replies; a bot draws ~1.
                let n = if last_kind == 'h' { 1 + rng.below(2) as usize } else { usize::from(rng.chance(1, 2)) };
                responders = pick_bots(&mut rng, n, &recent, &chat);
            } else if len == handled {
                idle_ticks += 1;
                if idle_ticks >= 16 {
                    // ~8s quiet -> one bot revives things.
                    idle_ticks = 0;
                    responders = pick_bots(&mut rng, 1, &recent, &chat);
                }
            }
            handled = len;

            for idx in responders {
                if inflight.load(Ordering::Relaxed) >= MAX_INFLIGHT {
                    break;
                }
                let (name, persona) = PERSONAS[idx];
                inflight.fetch_add(1, Ordering::Relaxed);
                chat.lock().unwrap().typing.insert(name.to_string());
                let chat = chat.clone();
                let inflight = inflight.clone();
                let jitter = rng.range(300, 1600) as u64;
                thread::spawn(move || {
                    thread::sleep(Duration::from_millis(jitter)); // human-like stagger
                    let transcript = chat.lock().unwrap().transcript(HISTORY);
                    let reply = ai_reply(name, persona, &transcript);
                    let mut c = chat.lock().unwrap();
                    c.typing.remove(name);
                    if let Some(r) = reply {
                        c.post(name, 'a', &r);
                    }
                    drop(c);
                    inflight.fetch_sub(1, Ordering::Relaxed);
                });
            }
        }
    });
}

/// Pick up to `n` distinct bot indices, avoiding ones that spoke recently or are
/// already typing.
fn pick_bots(rng: &mut Rng, n: usize, recent: &[String], chat: &Arc<Mutex<Chat>>) -> Vec<usize> {
    if n == 0 {
        return Vec::new();
    }
    let typing = chat.lock().unwrap().typing.clone();
    let mut pool: Vec<usize> = (0..PERSONAS.len())
        .filter(|&i| !recent.iter().any(|a| a == PERSONAS[i].0) && !typing.contains(PERSONAS[i].0))
        .collect();
    if pool.is_empty() {
        pool = (0..PERSONAS.len()).collect();
    }
    let mut out = Vec::new();
    for _ in 0..n.min(pool.len()) {
        let k = rng.below(pool.len() as u32) as usize;
        out.push(pool.swap_remove(k));
    }
    out
}

/// Ask OpenRouter for this persona's next single-line message. `None` on any error.
fn ai_reply(name: &str, persona: &str, transcript: &str) -> Option<String> {
    let key = std::env::var("OPENROUTER_API_KEY").ok().filter(|k| !k.is_empty())?;
    let system = format!(
        "You are {name} in a casual group chat called #lobby with humans and other people. \
         Your character: {persona}. \
         Chat like a real person: ONE short, natural line under 20 words. Be specific and concrete — \
         mention real things from your world, have opinions, react directly to the last messages, ask or \
         answer plainly. Stay in character. Avoid vague cosmic/abstract metaphors, no emoji, no name prefix, no quotes."
    );
    let user = format!("Recent chat in #lobby:\n{transcript}\n\nReply as {name} (one short, in-character line):");
    // Disable the model's hidden reasoning (it would eat the token budget and
    // truncate the actual reply), and give content enough room.
    let body = format!(
        "{{\"model\":\"{}\",\"max_tokens\":160,\"temperature\":0.85,\"reasoning\":{{\"enabled\":false}},\
         \"messages\":[{{\"role\":\"system\",\"content\":\"{}\"}},{{\"role\":\"user\",\"content\":\"{}\"}}]}}",
        MODEL,
        json_escape(&system),
        json_escape(&user),
    );
    let out = Command::new("curl")
        .args([
            "-s",
            "-m",
            "30",
            "-X",
            "POST",
            "https://openrouter.ai/api/v1/chat/completions",
            "-H",
            &format!("Authorization: Bearer {key}"),
            "-H",
            "Content-Type: application/json",
            "-d",
            &body,
        ])
        .output()
        .ok()?;
    let resp = String::from_utf8_lossy(&out.stdout);
    let content = extract_content(&resp)?;
    let line = sanitize(&content);
    if line.is_empty() {
        None
    } else {
        Some(line)
    }
}

fn sanitize(s: &str) -> String {
    let mut t = s.trim().replace(['\n', '\r', '\t'], " ");
    // strip a single pair of surrounding quotes the model sometimes adds
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        t = t[1..t.len() - 1].to_string();
    }
    let t = t.trim();
    t.chars().take(240).collect()
}

// --- minimal JSON helpers (no serde) ---------------------------------------

/// Escape a string for embedding in a JSON document.
fn json_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o
}

/// Extract `choices[0].message.content` from an OpenRouter response.
fn extract_content(resp: &str) -> Option<String> {
    // Find the message object, then the first "content":"..." after it.
    let from = resp.find("\"message\"").unwrap_or(0);
    let key = "\"content\":\"";
    let start = resp[from..].find(key)? + from + key.len();
    Some(decode_json_string(&resp[start..]))
}

/// Decode a JSON string body (the bytes after the opening quote) up to the
/// unescaped closing quote.
fn decode_json_string(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => break,
            '\\' => match chars.next() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('/') => out.push('/'),
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('b') => out.push('\u{8}'),
                Some('f') => out.push('\u{c}'),
                Some('u') => {
                    let hex: String = (0..4).filter_map(|_| chars.next()).collect();
                    if let Ok(hi) = u32::from_str_radix(&hex, 16) {
                        if (0xD800..=0xDBFF).contains(&hi) {
                            // high surrogate: pair it with the following \uXXXX low surrogate
                            let (bs, u) = (chars.next(), chars.next());
                            let lo_hex: String = (0..4).filter_map(|_| chars.next()).collect();
                            if bs == Some('\\') && u == Some('u') {
                                if let Ok(lo) = u32::from_str_radix(&lo_hex, 16) {
                                    let cp = 0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00);
                                    if let Some(ch) = char::from_u32(cp) {
                                        out.push(ch);
                                    }
                                }
                            }
                        } else if let Some(ch) = char::from_u32(hi) {
                            out.push(ch);
                        }
                    }
                }
                Some(other) => out.push(other),
                None => break,
            },
            c => out.push(c),
        }
    }
    out
}

// --- per-connection handling ----------------------------------------------

fn handle(mut stream: TcpStream, chat: Arc<Mutex<Chat>>) {
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
        let mut c = chat.lock().unwrap();
        let id = c.next_id;
        c.next_id += 1;
        id
    };

    // Writer thread: stream new channel lines + the user list to this client.
    let mut writer = stream.try_clone().expect("clone");
    let wchat = chat.clone();
    let writer_handle = thread::spawn(move || {
        let mut sent = 0usize;
        let mut last_users = String::new();
        let mut last_typing = String::new();
        loop {
            thread::sleep(Duration::from_millis(120));
            let (lines, users, typing) = {
                let c = wchat.lock().unwrap();
                let lines: Vec<String> = c.log[sent..]
                    .iter()
                    .map(|l| {
                        let k = match l.kind {
                            'a' => "1",
                            's' => "s",
                            _ => "0",
                        };
                        format!("m\t{}\t{}\t{}", l.author, k, l.text)
                    })
                    .collect();
                sent = c.log.len();
                let typing = c.typing.iter().cloned().collect::<Vec<_>>().join(",");
                (lines, c.userlist(), typing)
            };
            for l in &lines {
                if ws::write_text(&mut writer, l).is_err() {
                    return;
                }
            }
            if users != last_users {
                if ws::write_text(&mut writer, &format!("u\t{users}")).is_err() {
                    return;
                }
                last_users = users;
            }
            if typing != last_typing {
                if ws::write_text(&mut writer, &format!("t\t{typing}")).is_err() {
                    return;
                }
                last_typing = typing;
            }
        }
    });

    // Reader: this client's login + messages.
    let mut nick: Option<String> = None;
    loop {
        match ws::read_frame(&mut stream) {
            Ok(Some(ws::Msg::Text(t))) => {
                if let Some(rest) = t.strip_prefix("login ") {
                    let n = clean_nick(rest, id);
                    chat.lock().unwrap().humans.insert(id, n.clone());
                    chat.lock().unwrap().post(&n, 's', &format!("{n} joined #lobby"));
                    nick = Some(n);
                } else if let Some(rest) = t.strip_prefix("msg ") {
                    if let Some(n) = &nick {
                        let text = rest.trim();
                        if !text.is_empty() {
                            chat.lock().unwrap().post(n, 'h', text);
                        }
                    }
                }
            }
            Ok(Some(ws::Msg::Other)) => {}
            _ => break,
        }
    }

    // Disconnect.
    if let Some(n) = nick {
        let mut c = chat.lock().unwrap();
        c.humans.remove(&id);
        c.post(&n, 's', &format!("{n} left #lobby"));
    }
    drop(stream);
    let _ = writer_handle.join();
}

fn clean_nick(raw: &str, id: u32) -> String {
    let n: String = raw.trim().chars().filter(|c| !c.is_whitespace()).take(16).collect();
    if n.is_empty() {
        format!("guest{id}")
    } else {
        n
    }
}

fn now_nanos() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0)
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
        "/" | "/index.html" => ("text/html; charset=utf-8", include_str!("../web/chat.html").as_bytes()),
        "/chat.js" => ("application/javascript; charset=utf-8", include_str!("../web/chat.js").as_bytes()),
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
    fn json_escape_roundtrip_via_decode() {
        let s = "he said \"hi\"\nline2\tend \\ /";
        let escaped = json_escape(s);
        // decode_json_string expects the body after the opening quote, up to a closing quote
        let decoded = decode_json_string(&format!("{escaped}\""));
        assert_eq!(decoded, s);
    }

    #[test]
    fn extract_content_from_openrouter_shape() {
        let resp = r#"{"choices":[{"message":{"content":"hello \"world\"\nbye","role":"assistant"},"reasoning":"ignore me"}]}"#;
        assert_eq!(extract_content(resp).unwrap(), "hello \"world\"\nbye");
    }

    #[test]
    fn sanitize_strips_quotes_and_newlines() {
        assert_eq!(sanitize("  \"hey there\"\n "), "hey there");
        assert_eq!(sanitize("multi\nline\ttext"), "multi line text");
    }

    #[test]
    fn transcript_and_userlist() {
        let mut c = Chat::new();
        c.humans.insert(1, "alice".into());
        c.post("alice", 'h', "hi all");
        c.post("ada", 'a', "hey alice");
        assert!(c.transcript(10).contains("alice: hi all"));
        let ul = c.userlist();
        assert!(ul.contains("alice:0"));
        assert!(ul.contains("ada:1")); // AI bots always listed
    }
}
