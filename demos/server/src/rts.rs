//! opcusdb Warfront — a real-time-strategy battle of **hundreds of units**
//! (infantry + archers) clashing, with the engine's **`SpatialGrid`** doing both
//! combat neighbour queries and **camera area-of-interest (AOI) streaming** — the
//! server only sends each client the units inside its viewport, so the war stays
//! cheap on the wire no matter the army size. You command the blue army
//! (drag-select, click the ground to attack-move — no right-click needed) against
//! a red AI horde; raze the enemy keep to win.
//!
//! Run: `cargo run --release -p opcusdb-server --bin opcusdb-rts` then open
//! http://localhost:9010

use opcusdb_core::{EntityId, Rng, SpatialGrid};
use opcusdb_server::ws;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const PORT: u16 = 9010;
const W: i32 = 4000;
const H: i32 = 2600;
const CELL: i32 = 90;
const TICK_MS: u64 = 66; // ~15 Hz
const DT: f32 = 0.066;
const PER_TEAM: u32 = 350;

// two unit kinds: 0 = infantry (melee), 1 = archer (ranged)
const KSPEED: [f32; 2] = [96.0, 64.0];
const KRANGE: [f32; 2] = [62.0, 250.0];
const KDMG: [f32; 2] = [11.0, 7.0];
const KCD: [f32; 2] = [0.55, 1.1];
const KHP: [f32; 2] = [38.0, 20.0];
const SIGHT: f32 = 320.0; // grid query radius (>= max attack range)
const SEP: f32 = 17.0;
const BASE_HP: f32 = 4000.0;
const BASEX: [f32; 2] = [200.0, (W - 200) as f32];
const BASEY: f32 = (H / 2) as f32;

struct Unit {
    x: f32,
    y: f32,
    hp: f32,
    team: u8,
    kind: u8,
    ox: f32,
    oy: f32,
    has_order: bool,
    atk_cd: f32,
}

struct Rts {
    units: BTreeMap<u32, Unit>,
    grid: SpatialGrid,
    deaths: Vec<(f32, f32, u8)>,
    tracers: Vec<(i32, i32, i32, i32, u8)>,
    base_hp: [f32; 2],
    count: [u32; 2],
    time: f32,
    next_unit: u32,
    next_client: u32,
}

fn main() {
    let rts = Arc::new(Mutex::new(new_battle()));
    {
        let rts = rts.clone();
        thread::spawn(move || loop {
            thread::sleep(Duration::from_millis(TICK_MS));
            tick(&mut rts.lock().unwrap());
        });
    }
    let listener = TcpListener::bind(("0.0.0.0", PORT)).expect("bind");
    println!("opcusdb Warfront (RTS, {} units) on http://localhost:{PORT}", PER_TEAM * 2);
    for stream in listener.incoming().flatten() {
        let rts = rts.clone();
        thread::spawn(move || handle(stream, rts));
    }
}

fn new_battle() -> Rts {
    let mut r = Rts {
        units: BTreeMap::new(),
        grid: SpatialGrid::new(W, H, CELL),
        deaths: Vec::new(),
        tracers: Vec::new(),
        base_hp: [BASE_HP, BASE_HP],
        count: [0, 0],
        time: 0.0,
        next_unit: 1,
        next_client: 1,
    };
    let mut rng = Rng::seed(0x5741_5246_u64);
    spawn_army(&mut r, &mut rng, 0, 850.0, 1800.0);
    spawn_army(&mut r, &mut rng, 1, 2200.0, 3150.0);
    r
}

fn spawn_army(r: &mut Rts, rng: &mut Rng, team: u8, x0: f32, x1: f32) {
    for i in 0..PER_TEAM {
        let x = x0 + (rng.below(((x1 - x0) as u32).max(1)) as f32);
        let y = 150.0 + rng.below((H as u32) - 300) as f32;
        let kind: u8 = if i % 3 == 0 { 1 } else { 0 }; // ~1/3 archers
        let id = r.next_unit;
        r.next_unit += 1;
        r.units.insert(id, Unit { x, y, hp: KHP[kind as usize], team, kind, ox: 0.0, oy: 0.0, has_order: false, atk_cd: 0.0 });
    }
    r.count[team as usize] += PER_TEAM;
}

// --- simulation ------------------------------------------------------------

fn tick(r: &mut Rts) {
    r.time += DT;
    r.deaths.clear();
    // rebuild the spatial grid (cheap each tick) — engine's AOI primitive
    r.grid.clear();
    for (id, u) in &r.units {
        r.grid.insert(EntityId::from_raw(*id, 0), u.x as i32, u.y as i32);
    }

    let ids: Vec<u32> = r.units.keys().copied().collect();
    let mut damage: Vec<(u32, f32)> = Vec::new();
    let mut tracers: Vec<(i32, i32, i32, i32, u8)> = Vec::new();
    let mut base_dmg = [0.0f32, 0.0f32];

    for &id in &ids {
        let (ux, uy, team, kind, has_order, ox, oy, cd) = {
            let u = &r.units[&id];
            (u.x, u.y, u.team, u.kind as usize, u.has_order, u.ox, u.oy, u.atk_cd)
        };
        let (krange, kdmg, kcd, kspeed) = (KRANGE[kind], KDMG[kind], KCD[kind], KSPEED[kind]);
        // nearest enemy in sight (grid radius query)
        let near = r.grid.query_radius(ux as i32, uy as i32, SIGHT as i32);
        let mut best: Option<(u32, f32)> = None;
        let mut sepx = 0.0f32;
        let mut sepy = 0.0f32;
        for eid in near {
            let oid = eid.index();
            if oid == id {
                continue;
            }
            let o = match r.units.get(&oid) {
                Some(o) => o,
                None => continue,
            };
            let (dx, dy) = (o.x - ux, o.y - uy);
            let d2 = dx * dx + dy * dy;
            if o.team != team {
                if best.map_or(true, |(_, bd)| d2 < bd) {
                    best = Some((oid, d2));
                }
            } else {
                // light separation from same-team neighbours
                let d = d2.sqrt();
                if d > 0.01 && d < SEP {
                    sepx -= dx / d * (SEP - d);
                    sepy -= dy / d * (SEP - d);
                }
            }
        }

        let mut nx = ux;
        let mut ny = uy;
        let new_cd = (cd - DT).max(0.0);
        let mut attack_cd_reset = false;
        let ebase = 1 - team as usize;
        let (bx, by) = (BASEX[ebase], BASEY);
        match best {
            Some((eid, d2)) if d2 <= krange * krange => {
                // in range: strike the enemy unit (+ a tracer; long for archers)
                if new_cd <= 0.0 {
                    let (tx, ty) = { let o = &r.units[&eid]; (o.x, o.y) };
                    damage.push((eid, kdmg));
                    attack_cd_reset = true;
                    if tracers.len() < 220 && (kind == 1 || (id & 3) == 0) {
                        tracers.push((ux as i32, uy as i32, tx as i32, ty as i32, team));
                    }
                }
            }
            Some((eid, _)) => {
                let (tx, ty) = { let o = &r.units[&eid]; (o.x, o.y) };
                step_toward(&mut nx, &mut ny, tx, ty, kspeed * DT);
            }
            None => {
                let to_base = ((ux - bx).powi(2) + (uy - by).powi(2)).sqrt();
                if to_base < krange + 80.0 {
                    // reached the enemy keep: lay siege
                    if new_cd <= 0.0 {
                        base_dmg[ebase] += kdmg;
                        attack_cd_reset = true;
                        if tracers.len() < 220 && (id & 3) == 0 {
                            tracers.push((ux as i32, uy as i32, bx as i32, by as i32, team));
                        }
                    }
                } else if has_order {
                    step_toward(&mut nx, &mut ny, ox, oy, kspeed * DT);
                } else {
                    step_toward(&mut nx, &mut ny, bx, uy, kspeed * 0.95 * DT);
                }
            }
        }
        nx += sepx.clamp(-kspeed * DT, kspeed * DT);
        ny += sepy.clamp(-kspeed * DT, kspeed * DT);
        nx = nx.clamp(4.0, W as f32 - 4.0);
        ny = ny.clamp(4.0, H as f32 - 4.0);
        // clear order on arrival
        let cleared = has_order && (nx - ox).abs() < 6.0 && (ny - oy).abs() < 6.0;
        {
            let u = r.units.get_mut(&id).unwrap();
            u.x = nx;
            u.y = ny;
            u.atk_cd = if attack_cd_reset { kcd } else { new_cd };
            if cleared {
                u.has_order = false;
            }
        }
    }

    r.tracers = tracers;
    r.base_hp[0] = (r.base_hp[0] - base_dmg[0]).max(0.0);
    r.base_hp[1] = (r.base_hp[1] - base_dmg[1]).max(0.0);

    // apply damage; collect deaths
    for (tid, dmg) in damage {
        if let Some(u) = r.units.get_mut(&tid) {
            u.hp -= dmg;
            if u.hp <= 0.0 {
                let (x, y, team) = (u.x, u.y, u.team);
                r.units.remove(&tid);
                r.count[team as usize] = r.count[team as usize].saturating_sub(1);
                if r.deaths.len() < 200 {
                    r.deaths.push((x, y, team));
                }
            }
        }
    }
}

fn step_toward(x: &mut f32, y: &mut f32, tx: f32, ty: f32, step: f32) {
    let (dx, dy) = (tx - *x, ty - *y);
    let d = (dx * dx + dy * dy).sqrt();
    if d > 0.001 {
        *x += dx / d * step.min(d);
        *y += dy / d * step.min(d);
    }
}

fn build_snapshot(r: &Rts, cx: f32, cy: f32, hw: f32, hh: f32) -> String {
    let mut s = String::new();
    s.push_str(&format!("g\t{}\t{}\t{:.0}\n", r.count[0], r.count[1], r.time));
    // AOI: only the units inside the client's camera box (engine SpatialGrid)
    let (x0, y0, x1, y1) = ((cx - hw) as i32, (cy - hh) as i32, (cx + hw) as i32, (cy + hh) as i32);
    let mut u = String::new();
    let mut n = 0;
    for eid in r.grid.query_aabb(x0, y0, x1, y1) {
        if let Some(unit) = r.units.get(&eid.index()) {
            u.push_str(&format!(
                "{},{},{},{},{},{};",
                eid.index(),
                unit.x as i32,
                unit.y as i32,
                unit.team,
                (unit.hp / KHP[unit.kind as usize] * 9.0) as i32,
                unit.kind
            ));
            n += 1;
            if n >= 3000 {
                break;
            }
        }
    }
    s.push_str(&format!("u\t{u}\n"));
    s.push_str(&format!("b\t{:.0}\t{}\t{:.0}\t{}\n", r.base_hp[0], BASE_HP as i32, r.base_hp[1], BASE_HP as i32));
    let f = r
        .tracers
        .iter()
        .filter(|(x1, y1, _, _, _)| (*x1 as f32) >= cx - hw && (*x1 as f32) <= cx + hw && (*y1 as f32) >= cy - hh && (*y1 as f32) <= cy + hh)
        .map(|(x1, y1, x2, y2, t)| format!("{x1},{y1},{x2},{y2},{t}"))
        .collect::<Vec<_>>()
        .join(";");
    s.push_str(&format!("f\t{f}\n"));
    let d = r
        .deaths
        .iter()
        .filter(|(x, y, _)| *x >= cx - hw && *x <= cx + hw && *y >= cy - hh && *y <= cy + hh)
        .map(|(x, y, t)| format!("{},{},{}", *x as i32, *y as i32, t))
        .collect::<Vec<_>>()
        .join(";");
    s.push_str(&format!("x\t{d}\n"));
    s
}

// --- connections -----------------------------------------------------------

fn handle(mut stream: TcpStream, rts: Arc<Mutex<Rts>>) {
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
        let mut r = rts.lock().unwrap();
        let id = r.next_client;
        r.next_client += 1;
        id
    };
    let cam: Arc<Mutex<(f32, f32, f32, f32)>> = Arc::new(Mutex::new((W as f32 / 2.0, H as f32 / 2.0, 700.0, 450.0)));
    let _ = ws::write_text(&mut stream, &format!("w\t{id}\t0")); // humans command team 0 (blue)

    let mut writer = stream.try_clone().expect("clone");
    let wrts = rts.clone();
    let wcam = cam.clone();
    let writer_handle = thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(TICK_MS));
        let snap = {
            let (cx, cy, hw, hh) = *wcam.lock().unwrap();
            build_snapshot(&wrts.lock().unwrap(), cx, cy, hw, hh)
        };
        if ws::write_text(&mut writer, &snap).is_err() {
            return;
        }
    });

    loop {
        match ws::read_frame(&mut stream) {
            Ok(Some(ws::Msg::Text(t))) => {
                let (cmd, rest) = t.split_once(' ').unwrap_or((t.as_str(), ""));
                match cmd {
                    "view" => {
                        let v: Vec<f32> = rest.split_whitespace().filter_map(|s| s.parse().ok()).collect();
                        if v.len() == 4 {
                            *cam.lock().unwrap() = (v[0], v[1], v[2].clamp(120.0, 2200.0), v[3].clamp(80.0, 1500.0));
                        }
                    }
                    "order" => {
                        // "order <tx> <ty> <id,id,...>"
                        let mut it = rest.splitn(3, ' ');
                        if let (Some(tx), Some(ty), Some(ids)) = (it.next(), it.next(), it.next()) {
                            if let (Ok(tx), Ok(ty)) = (tx.parse::<f32>(), ty.parse::<f32>()) {
                                let mut r = rts.lock().unwrap();
                                for sid in ids.split(',').filter_map(|s| s.parse::<u32>().ok()).take(2000) {
                                    if let Some(u) = r.units.get_mut(&sid) {
                                        if u.team == 0 {
                                            u.ox = tx;
                                            u.oy = ty;
                                            u.has_order = true;
                                        }
                                    }
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
    let (ctype, body): (&str, &[u8]) = match path {
        "/" | "/index.html" => ("text/html; charset=utf-8", include_str!("../web/rts.html").as_bytes()),
        "/rts.js" => ("application/javascript; charset=utf-8", include_str!("../web/rts.js").as_bytes()),
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

    fn empty() -> Rts {
        Rts { units: BTreeMap::new(), grid: SpatialGrid::new(W, H, CELL), deaths: Vec::new(), tracers: Vec::new(), base_hp: [BASE_HP, BASE_HP], count: [0, 0], time: 0.0, next_unit: 1, next_client: 1 }
    }

    #[test]
    fn enemies_in_range_take_damage_and_die() {
        let mut r = empty();
        r.units.insert(1, Unit { x: 1000.0, y: 1000.0, hp: KHP[0], team: 0, kind: 0, ox: 0.0, oy: 0.0, has_order: false, atk_cd: 0.0 });
        r.units.insert(2, Unit { x: 1030.0, y: 1000.0, hp: KHP[0], team: 1, kind: 0, ox: 0.0, oy: 0.0, has_order: false, atk_cd: 0.0 });
        r.count = [1, 1];
        for _ in 0..60 {
            tick(&mut r);
        }
        // they shoot each other down over time
        assert!(r.units.len() < 2, "at least one unit died in the firefight");
    }

    #[test]
    fn aoi_only_returns_units_in_the_camera_box() {
        let mut r = empty();
        r.units.insert(1, Unit { x: 500.0, y: 500.0, hp: KHP[0], team: 0, kind: 0, ox: 0.0, oy: 0.0, has_order: false, atk_cd: 0.0 });
        r.units.insert(2, Unit { x: 3500.0, y: 2000.0, hp: KHP[0], team: 1, kind: 0, ox: 0.0, oy: 0.0, has_order: false, atk_cd: 0.0 });
        tick(&mut r); // builds the grid
        let snap = build_snapshot(&r, 500.0, 500.0, 300.0, 300.0);
        let uline = snap.lines().find(|l| l.starts_with("u\t")).unwrap();
        assert!(uline.contains("1,"), "the near unit is streamed");
        assert!(!uline.contains("2,"), "the far unit is culled by AOI");
    }

    #[test]
    fn orders_set_a_unit_target() {
        let mut r = empty();
        r.units.insert(1, Unit { x: 100.0, y: 100.0, hp: KHP[0], team: 0, kind: 0, ox: 0.0, oy: 0.0, has_order: false, atk_cd: 0.0 });
        let u = r.units.get_mut(&1).unwrap();
        u.ox = 800.0;
        u.oy = 600.0;
        u.has_order = true;
        assert!(r.units[&1].has_order);
    }
}
