//! opcusdb Hearth, a living **AI town** you can walk into.
//!
//! Twelve LLM residents (OpenRouter) live in one
//! shared town: they follow a daily routine (work → market → social → tavern →
//! home), and whenever characters share a place they hold a short, in-character
//! conversation. The twist vs. a 2023-style "watch the agents" demo: **every
//! browser is an embodied visitor**, you walk around, the residents perceive
//! whoever is near them (area-of-interest) and talk *to you*, and multiple humans
//! share the same town. So it's a place you're inside of, not a TV channel.
//!
//! The server is the authoritative simulation; the AI calls go out via system
//! `curl` (no HTTP/TLS dependency). The key is read from `OPENROUTER_API_KEY` and
//! never stored. Without a key, residents fall back to canned ambient lines so the
//! town still feels alive.
//!
//! Run: `OPENROUTER_API_KEY=sk-... cargo run -p opcusdb-server --bin opcusdb-town`
//! then open http://localhost:9011 (open several tabs to wander together).

use opcusdb_core::Rng;
use opcusdb_server::ws;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const PORT: u16 = 9011;
const TILE: f32 = 32.0;
const COLS: i32 = 30;
const ROWS: i32 = 19;
const TICK_MS: u64 = 50; // 20 Hz movement
const DT: f32 = 0.05;
const DAY_SECS: f32 = 200.0; // a full day cycle
const SPEED: f32 = 46.0;
// A free model that fails fast when the free tier is rate-limited (so the town
// drops to a canned line in about a second) instead of hanging like the 550B id did.
const MODEL: &str = "meta-llama/llama-3.3-70b-instruct:free";

// (name, persona, role, work-location index, favourite social-location index)
const RESIDENTS: [(&str, &str, &str, usize, usize); 12] = [
    ("Mara", "a warm, gossipy baker who knows everyone's business and talks about bread, the oven, and her neighbours", "baker", 1, 0),
    ("Tomas", "a gruff, practical blacksmith who complains about iron prices and distrusts new ideas", "smith", 2, 4),
    ("Lila", "a dreamy, shy gardener who loves her plants and notices small beautiful things", "gardener", 3, 3),
    ("Bran", "a jovial, loud tavern keeper who tells tall tales and pours strong drinks", "tavernkeep", 4, 4),
    ("Yuki", "a precise, curious librarian who quotes books and corrects people gently", "librarian", 5, 5),
    ("Ravi", "a shrewd, optimistic merchant who is always trying to sell something", "merchant", 6, 0),
    ("Nina", "a theatrical, flirty travelling bard who turns everything into a song or drama", "bard", 0, 4),
    ("Otto", "a grumpy, superstitious old fisherman who speaks in short sentences and reads the weather", "fisher", 7, 7),
    ("Pia", "a kind, slightly anxious healer who worries about everyone's health and herbs", "healer", 0, 5),
    ("Sol", "an energetic, mischievous child who asks endless questions and runs everywhere", "child", 0, 3),
    ("Greta", "a sarcastic, sharp-witted weaver who judges everyone but means well underneath", "weaver", 6, 0),
    ("Finn", "a restless, fast-talking courier who carries rumours and news between towns", "courier", 0, 0),
];

// (name, tile_x, tile_y, tile_w, tile_h, kind), kind drives client rendering
// Stand points in pixel coords on WALKABLE ground (the plaza and the cobblestone
// paths of the generated map), kept well inside the 960x608 view so nobody walks
// on a roof or wanders off-screen. Index order is fixed (RESIDENTS reference it).
const LOCS: [(&str, f32, f32, &str); 9] = [
    ("Plaza", 480.0, 300.0, "plaza"),
    ("Bakery", 344.0, 196.0, "bakery"),
    ("Forge", 616.0, 196.0, "forge"),
    ("Garden", 344.0, 410.0, "garden"),
    ("Tavern", 616.0, 410.0, "tavern"),
    ("Library", 658.0, 300.0, "library"),
    ("Market", 480.0, 188.0, "market"),
    ("Dock", 302.0, 300.0, "dock"),
    ("Homes", 480.0, 424.0, "homes"),
];

struct Char {
    x: f32,
    y: f32,
    tx: f32,
    ty: f32,
    name: String,
    persona: &'static str,
    role: &'static str,
    pal: u8,
    work: usize,
    fav: usize,
    here: i32,
    bubble: String,
    bubble_t: f32,
    last_spoke: f32,
    facing: f32,
    goal: usize,
    path: Vec<(f32, f32)>,
    human: bool,
    mem: Vec<String>,
}

struct Town {
    chars: BTreeMap<u32, Char>,
    transcripts: Vec<Vec<String>>, // per location: recent "Name: line"
    pending: Vec<bool>,            // per location: a human just spoke -> prioritise a reply
    time: f32,
    next_id: u32,
    next_client: u32,
    humans: usize,
}

fn loc_stand(i: usize) -> (f32, f32) {
    (LOCS[i].1, LOCS[i].2)
}

/// The location node nearest to a point (the start of a route).
fn nearest_node(x: f32, y: f32) -> usize {
    let mut best = 0;
    let mut bd = f32::MAX;
    for i in 0..LOCS.len() {
        let (sx, sy) = loc_stand(i);
        let d = (x - sx).powi(2) + (y - sy).powi(2);
        if d < bd {
            bd = d;
            best = i;
        }
    }
    best
}

/// Waypoints from node `from` to node `to`. The plaza (index 0) is the hub, so a
/// trip between two outer nodes goes through it, keeping residents on the roads
/// (cardinal-ish moves) instead of sliding diagonally across grass and roofs.
fn route(from: usize, to: usize) -> Vec<(f32, f32)> {
    if from == to || from == 0 || to == 0 {
        vec![loc_stand(to)]
    } else {
        vec![loc_stand(0), loc_stand(to)]
    }
}

fn new_town() -> Town {
    let mut chars = BTreeMap::new();
    let mut rng = Rng::seed(0x484541_u64);
    for (i, &(name, persona, role, work, fav)) in RESIDENTS.iter().enumerate() {
        let (sx, sy) = loc_stand(work); // start spread out, each at their own workplace
        let id = i as u32 + 1;
        chars.insert(
            id,
            Char {
                x: sx + (rng.below(40) as f32 - 20.0),
                y: sy + (rng.below(40) as f32 - 20.0),
                tx: sx,
                ty: sy,
                name: name.to_string(),
                persona,
                role,
                pal: i as u8,
                work,
                fav,
                here: -1,
                bubble: String::new(),
                bubble_t: 0.0,
                last_spoke: 0.0,
                facing: 0.0,
                goal: work,
                path: Vec::new(),
                human: false,
                mem: Vec::new(),
            },
        );
    }
    Town {
        chars,
        transcripts: vec![Vec::new(); LOCS.len()],
        pending: vec![false; LOCS.len()],
        time: 40.0,
        next_id: 100,
        next_client: 1,
        humans: 0,
    }
}

/// Where should this resident be right now? Returns a location index. Residents
/// mostly stay at their own workplace; a rotating, per-resident-staggered third
/// drifts to the plaza so small groups form and break up instead of everyone
/// piling onto one spot and marching in lockstep.
fn schedule(c: &Char, time: f32) -> usize {
    let p = (time % DAY_SECS) / DAY_SECS;
    if p >= 0.95 {
        return 8; // late night: home
    }
    if p >= 0.80 {
        return c.fav; // evening: your favourite social spot (small groups)
    }
    let slot = (time / 11.0) as u64 + c.work as u64 + c.fav as u64 + c.pal as u64;
    if slot % 3 == 0 {
        0 // a rotating third hang out at the plaza
    } else {
        c.work // otherwise at your own workplace
    }
}

fn tick(t: &mut Town) {
    t.time += DT;
    let ids: Vec<u32> = t.chars.keys().copied().collect();
    let mut arrivals: Vec<usize> = Vec::new();
    for id in ids {
        let human = t.chars[&id].human;
        // residents plan a route along the roads (through the plaza hub) toward
        // their scheduled goal; the human visitor steers itself with click targets.
        if !human {
            let desired = schedule(&t.chars[&id], t.time);
            let c = t.chars.get_mut(&id).unwrap();
            if desired != c.goal || c.path.is_empty() {
                c.goal = desired;
                c.path = route(nearest_node(c.x, c.y), desired);
            }
            let last_hop = c.path.len() <= 1;
            let (wx, wy) = *c.path.first().unwrap_or(&(c.x, c.y));
            let (tx, ty) = if last_hop {
                let a = id as f32 * 2.39996;
                let r = 7.0 + (id % 3) as f32 * 6.0; // tight stable offset so they stay on the path, not on roofs
                (wx + a.cos() * r, wy + a.sin() * r)
            } else {
                (wx, wy)
            };
            c.tx = tx;
            c.ty = ty;
        }
        // move toward the current target
        let c = t.chars.get_mut(&id).unwrap();
        let (dx, dy) = (c.tx - c.x, c.ty - c.y);
        let d = (dx * dx + dy * dy).sqrt();
        if d > 1.5 {
            let step = SPEED * DT;
            c.x += dx / d * step.min(d);
            c.y += dy / d * step.min(d);
            if dx.abs() > 0.4 {
                c.facing = if dx < 0.0 { -1.0 } else { 1.0 };
            }
        }
        // reached a waypoint with more to go: advance along the route
        if !human && d < 9.0 && c.path.len() > 1 {
            c.path.remove(0);
        }
        c.x = c.x.clamp(24.0, 936.0); // stay on-screen
        c.y = c.y.clamp(24.0, 584.0);
        if c.bubble_t > 0.0 {
            c.bubble_t -= DT;
            if c.bubble_t <= 0.0 {
                c.bubble.clear();
            }
        }
        // which location am I standing in?
        let old_here = c.here;
        let mut here = -1i32;
        for (li, _) in LOCS.iter().enumerate() {
            let (sx, sy) = loc_stand(li);
            if (c.x - sx).powi(2) + (c.y - sy).powi(2) < 70.0 * 70.0 {
                here = li as i32;
                break;
            }
        }
        c.here = here;
        // a visitor arriving at a spot nudges that group to speak, so residents
        // notice you walking up instead of only reacting when you type
        if human && here >= 0 && here != old_here {
            arrivals.push(here as usize);
        }
    }
    for li in arrivals {
        t.pending[li] = true;
    }
}

/// Characters currently at location `li`.
fn chars_at(t: &Town, li: i32) -> Vec<u32> {
    t.chars.iter().filter(|(_, c)| c.here == li).map(|(id, _)| *id).collect()
}

/// Pick a scene + speaker + prompt context for the next AI line.
/// Returns (speaker_id, system_prompt, user_prompt) or None.
fn next_utterance(t: &Town) -> Option<(u32, String, String)> {
    // prefer a location where a human just spoke
    let order: Vec<usize> = {
        let mut v: Vec<usize> = (0..LOCS.len()).collect();
        v.sort_by_key(|&i| if t.pending[i] { 0 } else { 1 });
        v
    };
    for li in order {
        let present = chars_at(t, li as i32);
        if present.len() < 2 {
            continue;
        }
        let ai_here: Vec<u32> = present.iter().copied().filter(|id| !t.chars[id].human).collect();
        if ai_here.is_empty() {
            continue;
        }
        // choose the AI who spoke least recently here
        let speaker = *ai_here.iter().min_by(|a, b| t.chars[a].last_spoke.partial_cmp(&t.chars[b].last_spoke).unwrap())?;
        let c = &t.chars[&speaker];
        let others: Vec<&str> = present.iter().filter(|&&id| id != speaker).map(|id| t.chars[id].name.as_str()).collect();
        let locname = LOCS[li].0;
        let transcript = if t.transcripts[li].is_empty() {
            "(it has been quiet)".to_string()
        } else {
            t.transcripts[li].join("\n")
        };
        let system = format!(
            "You are {}, a resident of the small town of Hearth. {}. \
             Right now you are at the {} with {}. \
             Reply with ONE short, natural line (under 22 words) that a real person would actually say here. \
             React to the most recent line, sometimes address someone by name, and vary what you do: share a \
             bit of local news or gossip, give a blunt opinion, tease a friend, ask a question, or mention your \
             own day and trade. Stay grounded in this town and your character. Do not repeat what was just said. \
             If a visitor spoke to you, answer them directly and warmly. No emoji, no name label, no quotes.",
            c.name,
            c.persona,
            locname,
            if others.is_empty() { "no one in particular".to_string() } else { others.join(", ") }
        );
        let user = format!("Recent talk at the {locname}:\n{transcript}\n\nReply as {} (one short line):", c.name);
        return Some((speaker, system, user));
    }
    None
}

fn record_line(t: &mut Town, li: usize, name: &str, line: &str) {
    let entry = format!("{name}: {line}");
    let tr = &mut t.transcripts[li];
    tr.push(entry.clone());
    while tr.len() > 7 {
        tr.remove(0);
    }
    // give everyone present a short memory of it
    let present = chars_at(t, li as i32);
    for id in present {
        if let Some(c) = t.chars.get_mut(&id) {
            c.mem.push(entry.clone());
            while c.mem.len() > 6 {
                c.mem.remove(0);
            }
        }
    }
}

/// Background conversation loop: every couple of seconds, advance one scene.
fn converse(town: Arc<Mutex<Town>>) {
    loop {
        // react quickly when a human is waiting (a pending scene), relax otherwise
        let (job, urgent) = {
            let t = town.lock().unwrap();
            if t.humans == 0 {
                (None, false) // only spend tokens when someone is actually in the town
            } else {
                (next_utterance(&t), t.pending.iter().any(|&p| p))
            }
        };
        thread::sleep(Duration::from_millis(if urgent { 700 } else { 2600 }));
        let Some((speaker, system, user)) = job else { continue };
        let (name, persona, human_facing) = {
            let t = town.lock().unwrap();
            let here = t.chars[&speaker].here;
            let hf = here >= 0 && t.pending[here as usize];
            (t.chars[&speaker].name.clone(), t.chars[&speaker].persona, hf)
        };
        // if the model is rate-limited, a visitor still gets a real greeting, not filler
        let line = ai_say(&system, &user).unwrap_or_else(|| if human_facing { canned_greet(&name) } else { canned(&name, persona) });
        let mut t = town.lock().unwrap();
        let li = t.chars[&speaker].here;
        if li < 0 {
            continue;
        }
        let li = li as usize;
        let now = t.time;
        t.pending[li] = false;
        {
            let c = t.chars.get_mut(&speaker).unwrap();
            c.bubble = line.clone();
            c.bubble_t = 6.0;
            c.last_spoke = now;
        }
        record_line(&mut t, li, &name, &line);
    }
}

// --- OpenRouter via curl (same approach as the chatroom) -------------------

fn ai_say(system: &str, user: &str) -> Option<String> {
    let key = std::env::var("OPENROUTER_API_KEY").ok().filter(|k| !k.is_empty())?;
    let body = format!(
        "{{\"model\":\"{MODEL}\",\"max_tokens\":120,\"temperature\":0.9,\"reasoning\":{{\"enabled\":false}},\
         \"messages\":[{{\"role\":\"system\",\"content\":\"{}\"}},{{\"role\":\"user\",\"content\":\"{}\"}}]}}",
        json_escape(system),
        json_escape(user)
    );
    let out = Command::new("curl")
        .args([
            "-s", "-m", "20", "--connect-timeout", "8", "-X", "POST", "https://openrouter.ai/api/v1/chat/completions",
            "-H", &format!("Authorization: Bearer {key}"), "-H", "Content-Type: application/json", "-d", &body,
        ])
        .output()
        .ok()?;
    let resp = String::from_utf8_lossy(&out.stdout);
    let line = sanitize(&extract_content(&resp)?);
    if line.is_empty() { None } else { Some(line) }
}

/// Fallback flavour lines when there is no API key / the call fails.
fn canned(name: &str, persona: &str) -> String {
    let base = [
        "Lovely weather for it, isn't it?",
        "Did you hear what happened by the market?",
        "Too much to do and too little time today.",
        "Sit a while, no need to rush off.",
        "Prices again, everything costs more these days.",
        "Have you eaten? You look a little hungry.",
        "Quiet morning, just how I like it.",
        "I keep meaning to fix that fence by the well.",
        "They say it might rain before evening.",
        "Otto swears the fish are biting again.",
        "My back is not what it used to be.",
        "Have you been down to the garden lately?",
        "The tavern was lively last night, I hear.",
        "Mind how you go on those cobbles.",
        "I could do with a hot cup of something.",
        "New faces in town, always good to see.",
        "Bit of a chill in the air, wrap up warm.",
        "Tell me, what brings you our way?",
    ];
    let h: usize = name.bytes().map(|b| b as usize).sum::<usize>() * 7 + persona.len() * 3;
    base[h % base.len()].to_string()
}

/// Visitor-facing fallback: used when a human is present but the model is unavailable,
/// so newcomers are still greeted rather than ignored.
fn canned_greet(name: &str) -> String {
    let base = [
        "Welcome, stranger. Make yourself at home.",
        "Good to see a new face. What brings you our way?",
        "Hello there. Lovely day to wander, isn't it?",
        "Pull up a spot, traveler. What's on your mind?",
        "Ah, a visitor. How can I help you today?",
        "You're new here, aren't you? Welcome to Hearth.",
        "Mind the cobbles, friend, and stay a while.",
        "Hello, hello. Come, tell me your story.",
    ];
    let h: usize = name.bytes().map(|b| b as usize).sum::<usize>() * 5 + 3;
    base[h % base.len()].to_string()
}

fn sanitize(s: &str) -> String {
    let mut t: String = s.trim().replace(['\n', '\r', '\t', '|', ';'], " ");
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        t = t[1..t.len() - 1].to_string();
    }
    t.trim().chars().take(160).collect()
}

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

fn extract_content(resp: &str) -> Option<String> {
    let from = resp.find("\"message\"").unwrap_or(0);
    let key = "\"content\":\"";
    let start = resp[from..].find(key)? + from + key.len();
    Some(decode_json_string(&resp[start..]))
}

fn decode_json_string(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => break,
            '\\' => match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => {}
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('/') => out.push('/'),
                Some('u') => {
                    let hex: String = chars.by_ref().take(4).collect();
                    if let Ok(n) = u32::from_str_radix(&hex, 16) {
                        if let Some(ch) = char::from_u32(n) {
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

// --- snapshots & networking ------------------------------------------------

fn map_line() -> String {
    let locs: String = LOCS
        .iter()
        .map(|(n, x, y, k)| format!("{n},{:.0},{:.0},{k}", x, y))
        .collect::<Vec<_>>()
        .join(";");
    format!("map\t{COLS}\t{ROWS}\t{}\t{locs}\n", TILE as i32)
}

/// One-line persona blurbs, sent once on join so the inspect card can show who a
/// resident actually is (not just their job).
fn bio_line() -> String {
    let bios: String = RESIDENTS
        .iter()
        .enumerate()
        .map(|(i, r)| format!("{}|{}", i as u32 + 1, r.1.replace(['|', ';', '\t', '\n'], " ")))
        .collect::<Vec<_>>()
        .join(";");
    format!("bio\t{bios}\n")
}

fn snapshot(t: &Town, you: u32) -> String {
    let mut s = String::new();
    s.push_str(&format!("clk\t{:.3}\n", (t.time % DAY_SECS) / DAY_SECS));
    // positions
    let p: String = t
        .chars
        .iter()
        .map(|(id, c)| format!("{id},{:.0},{:.0},{},{},{}", c.x, c.y, c.pal, if c.facing < 0.0 { 0 } else { 1 }, if *id == you { 1 } else { 0 }))
        .collect::<Vec<_>>()
        .join(";");
    s.push_str(&format!("p\t{p}\n"));
    // roster (name + role + activity)
    let r: String = t
        .chars
        .iter()
        .map(|(id, c)| {
            let act = if c.here >= 0 { LOCS[c.here as usize].0 } else { "walking" };
            let kind = if c.human { "you/visitor" } else { c.role };
            format!("{id}|{}|{kind}|{act}", c.name)
        })
        .collect::<Vec<_>>()
        .join(";");
    s.push_str(&format!("r\t{r}\n"));
    // active speech bubbles
    let b: String = t
        .chars
        .iter()
        .filter(|(_, c)| !c.bubble.is_empty())
        .map(|(id, c)| format!("{id}|{}", c.bubble))
        .collect::<Vec<_>>()
        .join(";");
    s.push_str(&format!("b\t{b}\n"));
    s
}

fn main() {
    let town = Arc::new(Mutex::new(new_town()));
    if std::env::var("OPENROUTER_API_KEY").map_or(true, |k| k.is_empty()) {
        eprintln!("WARNING: OPENROUTER_API_KEY not set, residents will use canned lines.");
    }
    {
        let town = town.clone();
        thread::spawn(move || loop {
            thread::sleep(Duration::from_millis(TICK_MS));
            tick(&mut town.lock().unwrap());
        });
    }
    {
        let town = town.clone();
        thread::spawn(move || converse(town));
    }
    let listener = TcpListener::bind(("0.0.0.0", PORT)).expect("bind");
    println!("opcusdb Hearth (AI town) on http://localhost:{PORT}");
    for stream in listener.incoming().flatten() {
        let town = town.clone();
        thread::spawn(move || handle(stream, town));
    }
}

fn handle(mut stream: TcpStream, town: Arc<Mutex<Town>>) {
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
    let _ = stream.set_nodelay(true);

    // spawn a human visitor in the plaza
    let id = {
        let mut t = town.lock().unwrap();
        let id = t.next_id;
        t.next_id += 1;
        t.next_client += 1;
        t.humans += 1;
        let (sx, sy) = loc_stand(0);
        let vname = format!("Visitor {}", t.next_client - 1);
        t.chars.insert(
            id,
            Char {
                x: sx, y: sy, tx: sx, ty: sy,
                name: vname,
                persona: "", role: "visitor", pal: 99, work: 0, fav: 0, here: 0,
                bubble: String::new(), bubble_t: 0.0, last_spoke: 0.0, facing: 1.0, goal: 0, path: Vec::new(), human: true, mem: Vec::new(),
            },
        );
        id
    };
    let _ = ws::write_text(&mut stream, &format!("w\t{id}"));
    let _ = ws::write_text(&mut stream, &map_line());
    let _ = ws::write_text(&mut stream, &bio_line());

    let mut writer = stream.try_clone().expect("clone");
    let wtown = town.clone();
    let writer_handle = thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(TICK_MS * 3)); // ~7 Hz snapshots
        let snap = snapshot(&wtown.lock().unwrap(), id);
        if ws::write_text(&mut writer, &snap).is_err() {
            return;
        }
    });

    loop {
        match ws::read_frame(&mut stream) {
            Ok(Some(ws::Msg::Text(t))) => {
                let (cmd, rest) = t.split_once(' ').unwrap_or((t.as_str(), ""));
                match cmd {
                    "name" => {
                        let nm: String = rest.chars().filter(|c| !c.is_control()).take(14).collect();
                        if !nm.trim().is_empty() {
                            if let Some(c) = town.lock().unwrap().chars.get_mut(&id) {
                                c.name = nm;
                            }
                        }
                    }
                    "go" => {
                        let v: Vec<f32> = rest.split_whitespace().filter_map(|s| s.parse().ok()).collect();
                        if v.len() == 2 {
                            if let Some(c) = town.lock().unwrap().chars.get_mut(&id) {
                                c.tx = v[0].clamp(0.0, COLS as f32 * TILE);
                                c.ty = v[1].clamp(0.0, ROWS as f32 * TILE);
                            }
                        }
                    }
                    "say" => {
                        let line = sanitize(rest);
                        if !line.is_empty() {
                            let mut tt = town.lock().unwrap();
                            let (nm, here) = {
                                let c = tt.chars.get_mut(&id).unwrap();
                                c.bubble = line.clone();
                                c.bubble_t = 6.0;
                                (c.name.clone(), c.here)
                            };
                            if here >= 0 {
                                let li = here as usize;
                                tt.pending[li] = true;
                                record_line(&mut tt, li, &nm, &line);
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
        let mut t = town.lock().unwrap();
        t.chars.remove(&id);
        t.humans = t.humans.saturating_sub(1);
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
    let (ctype, body): (&str, Vec<u8>) = match path {
        "/" | "/index.html" => {
            let html = include_str!("../web/town.html").replace("<script src=\"/town.js\"></script>", &format!("<script>\n{}\n</script>", include_str!("../web/town.js")));
            ("text/html; charset=utf-8", html.into_bytes())
        }
        "/town.js" => ("application/javascript; charset=utf-8", include_str!("../web/town.js").as_bytes().to_vec()),
        "/town-bg.png" => ("image/png", include_bytes!("../web/town-bg.png").to_vec()),
        "/town-sprites.png" => ("image/png", include_bytes!("../web/town-sprites.png").to_vec()),
        _ => {
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
            return;
        }
    };
    let resp = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {ctype}\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(resp.as_bytes());
    let _ = stream.write_all(&body);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_keeps_residents_spread_and_goes_home_at_night() {
        let t = new_town();
        let c = &t.chars[&1]; // Mara, work = Bakery (1)
        assert_eq!(schedule(c, DAY_SECS * 0.85), c.fav, "evening -> favourite spot");
        assert_eq!(schedule(c, DAY_SECS * 0.97), 8, "night -> Homes");
        // daytime: she is either at her workplace or rotating through the plaza, never elsewhere
        let day = schedule(c, DAY_SECS * 0.4);
        assert!(day == c.work || day == 0, "daytime -> own workplace or the plaza");
    }

    #[test]
    fn co_located_characters_form_a_scene_with_a_speaker() {
        let mut t = new_town();
        // park everyone at the plaza
        for c in t.chars.values_mut() {
            let (sx, sy) = loc_stand(0);
            c.x = sx;
            c.y = sy;
            c.here = 0;
        }
        let present = chars_at(&t, 0);
        assert!(present.len() >= 2);
        let job = next_utterance(&t);
        assert!(job.is_some(), "a scene with 2+ characters yields a speaker + prompt");
        let (sid, sys, _user) = job.unwrap();
        assert!(!t.chars[&sid].human, "speaker is an AI resident");
        assert!(sys.contains("Hearth"), "prompt grounds the character in the town");
    }

    #[test]
    fn a_human_line_marks_the_location_pending() {
        let mut t = new_town();
        for c in t.chars.values_mut() {
            c.here = 0;
        }
        record_line(&mut t, 0, "Visitor 1", "hello everyone");
        t.pending[0] = true;
        assert!(t.pending[0]);
        assert!(t.transcripts[0].iter().any(|l| l.contains("hello everyone")));
    }
}
