// opcusdb Boomborn — browser client. Thin client: renders the world the Rust
// server simulates (PixiJS) and sends movement input. Bomberman vs. vampires.

const $ = (id) => document.getElementById(id);
const VIEW_W = 960, VIEW_H = 600;
const PCOL = [0xffffff, 0x7dcfff, 0xffd24a, 0x6ee7b7, 0xff7eb6, 0xc4a7ff, 0xff9f5a, 0x4de1e6];

let ws = null, myId = 0, started = false;
let arena = 3000, time = 0;
let players = new Map();   // id -> {x,y,dispX,dispY,...}
let enemies = new Map();   // id -> {x,y,dispX,dispY,kind,hp}
let bombs = [], booms = [], gems = [], lb = [];
let particles = [], shake = 0, camX = 0, camY = 0;

const app = new PIXI.Application({ width: VIEW_W, height: VIEW_H, background: 0x0a0c16, antialias: false });
$("stageWrap").appendChild(app.view);
const world = new PIXI.Container();
app.stage.addChild(world);
const gGround = new PIXI.Graphics();
const gGems = new PIXI.Graphics();
const gBooms = new PIXI.Graphics();
const gEnemies = new PIXI.Graphics();
const gBombs = new PIXI.Graphics();
const gPlayers = new PIXI.Graphics();
const gParts = new PIXI.Graphics();
world.addChild(gGround, gGems, gBooms, gEnemies, gBombs, gPlayers, gParts);

let groundBuilt = false;
const tombs = [];
function buildGround() {
  gGround.clear();
  gGround.beginFill(0x0c1018).drawRect(0, 0, arena, arena).endFill();
  // grid
  gGround.lineStyle(1, 0x161d2c, 0.9);
  for (let i = 0; i <= arena; i += 90) { gGround.moveTo(i, 0).lineTo(i, arena); gGround.moveTo(0, i).lineTo(arena, i); }
  gGround.lineStyle(0);
  // border wall glow
  gGround.lineStyle(6, 0x3a1f2a, 1); gGround.drawRect(0, 0, arena, arena); gGround.lineStyle(0);
  // scattered graves / decals (deterministic-ish positions)
  for (let i = 0; i < 150; i++) {
    const x = (i * 211 + 60) % (arena - 80) + 40, y = (i * 367 + 90) % (arena - 80) + 40;
    tombs.push([x, y]);
    gGround.beginFill(0x12161f).drawRoundedRect(x - 9, y - 14, 18, 20, 4).endFill();
    gGround.beginFill(0x1a212e).drawRect(x - 2, y - 9, 4, 5).endFill();
    gGround.beginFill(0x0a0d14, 0.6).drawEllipse(x, y + 8, 12, 4).endFill();
  }
  groundBuilt = true;
}

// ---- networking -----------------------------------------------------------
function connect(nick) {
  ws = new WebSocket(`ws://${location.host}/ws`);
  ws.onopen = () => ws.send(`join ${nick}`);
  ws.onmessage = (e) => {
    const pSeen = new Set(), eSeen = new Set();
    for (const line of e.data.split("\n")) {
      if (!line) continue;
      const c = line.indexOf("\t");
      const tag = line.slice(0, c), rest = line.slice(c + 1);
      if (tag === "w") { myId = +rest; }
      else if (tag === "a") { const p = rest.split("\t"); arena = +p[0]; time = +p[2]; if (!groundBuilt) buildGround(); }
      else if (tag === "p") {
        const p = rest.split("\t"); const id = +p[0];
        const cur = players.get(id) || { dispX: +p[1], dispY: +p[2] };
        Object.assign(cur, { x: +p[1], y: +p[2], hp: +p[3], maxhp: +p[4], level: +p[5], xp: +p[6], xpneed: +p[7], facing: +p[8], kills: +p[9], down: +p[10], name: p[11], opts: p[12] || "-" });
        players.set(id, cur); pSeen.add(id);
      } else if (tag === "e") {
        for (const s of rest.split(";")) { if (!s) continue; const [id, x, y, k, h] = s.split(",").map(Number);
          const cur = enemies.get(id) || { dispX: x, dispY: y }; cur.x = x; cur.y = y; cur.kind = k; cur.hp = h; enemies.set(id, cur); eSeen.add(id); }
      } else if (tag === "j") {
        bombs = rest.split(";").filter(Boolean).map(s => { const [x, y, k] = s.split(",").map(Number); return { x, y, k }; });
      } else if (tag === "o") {
        booms = rest.split(";").filter(Boolean).map(s => { const [x, y, r] = s.split(",").map(Number); return { x, y, r }; });
      } else if (tag === "m") {
        gems = rest.split(";").filter(Boolean).map(s => { const [x, y] = s.split(",").map(Number); return { x, y }; });
      } else if (tag === "x") {
        for (const ev of rest.split(";")) { if (!ev) continue; const [t, x, y] = ev.split(":"); onEvent(t, +x, +y); }
      } else if (tag === "l") {
        lb = rest.split(",").filter(Boolean).map(s => { const [n, k] = s.split(":"); return [n, +k]; });
        renderLB();
      }
    }
    if (pSeen.size || players.size) for (const id of [...players.keys()]) if (!pSeen.has(id)) players.delete(id);
    for (const id of [...enemies.keys()]) if (!eSeen.has(id)) enemies.delete(id);
    updateHUD();
    updateBoss();
    updateOverlays();
  };
}

// ---- effects --------------------------------------------------------------
let sfxBudget = 3;
function onEvent(t, x, y) {
  if (t === "x" || t === "e") { fire(x, y, 26); shake = Math.min(shake + 8, 24); if (sfxBudget-- > 0) sfxBoom(); }
  else if (t === "k") { fire(x, y, 5, 0x9b3b3b); }
  else if (t === "t") { if (sfxBudget-- > 0) sfxThrow(); }
  else if (t === "h") { fire(x, y, 8, 0xff4d6a); shake = Math.min(shake + 6, 20); sfxHurt(); }
  else if (t === "d") { fire(x, y, 40, 0xff7eb6); shake = 26; sfxDown(); }
  else if (t === "l") { fire(x, y, 26, 0xffe066); sfxLevel(); }
}
function fire(x, y, n, col) {
  for (let i = 0; i < n; i++) { const a = Math.random() * 7, s = 1 + Math.random() * 5;
    particles.push({ x, y, vx: Math.cos(a) * s, vy: Math.sin(a) * s - 1, life: 1, decay: 0.02 + Math.random() * 0.04,
      col: col ?? [0xffffff, 0xffd24a, 0xff7a2d, 0xff3b2d][Math.random() * 4 | 0], size: 2 + (Math.random() * 4 | 0) }); }
  if (particles.length > 700) particles.splice(0, particles.length - 700);
}

let actx = null, muted = false;
function ac() { if (!actx) actx = new (window.AudioContext || window.webkitAudioContext)(); return actx; }
function tone(f0, f1, d, type, v) { if (muted) return; const c = ac(), o = c.createOscillator(), g = c.createGain();
  o.type = type; o.frequency.setValueAtTime(f0, c.currentTime); o.frequency.exponentialRampToValueAtTime(Math.max(1, f1), c.currentTime + d);
  g.gain.setValueAtTime(v, c.currentTime); g.gain.exponentialRampToValueAtTime(0.001, c.currentTime + d); o.connect(g).connect(c.destination); o.start(); o.stop(c.currentTime + d); }
function noise(d, v, hp) { if (muted) return; const c = ac(), n = c.sampleRate * d | 0, b = c.createBuffer(1, n, c.sampleRate), dt = b.getChannelData(0);
  for (let i = 0; i < n; i++) dt[i] = (Math.random() * 2 - 1) * (1 - i / n); const s = c.createBufferSource(); s.buffer = b;
  const g = c.createGain(); g.gain.value = v; const f = c.createBiquadFilter(); f.type = "highpass"; f.frequency.value = hp; s.connect(f).connect(g).connect(c.destination); s.start(); }
const sfxBoom = () => { noise(0.3, 0.22, 250); tone(150, 50, 0.32, "sawtooth", 0.18); };
const sfxThrow = () => tone(500, 900, 0.07, "square", 0.05);
const sfxHurt = () => noise(0.16, 0.2, 700);
const sfxDown = () => { noise(0.5, 0.25, 200); tone(400, 70, 0.5, "sawtooth", 0.2); };
const sfxLevel = () => { [523, 659, 784, 1047].forEach((f, i) => setTimeout(() => tone(f, f, 0.12, "triangle", 0.12), i * 70)); };

// ---- drawing --------------------------------------------------------------
function R(g, col, cx, cy, x, y, w, h, f, a = 1) { const rx = f > 0 ? x : -x - w; g.beginFill(col, a).drawRect(cx + rx, cy + y, w, h).endFill(); }

function drawPlayer(g, p) {
  const cx = p.dispX, cy = p.dispY, f = p.facing || 1, col = PCOL[(p.id - 1) % PCOL.length];
  g.beginFill(0x000000, 0.3).drawEllipse(cx, cy + 16, 16, 6).endFill();
  if (p.down) { g.beginFill(0x555, 0.5).drawCircle(cx, cy, 14).endFill(); return; }
  const flash = p.iframeFlash;
  const body = flash ? 0xffffff : col;
  // legs
  R(g, 0x2a2f3a, cx, cy, -7, 8, 6, 8, f); R(g, 0x2a2f3a, cx, cy, 1, 8, 6, 8, f);
  // body (round bomber suit)
  g.beginFill(body).drawRoundedRect(cx - 12, cy - 12, 24, 22, 8).endFill();
  g.beginFill(0x2a2f3a).drawRect(cx - 12, cy + 2, 24, 4).endFill(); // belt
  // head/helmet
  g.beginFill(body).drawCircle(cx, cy - 16, 10).endFill();
  g.beginFill(0xbfe0ff).drawRoundedRect(cx - 7, cy - 19, 14, 8, 3).endFill(); // visor
  g.beginFill(0x101018).drawRect(cx + f * 1, cy - 18, 4, 4).endFill();
  // antenna + light
  g.lineStyle(2, 0x9aa5b5).moveTo(cx, cy - 26).lineTo(cx, cy - 32); g.lineStyle(0);
  g.beginFill(0xff4d4d).drawCircle(cx, cy - 33, 2.5).endFill();
  // little held bomb
  g.beginFill(0x14161e).drawCircle(cx + f * 14, cy + 2, 5).endFill();
  g.beginFill(0xff8a3d).drawCircle(cx + f * 14, cy - 4, 1.6).endFill();
}

function drawEnemy(g, e) {
  const x = e.dispX, y = e.dispY;
  g.beginFill(0x000000, 0.28).drawEllipse(x, y + 8, 10, 4).endFill();
  const now = performance.now();
  if (e.kind === 0) { // bat
    const flap = Math.sin(now / 90 + x) * 4;
    g.beginFill(0x2a1430).drawEllipse(x, y, 7, 6).endFill();
    g.beginFill(0x3a1d44).drawPolygon([x - 6, y, x - 16, y - 6 - flap, x - 13, y + 4]).endFill();
    g.beginFill(0x3a1d44).drawPolygon([x + 6, y, x + 16, y - 6 - flap, x + 13, y + 4]).endFill();
    g.beginFill(0xff3b3b).drawRect(x - 3, y - 2, 2, 2).drawRect(x + 1, y - 2, 2, 2).endFill();
  } else if (e.kind === 1) { // ghoul
    g.beginFill(0x355e34).drawRoundedRect(x - 10, y - 14, 20, 24, 5).endFill();
    g.beginFill(0x2a4a2a).drawRect(x - 10, y + 2, 20, 8).endFill();
    g.beginFill(0xe8f57a).drawRect(x - 6, y - 9, 3, 3).drawRect(x + 3, y - 9, 3, 3).endFill();
    g.beginFill(0x101810).drawRect(x - 4, y - 2, 8, 2).endFill();
  } else if (e.kind === 2) { // vampire
    g.beginFill(0x2a1030).drawPolygon([x - 11, y + 10, x, y - 16, x + 11, y + 10]).endFill(); // cape
    g.beginFill(0xd9c3a3).drawCircle(x, y - 12, 6).endFill();
    g.beginFill(0x111).drawRect(x - 4, y - 13, 2, 2).drawRect(x + 2, y - 13, 2, 2).endFill();
    g.beginFill(0xff3b6b).drawRect(x - 2, y - 9, 1, 2).drawRect(x + 1, y - 9, 1, 2).endFill(); // fangs
  } else if (e.kind === 3) { // bat-lord (elite)
    const flap = Math.sin(now / 70 + x) * 7;
    g.beginFill(0x4a1230).drawEllipse(x, y, 13, 11).endFill();
    g.beginFill(0x6a1d44).drawPolygon([x - 10, y, x - 28, y - 10 - flap, x - 22, y + 8]).endFill();
    g.beginFill(0x6a1d44).drawPolygon([x + 10, y, x + 28, y - 10 - flap, x + 22, y + 8]).endFill();
    g.beginFill(0xffd24a).drawCircle(x, y - 16, 3).endFill(); // crown gem
    g.beginFill(0xff2a2a).drawRect(x - 5, y - 3, 3, 3).drawRect(x + 2, y - 3, 3, 3).endFill();
  } else { // VAMPIRE LORD (boss)
    const flap = Math.sin(now / 60 + x) * 12;
    g.beginFill(0x12000a, 0.5).drawEllipse(x, y + 16, 30, 8).endFill();
    g.beginFill(0x6a1030).drawPolygon([x - 18, y, x - 56, y - 18 - flap, x - 44, y + 18]).endFill();
    g.beginFill(0x6a1030).drawPolygon([x + 18, y, x + 56, y - 18 - flap, x + 44, y + 18]).endFill();
    g.beginFill(0x2a0a1a).drawRoundedRect(x - 20, y - 22, 40, 44, 12).endFill(); // cape body
    g.beginFill(0xd9c3a3).drawCircle(x, y - 18, 11).endFill(); // pale face
    g.beginFill(0xff2a2a).drawRect(x - 7, y - 20, 4, 4).drawRect(x + 3, y - 20, 4, 4).endFill(); // eyes
    g.beginFill(0xffffff).drawRect(x - 4, y - 12, 2, 4).drawRect(x + 2, y - 12, 2, 4).endFill(); // fangs
    // crown
    g.beginFill(0xffd24a).drawPolygon([x - 11, y - 27, x - 7, y - 34, x - 3, y - 27, x, y - 35, x + 3, y - 27, x + 7, y - 34, x + 11, y - 27]).endFill();
  }
  // hp pip when hurt
  if (e.hp < 9) { const w = 22, fr = e.hp / 9; g.beginFill(0x000, 0.5).drawRect(x - w / 2, y - 22, w, 3).endFill();
    g.beginFill(0x6ee27a).drawRect(x - w / 2, y - 22, w * fr, 3).endFill(); }
}

// ---- main loop ------------------------------------------------------------
app.ticker.add(() => {
  for (const p of players.values()) { p.dispX += (p.x - p.dispX) * 0.4; p.dispY += (p.y - p.dispY) * 0.4; }
  for (const e of enemies.values()) { e.dispX += (e.x - e.dispX) * 0.4; e.dispY += (e.y - e.dispY) * 0.4; }

  const me = players.get(myId);
  const tx = me ? me.dispX : arena / 2, ty = me ? me.dispY : arena / 2;
  camX += ((clamp(tx, VIEW_W / 2, arena - VIEW_W / 2) - VIEW_W / 2) - camX) * 0.15;
  camY += ((clamp(ty, VIEW_H / 2, arena - VIEW_H / 2) - VIEW_H / 2) - camY) * 0.15;
  const sx = shake > 0.3 ? (Math.random() - 0.5) * shake : 0, sy = shake > 0.3 ? (Math.random() - 0.5) * shake : 0;
  shake *= 0.85;
  world.x = -camX + sx; world.y = -camY + sy;

  // gems
  gGems.clear();
  const gp = 0.6 + 0.4 * Math.sin(performance.now() / 150);
  for (const g of gems) { gGems.beginFill(0x14e0ff, gp).drawPolygon([g.x, g.y - 5, g.x + 4, g.y, g.x, g.y + 5, g.x - 4, g.y]).endFill();
    gGems.beginFill(0xffffff, gp).drawRect(g.x - 1, g.y - 1, 2, 2).endFill(); }

  // explosions (under enemies)
  gBooms.clear();
  for (const b of booms) {
    gBooms.beginFill(0xff3b2d, 0.35).drawCircle(b.x, b.y, b.r).endFill();
    gBooms.beginFill(0xff8a3d, 0.5).drawCircle(b.x, b.y, b.r * 0.7).endFill();
    gBooms.beginFill(0xffe066, 0.85).drawCircle(b.x, b.y, b.r * 0.4).endFill();
    gBooms.beginFill(0xffffff, 0.9).drawCircle(b.x, b.y, b.r * 0.16).endFill();
  }

  // enemies
  gEnemies.clear();
  for (const e of enemies.values()) drawEnemy(gEnemies, e);

  // bombs / rockets
  gBombs.clear();
  const spark = (performance.now() / 80 | 0) % 2;
  for (const b of bombs) {
    if (b.k === 0) { gBombs.beginFill(0x14161e).drawCircle(b.x, b.y, 6).endFill();
      gBombs.lineStyle(2, 0x8a5a2a).moveTo(b.x, b.y - 6).lineTo(b.x + 3, b.y - 11); gBombs.lineStyle(0);
      if (spark) gBombs.beginFill(0xffd24a).drawCircle(b.x + 3, b.y - 12, 2).endFill(); }
    else { gBombs.beginFill(0xcfd6e0).drawRect(b.x - 3, b.y - 6, 6, 12).endFill();
      gBombs.beginFill(0xff8a3d).drawPolygon([b.x - 3, b.y + 6, b.x + 3, b.y + 6, b.x, b.y + 12]).endFill(); }
  }

  // players
  gPlayers.clear();
  for (const p of players.values()) drawPlayer(gPlayers, p);

  // particles
  gParts.clear();
  for (const pt of particles) { pt.vy += 0.12; pt.x += pt.vx; pt.y += pt.vy; pt.life -= pt.decay;
    gParts.beginFill(pt.col, Math.max(0, pt.life)).drawRect(pt.x, pt.y, pt.size, pt.size).endFill(); }
  particles = particles.filter(p => p.life > 0);

  // player name tags (PIXI text) — keep crisp
  syncTags();
  sfxBudget = 3;
});

const tags = new Map();
function syncTags() {
  for (const [id, p] of players) {
    let t = tags.get(id);
    if (!t) { t = new PIXI.Text("", { fontFamily: "monospace", fontSize: 11, fontWeight: "700", fill: 0xffffff, stroke: 0x000, strokeThickness: 3 });
      t.anchor.set(0.5, 1); world.addChild(t); tags.set(id, t); }
    t.text = p.name + (id === myId ? " ★" : ""); t.x = p.dispX; t.y = p.dispY - 36; t.visible = !p.down;
  }
  for (const [id, t] of tags) if (!players.has(id)) { t.destroy(); tags.delete(id); }
}

// ---- HUD ------------------------------------------------------------------
function updateHUD() {
  $("timer").textContent = `${(time / 60 | 0)}:${String(time % 60 | 0).padStart(2, "0")}`;
  $("wave").textContent = `${enemies.size} vampires on the field`;
  const me = players.get(myId);
  if (me) { $("lvl").textContent = "Lv " + me.level; $("xpfill").style.width = (100 * me.xp / Math.max(1, me.xpneed)) + "%"; $("kills").textContent = "☠ " + me.kills; }
  const box = $("players"); box.innerHTML = "";
  for (const [id, p] of [...players].sort((a, b) => b[1].kills - a[1].kills)) {
    const col = "#" + PCOL[(id - 1) % PCOL.length].toString(16).padStart(6, "0");
    const hpf = Math.max(0, 100 * p.hp / p.maxhp);
    const d = document.createElement("div"); d.className = "pc";
    d.innerHTML = `<div class="nm"><span><i class="pdot" style="background:${col}"></i>${esc(p.name)}${id===myId?" ★":""}</span><span class="meta">Lv${p.level} · ☠${p.kills}</span></div>
      <div class="bar"><i style="width:${hpf}%;background:${hpf<30?'#ff4d4d':hpf<60?'#ffd24a':'#6ee27a'}"></i></div>`;
    box.appendChild(d);
  }
}
function updateBoss() {
  let boss = null;
  for (const e of enemies.values()) if (e.kind === 4) { boss = e; break; }
  const bar = $("bossbar");
  if (boss) { bar.style.display = "block"; $("bossfill").style.width = (boss.hp / 9 * 100) + "%"; }
  else bar.style.display = "none";
}

let lastOpts = "";
function updateOverlays() {
  const me = players.get(myId);
  // game over
  const go = $("gameover");
  if (me && me.down) {
    if (go.style.display !== "flex") {
      go.style.display = "flex";
      $("goTime").textContent = `${(time / 60 | 0)}:${String(time % 60 | 0).padStart(2, "0")}`;
      $("goKills").textContent = me.kills; $("goLevel").textContent = "Lv " + me.level;
    }
  } else { go.style.display = "none"; }
  // level-up upgrade cards
  const up = $("upgrade");
  const opts = me && !me.down ? (me.opts || "-") : "-";
  if (opts !== "-" && opts) {
    if (opts !== lastOpts) {
      lastOpts = opts;
      const cards = opts.split("~").map(s => { const i = s.indexOf("|"); return [s.slice(0, i), s.slice(i + 1)]; });
      const box = $("ucards"); box.innerHTML = "";
      cards.forEach(([id, label], i) => {
        const d = document.createElement("div"); d.className = "ucard";
        d.innerHTML = `${esc(label)}<span class="key">press ${i + 1}</span>`;
        d.onclick = () => pick(i);
        box.appendChild(d);
      });
    }
    up.style.display = "flex";
  } else { up.style.display = "none"; lastOpts = ""; }
}
function pick(i) { if (ws && ws.readyState === 1) ws.send("pick " + i); lastOpts = ""; }

function renderLB() { const el = $("lbrows"); el.innerHTML = lb.length ? "" : '<div style="color:#6b7a99">no scores yet</div>';
  lb.slice(0, 6).forEach(([n, k], i) => { const d = document.createElement("div"); d.className = "row"; d.innerHTML = `<span>${i + 1}. ${esc(n)}</span><span>${k}</span>`; el.appendChild(d); }); }
const esc = (s) => (s || "").replace(/[&<>]/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" }[c]));
const clamp = (v, a, b) => Math.max(a, Math.min(b, v));

// ---- input ----------------------------------------------------------------
const keys = { l: false, r: false, u: false, d: false };
const MAP = { ArrowLeft: "l", a: "l", A: "l", ArrowRight: "r", d: "r", D: "r", ArrowUp: "u", w: "u", W: "u", ArrowDown: "d", s: "d", S: "d" };
function send() { if (ws && ws.readyState === 1) ws.send(`keys ${+keys.l} ${+keys.r} ${+keys.u} ${+keys.d}`); }
addEventListener("keydown", (e) => {
  if (started && (e.key === "1" || e.key === "2" || e.key === "3")) {
    const me = players.get(myId);
    if (me && me.opts && me.opts !== "-" && !me.down) { pick(+e.key - 1); e.preventDefault(); return; }
  }
  const k = MAP[e.key]; if (k && started) { if (!keys[k]) { keys[k] = true; send(); } e.preventDefault(); }
});
addEventListener("keyup", (e) => { const k = MAP[e.key]; if (k) { keys[k] = false; send(); e.preventDefault(); } });

// ---- start ----------------------------------------------------------------
$("sound").onclick = () => { muted = !muted; $("sound").textContent = muted ? "🔇 sound off" : "🔊 sound on"; if (!muted) ac().resume(); };
function start() { started = true; $("overlay").style.display = "none"; ac();
  connect($("nick").value.trim() || ("P" + (Math.random() * 900 + 100 | 0))); }
$("start").onclick = start;
$("nick").addEventListener("keydown", (e) => { if (e.key === "Enter") start(); });
$("again").onclick = () => { if (ws && ws.readyState === 1) ws.send("respawn"); $("gameover").style.display = "none"; };
