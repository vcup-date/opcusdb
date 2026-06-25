// opcusdb Rampart — tower-defense client (Kingdom-Rush-style presentation).
// The Rust server is authoritative; this renders a smooth, animated view of the
// state it broadcasts and sends two commands: build a tower, start a wave.

const $ = (id) => document.getElementById(id);
const cv = $("cv"), ctx = cv.getContext("2d");

let ws = null;
let COLS = 20, ROWS = 12, TILE = 48;
let road = new Set();           // "c,r" buildable mask
let way = [];                   // path waypoints [[x,y]...]
let base = [19, 10];
const creeps = new Map();       // id -> {dx,dy,tx,ty,kind,hp,slow,fa,phase}
let towers = [];                // {c,r,kind,aim,recoil,flash}
let projs = [];                 // {x,y,kind}
let gold = 0, lives = 0, wave = 0, maxWave = 12, state = 0, players = 1;
let pick = 0, hover = null;
let decor = [];                 // {x,y,type}
const ROOM = new URLSearchParams(location.search).get("room");

const COST = [50, 110, 75], RANGE = [120, 135, 105];
// kind-specific creep look: [body, dark, radius, bobSpeed]
const CREEP = [
  { body: "#5fd16a", dark: "#2f7a37", r: 11, bob: 9 },   // normal goblin
  { body: "#ffb14a", dark: "#b06a16", r: 8, bob: 14 },   // fast imp
  { body: "#b07bff", dark: "#5a36a0", r: 16, bob: 6 },   // tank ogre
];

// ---- networking (auto-reconnect) ------------------------------------------
function setConn(ok) { const c = $("conn"); if (!c) return; c.textContent = ok ? "● online" : "● offline"; c.style.color = ok ? "#3ec46a" : "#ff6b6b"; }
function connect() {
  ws = new WebSocket(`ws://${location.host}/ws${ROOM ? "?room=" + encodeURIComponent(ROOM) : ""}`);
  ws.onopen = () => setConn(true);
  ws.onclose = () => { setConn(false); const w = $("wave"); w.className = ""; w.textContent = "⟳ reconnecting…"; w.disabled = true; setTimeout(connect, 1000); };
  ws.onerror = () => {};
  ws.onmessage = (e) => {
    for (const line of e.data.split("\n")) {
      if (!line) continue;
      const i = line.indexOf("\t"), tag = i < 0 ? line : line.slice(0, i), rest = i < 0 ? "" : line.slice(i + 1);
      if (tag === "map") {
        const p = rest.split("\t"); COLS = +p[0]; ROWS = +p[1]; TILE = +p[2];
        road = new Set((p[3] || "").split(";").filter(Boolean));
        base = (p[4] || "19,10").split(",").map(Number);
        way = (p[5] || "").split(";").filter(Boolean).map(s => s.split(",").map(Number));
        cv.width = COLS * TILE; cv.height = ROWS * TILE;
        buildDecor();
      } else if (tag === "s") {
        const p = rest.split("\t"); gold = +p[0]; lives = +p[1]; wave = +p[2]; maxWave = +p[3]; state = +p[4]; updateHud();
      } else if (tag === "e") {
        const seen = new Set();
        for (const s of rest.split(";")) {
          if (!s) continue;
          const a = s.split(","); const id = +a[0], x = +a[1], y = +a[2], hp = +a[3], kind = +a[4], slow = +a[5];
          seen.add(id);
          let c = creeps.get(id);
          if (!c) { c = { dx: x, dy: y, fa: 0, phase: (id * 1.7) % 6.28 }; creeps.set(id, c); }
          c.tx = x; c.ty = y; c.hp = hp; c.kind = kind; c.slow = slow;
        }
        for (const id of [...creeps.keys()]) if (!seen.has(id)) creeps.delete(id);
      } else if (tag === "t") {
        const next = rest.split(";").filter(Boolean).map(s => { const a = s.split(","); return { c:+a[0], r:+a[1], kind:+a[2] }; });
        // preserve turret animation state across snapshots
        towers = next.map(n => { const o = towers.find(t => t.c === n.c && t.r === n.r); return Object.assign({ aim: -1.57, recoil: 0, flash: 0 }, o, n); });
      } else if (tag === "p") {
        projs = rest.split(";").filter(Boolean).map(s => { const a = s.split(","); return { x:+a[0], y:+a[1], kind:+a[2] }; });
      } else if (tag === "n") {
        players = +rest || 1; renderMp();
      }
    }
  };
}

function updateHud() {
  $("gold").innerHTML = gold + '<small>GOLD</small>';
  $("lives").innerHTML = lives + '<small>LIVES</small>';
  $("wavn").innerHTML = wave + '/' + maxWave + '<small>WAVE</small>';
  const wb = $("wave");
  if (state < 2 && wave < maxWave) { wb.className = "ready"; wb.disabled = false; wb.textContent = (state === 1 ? "▶ Call Wave " : "▶ Start Wave ") + (wave + 1); }
  else if (state === 1) { wb.className = ""; wb.disabled = true; wb.textContent = "Final wave…"; }
  else { wb.className = ""; wb.disabled = true; wb.textContent = "—"; }
  $("over").style.display = (state >= 2) ? "flex" : "none";
  if (state >= 2) { $("otxt").textContent = state === 2 ? "VICTORY 🏆" : "DEFEAT"; $("otxt").style.color = state === 2 ? "#3ec46a" : "#ff5d5d"; }
  document.querySelectorAll(".tw").forEach(el => el.classList.toggle("poor", gold < COST[+el.dataset.k]));
}

// deterministic scenery on non-road tiles
function buildDecor() {
  decor = []; let s = 99;
  const rnd = () => (s = (s * 1103515245 + 12345) & 0x7fffffff) / 0x7fffffff;
  for (let r = 0; r < ROWS; r++) for (let c = 0; c < COLS; c++) {
    if (road.has(c + "," + r) || (c === base[0] && r === base[1])) continue;
    const v = rnd();
    if (v > 0.86) decor.push({ x: (c + 0.5) * TILE + (rnd() - 0.5) * 14, y: (r + 0.5) * TILE + (rnd() - 0.5) * 14, type: v > 0.94 ? "rock" : "tree", s: 0.7 + rnd() * 0.5 });
  }
}

// ---- drawing helpers ------------------------------------------------------
function drawRoad() {
  if (way.length < 2) return;
  ctx.lineCap = "round"; ctx.lineJoin = "round";
  ctx.strokeStyle = "#3a2c1c"; ctx.lineWidth = 40; stroke();
  ctx.strokeStyle = "#7a6038"; ctx.lineWidth = 32; stroke();
  ctx.strokeStyle = "#8c7044"; ctx.lineWidth = 22; stroke();
  ctx.setLineDash([2, 16]); ctx.strokeStyle = "#6a522f"; ctx.lineWidth = 3; stroke(); ctx.setLineDash([]);
  function stroke() { ctx.beginPath(); ctx.moveTo(way[0][0], way[0][1]); for (let i = 1; i < way.length; i++) ctx.lineTo(way[i][0], way[i][1]); ctx.stroke(); }
}
function drawPortal(t) {
  const [x, y] = way[0]; const pr = 17 + Math.sin(t * 3) * 2;
  for (let i = 0; i < 3; i++) { ctx.strokeStyle = `rgba(150,90,255,${0.5 - i * 0.13})`; ctx.lineWidth = 3; ctx.beginPath(); ctx.arc(x, y, pr - i * 4, t * 2 + i, t * 2 + i + 4.5); ctx.stroke(); }
  ctx.fillStyle = "rgba(120,60,220,0.35)"; ctx.beginPath(); ctx.arc(x, y, 9, 0, 7); ctx.fill();
}
function drawKeep(t) {
  const x = (base[0] + 0.5) * TILE, y = (base[1] + 0.5) * TILE, s = TILE * 0.5;
  ctx.fillStyle = "rgba(0,0,0,0.28)"; ctx.beginPath(); ctx.ellipse(x, y + s * 0.7, s, s * 0.4, 0, 0, 7); ctx.fill();
  ctx.fillStyle = "#9aa1ae"; ctx.fillRect(x - s * 0.75, y - s * 0.4, s * 1.5, s * 1.2);
  ctx.fillStyle = "#c2c8d2"; for (let i = 0; i < 4; i++) ctx.fillRect(x - s * 0.75 + i * s * 0.42, y - s * 0.65, s * 0.28, s * 0.32);
  ctx.fillStyle = "#7d828f"; ctx.fillRect(x - s * 0.2, y - s * 0.1, s * 0.4, s * 0.7); // door
  ctx.strokeStyle = "#ffc24a"; ctx.lineWidth = 2; ctx.beginPath(); ctx.moveTo(x, y - s * 0.65); ctx.lineTo(x, y - s * 1.15); ctx.stroke();
  ctx.fillStyle = "#ffc24a"; ctx.beginPath(); ctx.moveTo(x, y - s * 1.15); ctx.lineTo(x + s * 0.4, y - s * 1.02); ctx.lineTo(x, y - s * 0.9); ctx.closePath(); ctx.fill();
}
function drawDecor() {
  for (const d of decor) {
    if (d.type === "rock") { ctx.fillStyle = "#6b7280"; ctx.beginPath(); ctx.ellipse(d.x, d.y, 9 * d.s, 7 * d.s, 0, 0, 7); ctx.fill(); ctx.fillStyle = "#878e99"; ctx.beginPath(); ctx.ellipse(d.x - 2, d.y - 2, 4 * d.s, 3 * d.s, 0, 0, 7); ctx.fill(); }
    else { ctx.fillStyle = "rgba(0,0,0,0.22)"; ctx.beginPath(); ctx.ellipse(d.x, d.y + 9 * d.s, 9 * d.s, 3.5 * d.s, 0, 0, 7); ctx.fill();
      ctx.fillStyle = "#5a3a22"; ctx.fillRect(d.x - 2 * d.s, d.y, 4 * d.s, 10 * d.s);
      ctx.fillStyle = "#2f7d3a"; ctx.beginPath(); ctx.arc(d.x, d.y - 4 * d.s, 11 * d.s, 0, 7); ctx.fill();
      ctx.fillStyle = "#3a9a49"; ctx.beginPath(); ctx.arc(d.x - 3 * d.s, d.y - 7 * d.s, 6 * d.s, 0, 7); ctx.fill(); }
  }
}
function drawCreep(c, t) {
  const k = CREEP[c.kind] || CREEP[0], r = k.r;
  const bob = Math.sin(t * k.bob + c.phase) * 1.6;
  const x = c.dx, y = c.dy + bob - r * 0.4;
  // shadow
  ctx.fillStyle = "rgba(0,0,0,0.25)"; ctx.beginPath(); ctx.ellipse(c.dx, c.dy + r * 0.5, r * 0.95, r * 0.4, 0, 0, 7); ctx.fill();
  // feet (walk cycle)
  const f = Math.sin(t * k.bob * 1.3 + c.phase) * r * 0.45;
  ctx.fillStyle = k.dark;
  ctx.beginPath(); ctx.ellipse(x - r * 0.45, y + r * 0.55 + f, r * 0.32, r * 0.22, 0, 0, 7); ctx.fill();
  ctx.beginPath(); ctx.ellipse(x + r * 0.45, y + r * 0.55 - f, r * 0.32, r * 0.22, 0, 0, 7); ctx.fill();
  // body
  ctx.fillStyle = c.slow ? mix(k.body, "#7fd4ff", 0.45) : k.body;
  ctx.beginPath(); ctx.ellipse(x, y, r, r * 1.05, 0, 0, 7); ctx.fill();
  ctx.lineWidth = 2; ctx.strokeStyle = k.dark; ctx.stroke();
  if (c.kind === 2) { ctx.fillStyle = "#9aa1ae"; ctx.fillRect(x - r * 0.7, y - r * 0.9, r * 1.4, r * 0.45); } // tank helmet
  // eyes look toward travel
  const ang = c.fa, ex = Math.cos(ang) * r * 0.28, ey = Math.sin(ang) * r * 0.28;
  for (const sgn of [-1, 1]) {
    const bx = x + sgn * r * 0.36, by = y - r * 0.18;
    ctx.fillStyle = "#fff"; ctx.beginPath(); ctx.arc(bx, by, r * 0.22, 0, 7); ctx.fill();
    ctx.fillStyle = "#15202b"; ctx.beginPath(); ctx.arc(bx + ex * 0.5, by + ey * 0.5, r * 0.11, 0, 7); ctx.fill();
  }
  // hp bar
  const w = r * 2.1, fr = Math.max(0, c.hp / 10);
  ctx.fillStyle = "rgba(0,0,0,0.55)"; ctx.fillRect(x - w / 2, y - r - 9, w, 4.5);
  ctx.fillStyle = fr > 0.5 ? "#54d36a" : (fr > 0.25 ? "#ffd24a" : "#ff5d5d"); ctx.fillRect(x - w / 2, y - r - 9, w * fr, 4.5);
}
function drawTower(tw, t) {
  const x = (tw.c + 0.5) * TILE, y = (tw.r + 0.5) * TILE;
  // stone base
  ctx.fillStyle = "rgba(0,0,0,0.28)"; ctx.beginPath(); ctx.ellipse(x, y + 5, TILE * 0.42, TILE * 0.22, 0, 0, 7); ctx.fill();
  ctx.fillStyle = "#5a6072"; ctx.beginPath(); ctx.arc(x, y, TILE * 0.4, 0, 7); ctx.fill();
  ctx.fillStyle = "#737a8c"; ctx.beginPath(); ctx.arc(x, y - 2, TILE * 0.32, 0, 7); ctx.fill();
  ctx.save(); ctx.translate(x, y - 2); ctx.rotate(tw.aim);
  const rec = -tw.recoil * 4;
  if (tw.kind === 0) { // arrow / ballista
    ctx.strokeStyle = "#caa46a"; ctx.lineWidth = 3; ctx.beginPath(); ctx.moveTo(6 + rec, -9); ctx.lineTo(6 + rec, 9); ctx.stroke();
    ctx.strokeStyle = "#e9d9a8"; ctx.lineWidth = 2; ctx.beginPath(); ctx.moveTo(-6 + rec, 0); ctx.lineTo(14 + rec, 0); ctx.stroke();
    ctx.fillStyle = "#6fb0ff"; ctx.beginPath(); ctx.arc(rec - 6, 0, 4, 0, 7); ctx.fill();
  } else if (tw.kind === 1) { // cannon
    ctx.fillStyle = "#2c3140"; ctx.fillRect(rec - 4, -6, 20, 12);
    ctx.fillStyle = "#171b24"; ctx.beginPath(); ctx.arc(16 + rec, 0, 6.5, 0, 7); ctx.fill();
    ctx.fillStyle = "#9a6bff"; ctx.beginPath(); ctx.arc(rec - 6, 0, 5, 0, 7); ctx.fill();
  } else { // frost crystal
    const p = 1 + Math.sin(t * 4 + tw.c) * 0.12; ctx.fillStyle = "#49d6ff"; ctx.beginPath();
    ctx.moveTo(11 * p, 0); ctx.lineTo(0, 8 * p); ctx.lineTo(-9 * p, 0); ctx.lineTo(0, -8 * p); ctx.closePath(); ctx.fill();
    ctx.fillStyle = "rgba(255,255,255,0.7)"; ctx.beginPath(); ctx.arc(0, -2, 2.5, 0, 7); ctx.fill();
  }
  if (tw.flash > 0) { ctx.fillStyle = `rgba(255,235,150,${tw.flash})`; ctx.beginPath(); ctx.arc(16 + rec, 0, 7 * tw.flash + 3, 0, 7); ctx.fill(); }
  ctx.restore();
}
function drawProj(p) {
  if (p.kind === 0) { // arrow streak toward nearest creep
    let a = nearestAngle(p.x, p.y);
    ctx.save(); ctx.translate(p.x, p.y); ctx.rotate(a);
    ctx.strokeStyle = "#ffe9a8"; ctx.lineWidth = 2.5; ctx.beginPath(); ctx.moveTo(-7, 0); ctx.lineTo(6, 0); ctx.stroke();
    ctx.fillStyle = "#fff2c0"; ctx.beginPath(); ctx.moveTo(9, 0); ctx.lineTo(4, -3); ctx.lineTo(4, 3); ctx.closePath(); ctx.fill(); ctx.restore();
  } else if (p.kind === 1) { ctx.fillStyle = "#0c0f16"; ctx.beginPath(); ctx.arc(p.x, p.y, 6, 0, 7); ctx.fill(); ctx.fillStyle = "rgba(255,255,255,0.4)"; ctx.beginPath(); ctx.arc(p.x - 2, p.y - 2, 2, 0, 7); ctx.fill(); }
  else { ctx.fillStyle = "#aef0ff"; ctx.save(); ctx.translate(p.x, p.y); ctx.rotate(p.x * 0.1); ctx.beginPath(); ctx.moveTo(5, 0); ctx.lineTo(0, 4); ctx.lineTo(-5, 0); ctx.lineTo(0, -4); ctx.closePath(); ctx.fill(); ctx.restore(); }
}
function nearestAngle(x, y) { let bd = 1e9, a = 0; for (const c of creeps.values()) { const d = (c.dx - x) ** 2 + (c.dy - y) ** 2; if (d < bd) { bd = d; a = Math.atan2(c.dy - y, c.dx - x); } } return a; }
function mix(a, b, t) { const pa = hx(a), pb = hx(b); return `rgb(${pa.map((v, i) => Math.round(v + (pb[i] - v) * t)).join(",")})`; }
function hx(h) { return [1, 3, 5].map(i => parseInt(h.slice(i, i + 2), 16)); }

// ---- main loop ------------------------------------------------------------
let last = performance.now();
function loop(now) {
  requestAnimationFrame(loop);
  const dt = Math.min(0.05, (now - last) / 1000); last = now; const t = now / 1000;
  if (!cv.width) return;
  // smooth creep motion + facing
  const k = Math.min(1, dt * 14);
  for (const c of creeps.values()) {
    const ox = c.dx, oy = c.dy;
    c.dx += (c.tx - c.dx) * k; c.dy += (c.ty - c.dy) * k;
    const vx = c.dx - ox, vy = c.dy - oy; if (vx * vx + vy * vy > 0.02) c.fa = Math.atan2(vy, vx);
  }
  // turret aim + fire detection (a projectile sitting on a tower => it just fired)
  for (const tw of towers) {
    const cx = (tw.c + 0.5) * TILE, cy = (tw.r + 0.5) * TILE; let bd = RANGE[tw.kind] ** 2, tgt = null;
    for (const c of creeps.values()) { const d = (c.dx - cx) ** 2 + (c.dy - cy) ** 2; if (d < bd) { bd = d; tgt = c; } }
    if (tgt) { const want = Math.atan2(tgt.dy - cy, tgt.dx - cx); let da = want - tw.aim; while (da > Math.PI) da -= 6.283; while (da < -Math.PI) da += 6.283; tw.aim += da * Math.min(1, dt * 12); }
    if (projs.some(p => (p.x - cx) ** 2 + (p.y - cy) ** 2 < 200)) { tw.recoil = 1; tw.flash = 1; }
    tw.recoil = Math.max(0, tw.recoil - dt * 5); tw.flash = Math.max(0, tw.flash - dt * 6);
  }

  // grass
  for (let r = 0; r < ROWS; r++) for (let c = 0; c < COLS; c++) { ctx.fillStyle = (c + r) % 2 ? "#2c6b34" : "#327239"; ctx.fillRect(c * TILE, r * TILE, TILE, TILE); }
  drawRoad();
  drawPortal(t);
  drawDecor();
  // hover build preview
  if (hover && state < 2) {
    const [c, r] = hover, key = c + "," + r;
    const ok = !road.has(key) && !towers.some(t2 => t2.c === c && t2.r === r) && gold >= COST[pick] && c >= 0 && c < COLS && r >= 0 && r < ROWS;
    ctx.fillStyle = ok ? "rgba(110,230,140,0.28)" : "rgba(255,90,90,0.24)"; ctx.fillRect(c * TILE, r * TILE, TILE, TILE);
    ctx.strokeStyle = ok ? "rgba(170,255,190,0.7)" : "rgba(255,120,120,0.7)"; ctx.lineWidth = 2; ctx.strokeRect(c * TILE + 1, r * TILE + 1, TILE - 2, TILE - 2);
    ctx.beginPath(); ctx.arc(c * TILE + TILE / 2, r * TILE + TILE / 2, RANGE[pick], 0, 7); ctx.strokeStyle = "rgba(255,255,255,0.16)"; ctx.lineWidth = 1.5; ctx.stroke();
  }
  drawKeep(t);
  for (const tw of towers) drawTower(tw, t);
  for (const p of projs) drawProj(p);
  // creeps sorted by y for depth
  [...creeps.values()].sort((a, b) => a.dy - b.dy).forEach(c => drawCreep(c, t));
}
requestAnimationFrame(loop);

// ---- input ----------------------------------------------------------------
function tileAt(ev) { const r = cv.getBoundingClientRect(); const sx = cv.width / r.width, sy = cv.height / r.height; return [Math.floor((ev.clientX - r.left) * sx / TILE), Math.floor((ev.clientY - r.top) * sy / TILE)]; }
cv.addEventListener("mousemove", (ev) => hover = tileAt(ev));
cv.addEventListener("mouseleave", () => hover = null);
cv.addEventListener("click", (ev) => { const [c, r] = tileAt(ev); ws && ws.readyState === 1 && ws.send(`place ${pick} ${c} ${r}`); });
document.querySelectorAll(".tw").forEach(el => el.onclick = () => { pick = +el.dataset.k; document.querySelectorAll(".tw").forEach(x => x.classList.toggle("sel", x === el)); });
$("wave").onclick = () => ws && ws.readyState === 1 && ws.send("wave");
$("again").onclick = () => ws && ws.readyState === 1 && ws.send("reset");
addEventListener("keydown", (e) => {
  if (e.key >= "1" && e.key <= "3") { pick = +e.key - 1; document.querySelectorAll(".tw").forEach(x => x.classList.toggle("sel", +x.dataset.k === pick)); }
  if (e.key === " ") { e.preventDefault(); ws && ws.readyState === 1 && ws.send("wave"); }
});

// ---- multiplayer (rooms) --------------------------------------------------
function renderMp() {
  const el = $("mp"); if (!el) return;
  if (ROOM) {
    el.innerHTML = `👥 <b style="color:#e9eef8">Co-op room ${ROOM}</b><br>` +
      `<span style="color:#3ec46a">${players} player${players === 1 ? "" : "s"} here</span> · ` +
      `<a href="#" id="copylink" style="color:#5b9dff">copy invite link</a>`;
    const cl = $("copylink"); if (cl) cl.onclick = (e) => { e.preventDefault(); navigator.clipboard && navigator.clipboard.writeText(location.href); cl.textContent = "link copied!"; };
  } else {
    el.innerHTML = `Playing solo. <button id="mkroom" style="margin-top:6px;font:inherit;font-weight:700;padding:7px 10px;border:1px solid #2a3550;border-radius:8px;background:#16203a;color:#cfe0ff;cursor:pointer">👥 Play with a friend</button>`;
    const mk = $("mkroom"); if (mk) mk.onclick = () => { const code = Math.random().toString(36).slice(2, 8); location.search = "?room=" + code; };
  }
}
renderMp();

connect();
