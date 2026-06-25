// opcusdb Hearth — PixiJS client for the AI town. Renders crafted, animated
// residents on a (optionally Qwen-Image-generated) map; you walk in and talk to
// whoever is nearby. Server is authoritative; this interpolates + animates.

const W = 960, H = 608;
const $ = (id) => document.getElementById(id);
let ws = null, myId = 0, started = false;
const chars = new Map();   // id -> view state
let roster = [];           // [{id,name,kind,act}]
let phase = 0.25;
let LOCS = [];

// 12 resident palettes [shirt, hair, skin]; 99 = the human visitor
const PAL = [
  [0xe2574c, 0x3a2418, 0xf0c9a0], [0x4a6fb0, 0x111111, 0xe6b98e], [0x4caf7d, 0x6b4a2a, 0xf0c9a0],
  [0xc24d8a, 0x8a5a2a, 0xe6b98e], [0x6c63b5, 0x2a2a2a, 0xf0c9a0], [0xd98a2b, 0x4a2f18, 0xe6b98e],
  [0xb04ad9, 0x1a1a1a, 0xf0c9a0], [0x5a7d8c, 0x9a9a9a, 0xdcb38a], [0x3fa7a0, 0x5a3a1a, 0xf0c9a0],
  [0xe8b54a, 0xc25a2a, 0xf2d0aa], [0x8a8f5a, 0x2a2a2a, 0xe6b98e], [0x4a90e2, 0x6b4a2a, 0xf0c9a0],
];
const humanPal = [0xffd24a, 0x3a2418, 0xf0c9a0];

const app = new PIXI.Application({ width: W, height: H, backgroundColor: 0x223021, antialias: true });
$("app").appendChild(app.view);
function fit() { const s = Math.min(innerWidth / W, innerHeight / H) * 0.98; app.view.style.width = W * s + "px"; app.view.style.height = H * s + "px"; }
addEventListener("resize", fit); fit();

const bgL = new PIXI.Container(), groundL = new PIXI.Container(), labelL = new PIXI.Container(), charL = new PIXI.Container(), bubbleL = new PIXI.Container(), nightL = new PIXI.Container();
app.stage.addChild(bgL, groundL, labelL, charL, bubbleL, nightL);
let hasBg = false;
const night = new PIXI.Graphics().beginFill(0xffffff).drawRect(0, 0, W, H).endFill();
nightL.addChild(night); nightL.eventMode = "none";

// ---- map / scenery --------------------------------------------------------
function buildScenery() {
  groundL.removeChildren(); labelL.removeChildren();
  const g = new PIXI.Graphics();
  // grass with a soft checker
  for (let y = 0; y < 19; y++) for (let x = 0; x < 30; x++) { g.beginFill((x + y) % 2 ? 0x33502c : 0x39572f).drawRect(x * 32, y * 32, 32, 32).endFill(); }
  groundL.addChild(g);
  // paths from plaza to each location
  const plaza = LOCS.find(l => l.kind === "plaza");
  if (plaza) { const pc = [(plaza.x + plaza.w / 2) * 32, (plaza.y + plaza.h / 2) * 32];
    const pg = new PIXI.Graphics(); pg.lineStyle(14, 0x8a7048, 1);
    for (const l of LOCS) { if (l === plaza) continue; pg.moveTo(pc[0], pc[1]); pg.lineTo((l.x + l.w / 2) * 32, (l.y + l.h / 2) * 32); }
    pg.lineStyle(8, 0xa3865a, 1);
    for (const l of LOCS) { if (l === plaza) continue; pg.moveTo(pc[0], pc[1]); pg.lineTo((l.x + l.w / 2) * 32, (l.y + l.h / 2) * 32); }
    groundL.addChild(pg);
  }
  // buildings / features
  for (const l of LOCS) drawLoc(l);
  // labels (always visible — vital once a generated bg replaces the buildings)
  for (const l of LOCS) {
    const t = new PIXI.Text(l.name, { fontFamily: "system-ui", fontSize: 12, fontWeight: "700", fill: 0xfff4d6, stroke: 0x2a1c0a, strokeThickness: 3 });
    t.anchor.set(0.5, 1); t.position.set((l.x + l.w / 2) * 32, l.y * 32 - 2); labelL.addChild(t);
  }
  if (hasBg) groundL.visible = false; // generated art replaces the drawn ground
}
function drawLoc(l) {
  const x = l.x * 32, y = l.y * 32, w = l.w * 32, h = l.h * 32, g = new PIXI.Graphics();
  const roof = { bakery: 0xd98a4a, forge: 0x6a6a72, tavern: 0x8a4a2a, library: 0x4a6fb0, market: 0x4caf7d, homes: 0xb0584a }[l.kind];
  if (roof !== undefined) {
    g.beginFill(0x000000, 0.22).drawEllipse(x + w / 2, y + h + 6, w * 0.5, 8).endFill();
    g.beginFill(0xc9b79a).drawRoundedRect(x + 4, y + h * 0.42, w - 8, h * 0.62, 4).endFill();        // walls
    g.beginFill(0x4a3526).drawRoundedRect(x + w / 2 - 7, y + h * 0.7, 14, h * 0.34, 2).endFill();    // door
    g.beginFill(roof).moveTo(x, y + h * 0.5).lineTo(x + w / 2, y - 4).lineTo(x + w, y + h * 0.5).closePath().endFill(); // roof
    g.lineStyle(2, 0x00000022).moveTo(x, y + h * 0.5).lineTo(x + w / 2, y - 4).lineTo(x + w, y + h * 0.5);
  } else if (l.kind === "plaza") {
    g.beginFill(0x9a8a6a).drawCircle(x + w / 2, y + h / 2, Math.min(w, h) * 0.5).endFill();
    g.beginFill(0x6fb3d6).drawCircle(x + w / 2, y + h / 2, 16).endFill();
    g.beginFill(0xbfe6f5).drawCircle(x + w / 2, y + h / 2, 7).endFill();
  } else if (l.kind === "garden") {
    g.beginFill(0x2f6b34).drawRoundedRect(x, y, w, h, 8).endFill();
    for (let i = 0; i < 6; i++) { const px = x + 14 + (i * 53) % (w - 20), py = y + 14 + ((i * 37) % (h - 20)); g.beginFill(0x000000, 0.2).drawEllipse(px, py + 9, 8, 3).endFill(); g.beginFill(0x6b4a2a).drawRect(px - 2, py, 4, 9).endFill(); g.beginFill(0x3a9a49).drawCircle(px, py - 4, 9).endFill(); }
  } else if (l.kind === "dock") {
    g.beginFill(0x355f86).drawRoundedRect(x - 4, y, w + 8, h, 6).endFill();
    g.beginFill(0x8a6a44).drawRect(x + w * 0.4, y + 6, 10, h - 12).endFill();
  }
  groundL.addChild(g);
}

// optional generated background (Qwen-Image). Falls back silently if absent.
(function loadBg() {
  const img = new Image();
  img.onload = () => { if (img.naturalWidth >= 400) { const sp = PIXI.Sprite.from(PIXI.Texture.from(img)); sp.width = W; sp.height = H; bgL.addChild(sp); hasBg = true; groundL.visible = false; } };
  img.onerror = () => {};
  img.src = "/town-bg.png?v=" + Date.now();
})();

// ---- characters -----------------------------------------------------------
let atlasBase = null; // generated sprite atlas (12 rows x 4 frames, 96x128)
function makeChar(pal, isHuman, palIdx) {
  // residents use the Qwen-generated animated sprite; the human "you" stays crafted (+ ring)
  if (atlasBase && !isHuman && palIdx >= 0 && palIdx < 12) return makeSpriteChar(palIdx);
  const c = new PIXI.Container();
  const [shirt, hair, skin] = pal;
  const shadow = new PIXI.Graphics().beginFill(0x000000, 0.28).drawEllipse(0, 17, 11, 4.5).endFill(); c.addChild(shadow);
  let ring = null;
  if (isHuman) { ring = new PIXI.Graphics().lineStyle(2.5, 0xffe07a, 0.9).drawEllipse(0, 16, 14, 6); c.addChild(ring); }
  const body = new PIXI.Container(); c.addChild(body);
  const legL = new PIXI.Graphics().beginFill(0x33302c).drawRoundedRect(-2, 0, 4, 9, 2).endFill(); legL.pivot.set(0, 0); legL.position.set(-3.5, 7);
  const legR = new PIXI.Graphics().beginFill(0x33302c).drawRoundedRect(-2, 0, 4, 9, 2).endFill(); legR.position.set(3.5, 7);
  const torso = new PIXI.Graphics(); torso.beginFill(shirt).drawRoundedRect(-8, -8, 16, 17, 6).endFill(); torso.lineStyle(1.5, 0x00000033).drawRoundedRect(-8, -8, 16, 17, 6);
  const head = new PIXI.Graphics();
  head.beginFill(skin).drawCircle(0, -15, 8).endFill();
  head.beginFill(hair).arc(0, -15, 8.5, Math.PI, 0).endFill();                         // hair cap
  head.beginFill(0x1a1a22).drawCircle(-3, -15, 1.4).drawCircle(3, -15, 1.4).endFill(); // eyes
  body.addChild(legL, legR, torso, head);
  const nm = new PIXI.Text("", { fontFamily: "system-ui", fontSize: 11, fontWeight: "700", fill: isHuman ? 0xffe07a : 0xffffff, stroke: 0x10131c, strokeThickness: 3 });
  nm.anchor.set(0.5, 0); nm.position.set(0, 21); c.addChild(nm);
  c._p = { body, legL, legR, head, nm, ring, walk: Math.random() * 6 };
  charL.addChild(c);
  return c;
}
function makeSpriteChar(row) {
  const c = new PIXI.Container();
  const shadow = new PIXI.Graphics().beginFill(0x000000, 0.30).drawEllipse(0, 16, 12, 4.5).endFill(); c.addChild(shadow);
  const frames = [0, 1, 2, 3].map(col => new PIXI.Texture(atlasBase, new PIXI.Rectangle(col * 96, row * 128, 96, 128)));
  const spr = new PIXI.AnimatedSprite(frames); spr.anchor.set(0.5, 1); spr.animationSpeed = 0.13;
  const sc = 50 / 128; spr.scale.set(sc); spr.position.set(0, 19); spr.gotoAndStop(0);
  c.addChild(spr);
  const nm = new PIXI.Text("", { fontFamily: "system-ui", fontSize: 11, fontWeight: "700", fill: 0xffffff, stroke: 0x10131c, strokeThickness: 3 });
  nm.anchor.set(0.5, 0); nm.position.set(0, 22); c.addChild(nm);
  c._p = { spr, nm, sc, isSprite: true };
  charL.addChild(c);
  return c;
}

// ---- networking -----------------------------------------------------------
function connect() {
  ws = new WebSocket(`ws://${location.host}/ws`);
  ws.onmessage = (e) => {
    for (const line of e.data.split("\n")) {
      if (!line) continue;
      const i = line.indexOf("\t"), tag = i < 0 ? line : line.slice(0, i), rest = i < 0 ? "" : line.slice(i + 1);
      if (tag === "w") myId = +rest;
      else if (tag === "clk") phase = +rest;
      else if (tag === "map") {
        const p = rest.split("\t");
        LOCS = (p[3] || "").split(";").filter(Boolean).map(s => { const a = s.split(","); return { name: a[0], x: +a[1], y: +a[2], w: +a[3], h: +a[4], kind: a[5] }; });
        buildScenery();
      } else if (tag === "p") {
        const seen = new Set();
        for (const s of rest.split(";")) {
          if (!s) continue; const a = s.split(","); const id = +a[0], x = +a[1], y = +a[2], pal = +a[3], face = +a[4], you = +a[5];
          seen.add(id);
          let v = chars.get(id);
          if (!v) { v = { dx: x, dy: y, tx: x, ty: y, face: 1, view: makeChar(pal === 99 ? humanPal : PAL[pal % 12], pal === 99 || you === 1, pal) }; chars.set(id, v); }
          v.tx = x; v.ty = y; v.tface = face === 0 ? -1 : 1;
        }
        for (const [id, v] of chars) if (!seen.has(id)) { charL.removeChild(v.view); chars.delete(id); const b = bubbleL.getChildByName("b" + id); if (b) bubbleL.removeChild(b); }
      } else if (tag === "r") {
        roster = rest.split(";").filter(Boolean).map(s => { const a = s.split("|"); return { id: +a[0], name: a[1], kind: a[2], act: a[3] }; });
        for (const e of roster) { const v = chars.get(e.id); if (v && v.view._p.nm) v.view._p.nm.text = e.name; }
        renderRoster();
      } else if (tag === "b") {
        const active = new Set();
        for (const s of rest.split(";")) { if (!s) continue; const j = s.indexOf("|"); const id = +s.slice(0, j), text = s.slice(j + 1); active.add(id); setBubble(id, text); }
        for (const ch of [...bubbleL.children]) { const id = +ch.name.slice(1); if (!active.has(id)) bubbleL.removeChild(ch); }
      }
    }
  };
  ws.onclose = () => setTimeout(connect, 1000);
}

function setBubble(id, text) {
  let b = bubbleL.getChildByName("b" + id);
  if (!b) {
    b = new PIXI.Container(); b.name = "b" + id;
    b._bg = new PIXI.Graphics(); b._tx = new PIXI.Text("", { fontFamily: "system-ui", fontSize: 12, fill: 0x14171f, wordWrap: true, wordWrapWidth: 150, lineHeight: 15 });
    b._tx.position.set(8, 6); b.addChild(b._bg, b._tx); bubbleL.addChild(b);
  }
  if (b._last !== text) {
    b._last = text; b._tx.text = text;
    const w = Math.min(166, b._tx.width + 16), h = b._tx.height + 12;
    b._bg.clear().beginFill(0xffffff, 0.96).drawRoundedRect(0, 0, w, h, 8).endFill().beginFill(0xffffff, 0.96).moveTo(w / 2 - 6, h).lineTo(w / 2 + 6, h).lineTo(w / 2, h + 7).closePath().endFill();
    b._w = w; b._h = h;
  }
}

// ---- loop -----------------------------------------------------------------
app.ticker.add(() => {
  const dt = app.ticker.deltaMS / 1000;
  for (const v of chars.values()) {
    const k = Math.min(1, dt * 9), ox = v.dx, oy = v.dy;
    v.dx += (v.tx - v.dx) * k; v.dy += (v.ty - v.dy) * k;
    const moving = Math.hypot(v.dx - ox, v.dy - oy) > 0.15;
    if (v.tface) v.face = v.tface;
    const p = v.view._p;
    v.view.position.set(v.dx, v.dy);
    if (p.isSprite) {
      p.spr.scale.x = p.sc * v.face; p.spr.scale.y = p.sc;
      if (moving) { if (!p.spr.playing) p.spr.play(); }
      else if (p.spr.playing) { p.spr.stop(); p.spr.gotoAndStop(0); }
    } else {
      p.body.scale.x = v.face;
      if (moving) { p.walk += dt * 12; p.legL.rotation = Math.sin(p.walk) * 0.6; p.legR.rotation = -Math.sin(p.walk) * 0.6; p.body.y = -Math.abs(Math.sin(p.walk)) * 1.5; }
      else { p.legL.rotation *= 0.7; p.legR.rotation *= 0.7; p.body.y *= 0.7; }
      if (p.ring) { p.ring.alpha = 0.5 + 0.4 * Math.sin(performance.now() / 300); }
    }
    v.view.zIndex = v.dy;
  }
  charL.children.sort((a, b) => a.zIndex - b.zIndex);
  // bubbles follow their character
  for (const b of bubbleL.children) { const v = chars.get(+b.name.slice(1)); if (v) b.position.set(v.dx - (b._w || 80) / 2, v.dy - 40 - (b._h || 24)); }
  // day/night
  const [col, a] = skyTint(phase); night.tint = col; night.alpha = a;
  updateClock();
});
function skyTint(p) {
  if (p < 0.22) return [0xffd9a0, 0.16 * (1 - p / 0.22) + 0.04];
  if (p < 0.5) return [0xffffff, 0.0];
  if (p < 0.72) return [0xffb066, 0.12 * (p - 0.5) / 0.22];
  if (p < 0.85) return [0xff8a4a, 0.20];
  return [0x16204a, 0.20 + 0.34 * Math.min(1, (p - 0.85) / 0.15)];
}
let lastClk = -1;
function updateClock() {
  if (Math.abs(phase - lastClk) < 0.002) return; lastClk = phase;
  const hr = (6 + phase * 24) % 24, h = Math.floor(hr), m = Math.floor((hr - h) * 60);
  $("clock").textContent = String(h).padStart(2, "0") + ":" + String(m).padStart(2, "0");
  $("phase").textContent = phase < 0.22 ? "morning" : phase < 0.5 ? "midday" : phase < 0.72 ? "afternoon" : phase < 0.85 ? "evening" : "night";
}
function renderRoster() {
  $("rlist").innerHTML = roster.map(r => {
    const col = r.kind === "you/visitor" ? "#ffd24a" : "#" + (PAL[(r.id - 1) % 12] ? PAL[(r.id - 1) % 12][0].toString(16).padStart(6, "0") : "888888");
    const me = r.id === myId ? " (you)" : "";
    return `<div class="rrow"><span class="dot" style="background:${col}"></span><span class="nm">${esc(r.name)}${me}</span><span class="ac">${esc(r.act)}</span></div>`;
  }).join("");
}
const esc = (s) => (s || "").replace(/[&<>]/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" }[c]));

// ---- input ----------------------------------------------------------------
app.view.addEventListener("click", (e) => {
  if (!started) return;
  const r = app.view.getBoundingClientRect();
  const x = (e.clientX - r.left) / r.width * W, y = (e.clientY - r.top) / r.height * H;
  ws && ws.readyState === 1 && ws.send(`go ${x.toFixed(0)} ${y.toFixed(0)}`);
});
$("say").addEventListener("keydown", (e) => {
  if (e.key === "Enter" && e.target.value.trim()) { ws && ws.readyState === 1 && ws.send("say " + e.target.value.trim()); e.target.value = ""; }
});
function afterAtlas(nick) {
  connect();
  const wait = setInterval(() => { if (ws && ws.readyState === 1) { if (nick) ws.send("name " + nick); clearInterval(wait); $("say").focus(); } }, 100);
}
function start() {
  const nick = $("nick").value.trim();
  started = true; $("join").style.display = "none";
  const img = new Image();
  img.onload = () => { atlasBase = PIXI.BaseTexture.from(img); atlasBase.scaleMode = PIXI.SCALE_MODES.NEAREST; afterAtlas(nick); };
  img.onerror = () => afterAtlas(nick);
  img.src = "/town-sprites.png?v=" + Date.now();
}
$("go").onclick = start;
$("nick").addEventListener("keydown", (e) => { if (e.key === "Enter") start(); });
