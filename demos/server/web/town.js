// opcusdb Hearth, PixiJS client for the AI town. Renders crafted, animated
// residents on a (optionally Qwen-Image-generated) map; you walk in and talk to
// whoever is nearby. Server is authoritative; this interpolates + animates.

const W = 960, H = 608;
const $ = (id) => document.getElementById(id);
let ws = null, myId = 0, started = false;
const chars = new Map();   // id -> view state
let roster = [];           // [{id,name,kind,act}]
let phase = 0.25, firstSnapDone = false, townNews = ""; // skip join notes for visitors already present on load
let LOCS = [];

// 12 resident palettes [shirt, hair, skin]; 99 = the human visitor
const PAL = [
  [0xe2574c, 0x3a2418, 0xf0c9a0], [0x4a6fb0, 0x111111, 0xe6b98e], [0x4caf7d, 0x6b4a2a, 0xf0c9a0],
  [0xc24d8a, 0x8a5a2a, 0xe6b98e], [0x6c63b5, 0x2a2a2a, 0xf0c9a0], [0xd98a2b, 0x4a2f18, 0xe6b98e],
  [0xb04ad9, 0x1a1a1a, 0xf0c9a0], [0x5a7d8c, 0x9a9a9a, 0xdcb38a], [0x3fa7a0, 0x5a3a1a, 0xf0c9a0],
  [0xe8b54a, 0xc25a2a, 0xf2d0aa], [0x8a8f5a, 0x2a2a2a, 0xe6b98e], [0x4a90e2, 0x6b4a2a, 0xf0c9a0],
];
const humanPal = [0xffd24a, 0x3a2418, 0xf0c9a0];
// distinct ring colors so visitors who share the town can be told apart; "you" is gold
const VIS_RINGS = [0x6ad1ff, 0xff7ab0, 0x8aff6a, 0xc79bff, 0xffae5b, 0x5bffd0, 0xff6a6a];
const visitorColor = (id) => VIS_RINGS[id % VIS_RINGS.length];

// render at the display's pixel ratio (capped) so the art stays crisp on retina and
// hidpi screens instead of being upscaled softly by the fit() CSS scaling below
const app = new PIXI.Application({ width: W, height: H, backgroundColor: 0x223021, antialias: true, resolution: Math.min(2, window.devicePixelRatio || 1), autoDensity: true });
$("app").appendChild(app.view);
function fit() { const s = Math.min(innerWidth / W, innerHeight / H) * 0.98; app.view.style.width = W * s + "px"; app.view.style.height = H * s + "px"; }
addEventListener("resize", fit); fit();

const bgL = new PIXI.Container(), groundL = new PIXI.Container(), labelL = new PIXI.Container(), charL = new PIXI.Container(), bubbleL = new PIXI.Container(), fxL = new PIXI.Container(), nightL = new PIXI.Container(), glowL = new PIXI.Container(), selL = new PIXI.Container();
// bubbleL sits ABOVE the night tint so speech (the dialogue) stays crisp and readable
// after dark instead of being multiplied toward blue with the rest of the scene
app.stage.addChild(bgL, groundL, labelL, charL, fxL, nightL, glowL, bubbleL, selL);
fxL.eventMode = "none"; glowL.eventMode = "none"; selL.eventMode = "none";
const selRing = new PIXI.Graphics(); selL.addChild(selRing); let selectedId = 0;
const clickMark = new PIXI.Graphics(); selL.addChild(clickMark); clickMark.visible = false; let clickT = 0; // brief ring where you click to walk
const bios = {}; // id -> one-line persona, for the inspect card
// warm lamp glows + drifting fireflies, drawn above the night tint so they shine in the dark
const lampG = new PIXI.Graphics(); const fireG = new PIXI.Graphics(); fireG.blendMode = PIXI.BLEND_MODES.ADD; glowL.addChild(lampG, fireG);
const fireflies = Array.from({ length: 38 }, () => ({ x: Math.random() * W, y: Math.random() * H, vx: (Math.random() - 0.5) * 10, vy: (Math.random() - 0.5) * 10, ph: Math.random() * 6 }));
// lamp-glow / firefly / music-dim intensity, interpolated continuously and aligned
// with the sky keyframes so the glows ramp in and out without popping at dawn
const NIGHT_KEYS = [
  [0.00, 0.85], [0.10, 0.45], [0.16, 0.0], [0.50, 0.0],
  [0.62, 0.15], [0.74, 0.40], [0.83, 0.68], [0.92, 0.90], [1.00, 0.85],
];
function nightAmt(p) {
  let i = 0; while (i < NIGHT_KEYS.length - 1 && p >= NIGHT_KEYS[i + 1][0]) i++;
  const a = NIGHT_KEYS[i], b = NIGHT_KEYS[Math.min(i + 1, NIGHT_KEYS.length - 1)];
  const t = b[0] > a[0] ? (p - a[0]) / (b[0] - a[0]) : 0;
  return a[1] + (b[1] - a[1]) * t;
}
let hasBg = false;
const night = new PIXI.Graphics().beginFill(0xffffff).drawRect(0, 0, W, H).endFill();
nightL.addChild(night); nightL.eventMode = "none"; night.blendMode = PIXI.BLEND_MODES.MULTIPLY;

// ---- map / scenery --------------------------------------------------------
function buildScenery() {
  groundL.removeChildren(); labelL.removeChildren();
  // plain grass fallback (hidden when the generated map loads)
  const g = new PIXI.Graphics();
  for (let y = 0; y < 19; y++) for (let x = 0; x < 30; x++) { g.beginFill((x + y) % 2 ? 0x33502c : 0x39572f).drawRect(x * 32, y * 32, 32, 32).endFill(); }
  groundL.addChild(g);
  // location signs sit just above each ground stand point (pixel coords)
  for (const l of LOCS) {
    // place labels are static reference info, so let them sit on the ground plane and
    // recede a little: the living residents' name tags (white, above their heads) should
    // visually dominate, and this keeps the two from stacking muddily at a node
    const t = new PIXI.Text(l.name, { fontFamily: "system-ui", fontSize: 11, fontWeight: "700", fill: 0xf2e2bd, stroke: 0x2a1c0a, strokeThickness: 4 });
    t.anchor.set(0.5, 1); t.position.set(l.x, l.y - 30); t.alpha = 0.8; labelL.addChild(t);
  }
  // soft warm lamp glow near each spot, brightest at night
  lampG.clear(); lampG.blendMode = PIXI.BLEND_MODES.ADD;
  for (const l of LOCS) { const col = l.kind === "plaza" ? 0x8fbfff : 0xffb24a; for (let i = 8; i >= 1; i--) lampG.beginFill(col, 0.05).drawCircle(l.x, l.y - 6, 6 * i).endFill(); }
  if (hasBg) groundL.visible = false;
}

// generated town map; falls back silently to the grass above if absent.
(function loadBg() {
  const img = new Image();
  img.onload = () => { if (img.naturalWidth >= 400) { const sp = PIXI.Sprite.from(PIXI.Texture.from(img)); sp.width = W; sp.height = H; bgL.addChild(sp); hasBg = true; groundL.visible = false; } };
  img.onerror = () => {};
  img.src = "/town-bg.png"; // no cache-buster, so the browser can cache the 1.7MB art
})();

// ---- characters -----------------------------------------------------------
let atlasBase = null; // generated sprite atlas: 13 rows (12 residents + 1 traveler) x 4 frames, 96x128 each
function makeChar(pal, isHuman, palIdx, ringCol) {
  // everyone uses a generated sprite when the atlas is loaded; the human "you" is
  // the traveler in row 12 with a gold ring (other visitors get their own color).
  if (atlasBase) return makeSpriteChar(isHuman ? 12 : (palIdx % 12), isHuman, ringCol);
  const c = new PIXI.Container();
  const [shirt, hair, skin] = pal;
  const shadow = new PIXI.Graphics().beginFill(0x000000, 0.28).drawEllipse(0, 17, 11, 4.5).endFill(); c.addChild(shadow);
  let ring = null;
  if (isHuman) { ring = new PIXI.Graphics().lineStyle(2.5, ringCol || 0xffe07a, 0.9).drawEllipse(0, 16, 14, 6); c.addChild(ring); }
  const body = new PIXI.Container(); c.addChild(body);
  const legL = new PIXI.Graphics().beginFill(0x33302c).drawRoundedRect(-2, 0, 4, 9, 2).endFill(); legL.pivot.set(0, 0); legL.position.set(-3.5, 7);
  const legR = new PIXI.Graphics().beginFill(0x33302c).drawRoundedRect(-2, 0, 4, 9, 2).endFill(); legR.position.set(3.5, 7);
  const torso = new PIXI.Graphics(); torso.beginFill(shirt).drawRoundedRect(-8, -8, 16, 17, 6).endFill(); torso.lineStyle(1.5, 0x00000033).drawRoundedRect(-8, -8, 16, 17, 6);
  const head = new PIXI.Graphics();
  head.beginFill(skin).drawCircle(0, -15, 8).endFill();
  head.beginFill(hair).arc(0, -15, 8.5, Math.PI, 0).endFill();                         // hair cap
  head.beginFill(0x1a1a22).drawCircle(-3, -15, 1.4).drawCircle(3, -15, 1.4).endFill(); // eyes
  body.addChild(legL, legR, torso, head);
  const nm = new PIXI.Text("", { fontFamily: "system-ui", fontSize: 11, fontWeight: "700", fill: isHuman ? (ringCol || 0xffe07a) : 0xffffff, stroke: 0x10131c, strokeThickness: 4 });
  nm.anchor.set(0.5, 0); nm.position.set(0, -38); c.addChild(nm); // name above the head
  c._p = { body, legL, legR, head, nm, ring, walk: Math.random() * 6 };
  charL.addChild(c);
  return c;
}
function makeSpriteChar(row, isHuman, ringCol) {
  const c = new PIXI.Container();
  const shadow = new PIXI.Graphics().beginFill(0x000000, 0.30).drawEllipse(0, 16, 12, 4.5).endFill(); c.addChild(shadow);
  let ring = null;
  if (isHuman) { ring = new PIXI.Graphics().lineStyle(2.5, ringCol || 0xffe07a, 0.9).drawEllipse(0, 16, 15, 6); c.addChild(ring); }
  // one clean frame, animated procedurally (bob + sway + squash + lean + a scaling
  // shadow): reads as a grounded walk and avoids the jitter of cycling four inconsistent
  // generated frames. True multi-frame leg articulation would need an image-to-video walk
  // cycle (the LTX 2.3 22B model under ~/Documents/ltx is installed but heavy: a separate
  // ComfyUI, slow on this machine, and the video-to-clean-sprite conversion needs a human
  // eye), so that is a supervised pass; this procedural walk is the standalone solution.
  const spr = new PIXI.Sprite(new PIXI.Texture(atlasBase, new PIXI.Rectangle(0, row * 128, 96, 128)));
  spr.anchor.set(0.5, 1); const sc = 50 / 128; spr.scale.set(sc); spr.position.set(0, 19);
  c.addChild(spr);
  const nm = new PIXI.Text("", { fontFamily: "system-ui", fontSize: 11, fontWeight: "700", fill: isHuman ? (ringCol || 0xffe07a) : 0xffffff, stroke: 0x10131c, strokeThickness: 4 });
  nm.anchor.set(0.5, 0); nm.position.set(0, -56); c.addChild(nm); // name above the head
  c._p = { spr, nm, sc, isSprite: true, bob: Math.random() * 6, shadow, ring };
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
        LOCS = (p[3] || "").split(";").filter(Boolean).map(s => { const a = s.split(","); return { name: a[0], x: +a[1], y: +a[2], kind: a[3] }; });
        buildScenery();
      } else if (tag === "p") {
        const seen = new Set();
        for (const s of rest.split(";")) {
          if (!s) continue; const a = s.split(","); const id = +a[0], x = +a[1], y = +a[2], pal = +a[3], face = +a[4], you = id === myId ? 1 : 0; // the snapshot is shared, so we mark our own character ourselves
          seen.add(id);
          let v = chars.get(id);
          if (!v) { const human = pal === 99 || you === 1; const ringCol = human ? (you === 1 ? 0xffe07a : visitorColor(id)) : null; v = { dx: x, dy: y, tx: x, ty: y, face: 1, pal, view: makeChar(pal === 99 ? humanPal : PAL[pal % 12], human, pal, ringCol) }; chars.set(id, v);
            if (firstSnapDone && pal === 99 && you !== 1) logSystem("A traveler walked into Hearth."); } // notice friends arriving
          v.tx = x; v.ty = y; v.tface = face === 0 ? -1 : 1;
        }
        for (const [id, v] of chars) if (!seen.has(id)) { if (firstSnapDone && v.pal === 99) logSystem(nameOf(id) + " left Hearth."); charL.removeChild(v.view); chars.delete(id); lastLine.delete(id); if (selectedId === id) selectedId = 0; const b = bubbleL.getChildByName("b" + id); if (b) bubbleL.removeChild(b); }
        firstSnapDone = true;
      } else if (tag === "r") {
        roster = rest.split(";").filter(Boolean).map(s => { const a = s.split("|"); return { id: +a[0], name: a[1], kind: a[2], act: a[3] }; });
        for (const e of roster) { const v = chars.get(e.id); if (v && v.view._p.nm) v.view._p.nm.text = e.name; }
        renderRoster();
      } else if (tag === "bio") {
        for (const s of rest.split(";")) { if (!s) continue; const j = s.indexOf("|"); bios[+s.slice(0, j)] = s.slice(j + 1); }
      } else if (tag === "news") {
        if (rest && rest !== townNews) { townNews = rest; logSystem("Talk of the town: " + rest); } // log on join and at each day's change
      } else if (tag === "b") {
        const active = new Set();
        for (const s of rest.split(";")) { if (!s) continue; const j = s.indexOf("|"); const id = +s.slice(0, j), text = s.slice(j + 1); active.add(id); setBubble(id, text); logChatter(id, text); }
        for (const ch of [...bubbleL.children]) { const id = +ch.name.slice(1); if (!active.has(id)) ch._dying = ch._dying || performance.now(); } // mark for fade-out, removed by the loop
      }
    }
  };
  // on a reconnect, treat the next snapshot as a fresh load so reconciling the old
  // characters does not fire spurious arrived/left notes
  ws.onclose = () => { firstSnapDone = false; setTimeout(connect, 1000); };
}

function setBubble(id, text) {
  let b = bubbleL.getChildByName("b" + id);
  if (!b) {
    b = new PIXI.Container(); b.name = "b" + id; b._born = performance.now(); // for a brief fade-in
    b._bg = new PIXI.Graphics(); b._tail = new PIXI.Graphics(); b._tx = new PIXI.Text("", { fontFamily: "system-ui", fontSize: 12, fill: 0x14171f, wordWrap: true, wordWrapWidth: 150, lineHeight: 15 });
    b._tx.position.set(8, 6); b.addChild(b._bg, b._tail, b._tx); bubbleL.addChild(b);
  }
  b._dying = 0; // active again, cancel any fade-out
  if (b._last !== text) {
    b._last = text; b._tx.text = text;
    const w = Math.min(166, b._tx.width + 16), h = b._tx.height + 12;
    b._bg.clear().beginFill(0xffffff, 0.96).drawRoundedRect(0, 0, w, h, 8).endFill();
    b._tail.clear().beginFill(0xffffff, 0.96).moveTo(w / 2 - 6, h).lineTo(w / 2 + 6, h).lineTo(w / 2, h + 7).closePath().endFill();
    b._w = w; b._h = h;
  }
}

// little floating emote puffs that rise and fade, to make idle residents lively.
// each resident pulls from a pool that fits their trade, with a generic fallback.
const EMOTES = ["💬", "🎵", "✨", "😄", "💤", "❓", "❤️", "☕", "🌸", "🙂", "💡"];
const ROLE_EMOTES = {
  0: ["🍞", "🥐", "😋", "🔥"], 1: ["🔨", "⚒️", "🔥", "💪"], 2: ["🌸", "🌱", "🌻", "💧"],
  3: ["🍺", "🍻", "😄", "🎶"], 4: ["📖", "📚", "🤔", "🔎"], 5: ["💰", "🪙", "✨", "🤝"],
  6: ["🎵", "🎶", "🎭", "💃"], 7: ["🐟", "🎣", "🌊", "😤"], 8: ["❤️", "🌿", "💊", "😊"],
  9: ["⭐", "🎈", "❓", "😆"], 10: ["🧵", "🧶", "👀", "😒"], 11: ["✉️", "📜", "💨", "🏃"],
  99: ["🙂", "💬", "✨", "👋"],
};
function pickEmote(v) { const pool = ROLE_EMOTES[v.pal] || EMOTES; return pool[(Math.random() * pool.length) | 0]; }
function addEmote(x, y, txt) {
  if (fxL.children.length > 36) return;
  const t = new PIXI.Text(txt, { fontFamily: "system-ui", fontSize: 19 });
  t.anchor.set(0.5, 1); t.position.set(x, y); t.life = 1.2; fxL.addChild(t);
}

// ---- loop -----------------------------------------------------------------
app.ticker.add(() => {
  const dt = app.ticker.deltaMS / 1000;
  const speakers = new Set(bubbleL.children.map(b => +b.name.slice(1))); // who is talking right now
  for (const [id, v] of chars) {
    const k = Math.min(1, dt * 9), ox = v.dx, oy = v.dy;
    v.dx += (v.tx - v.dx) * k; v.dy += (v.ty - v.dy) * k;
    const vx = v.dx - ox, vy = v.dy - oy, spd = Math.hypot(vx, vy);
    v.spd = (v.spd || 0) + (spd - (v.spd || 0)) * Math.min(1, dt * 6); // smoothed pace drives the step cadence
    const moving = spd > 0.15;
    // face the way you travel, but only flip when sideways motion is a real component:
    // a mostly-vertical walk (up or down a path) keeps a stable facing instead of
    // flickering left and right on tiny horizontal jitter
    if (moving && Math.abs(vx) > 0.12 && Math.abs(vx) > Math.abs(vy) * 0.35) v.face = vx < 0 ? -1 : 1;
    const leanT = moving ? Math.max(-0.14, Math.min(0.14, vx * 0.05)) : 0;
    v.lean = (v.lean || 0) + (leanT - (v.lean || 0)) * Math.min(1, dt * 8); // ease a lean into the motion
    // when standing, turn toward the active speaker nearby (so a group attends to whoever
    // is talking), or the nearest neighbour otherwise; the speaker does a talking bounce
    const talking = speakers.has(id);
    if (!moving) {
      let bx = null, bd = 90 * 90, sx = null, sd = 90 * 90;
      for (const [oid, ov] of chars) {
        if (ov === v) continue;
        const d = (ov.dx - v.dx) ** 2 + (ov.dy - v.dy) ** 2;
        if (d < bd) { bd = d; bx = ov.dx; }
        if (d < sd && speakers.has(oid)) { sd = d; sx = ov.dx; }
      }
      const target = sx != null ? sx : bx; // prefer the speaker
      if (target != null && Math.abs(target - v.dx) > 4) v.face = target < v.dx ? -1 : 1;
      v.talk = talking ? (v.talk || 0) + dt * 9 : 0;
    } else v.talk = 0;
    const p = v.view._p;
    v.view.position.set(v.dx, v.dy);
    // idle actions: when standing around, occasionally hop and puff an emote
    if (moving) { v.idle = Math.max(v.idle || 0, 1.5); }
    else {
      v.idle = (v.idle == null ? 2 + Math.random() * 5 : v.idle) - dt;
      // residents (NPCs) emote on their own; a human's avatar should not act autonomously
      if (v.idle <= 0) { if (!talking && v.pal !== 99) { addEmote(v.dx, v.dy - (p.isSprite ? 54 : 40), pickEmote(v)); v.hop = 1; } v.idle = 4 + Math.random() * 7; }
    }
    if (v.hop > 0) v.hop -= dt * 3.2;
    if (p.isSprite) {
      p.spr.scale.x = p.sc * v.face;
      if (moving) {
        p.bob += Math.min(0.4, v.spd * 0.32);                // step cadence tracks distance moved, not wall time
        const s = Math.sin(p.bob);
        p.spr.y = 19 - Math.abs(s) * 4.5;                 // hop while walking
        p.spr.rotation = s * 0.06 * v.face + (v.lean || 0); // sway plus a lean into the direction of travel
        p.spr.scale.y = p.sc * (1 - Math.abs(s) * 0.05);  // squash
        p.shadow.scale.set(1 + Math.abs(s) * 0.12, 1);
      } else {
        const talkb = v.talk ? Math.abs(Math.sin(v.talk)) * 3 : 0;            // bounce while speaking
        const lift = Math.max(0, v.hop) * 9 + talkb;
        const breathe = Math.sin(performance.now() / 700 + p.bob) * 0.02;     // gentle breathing
        p.spr.y += (19 - lift - p.spr.y) * 0.3; p.spr.rotation *= 0.8; p.spr.scale.y = p.sc * (1 + breathe + (v.talk ? Math.sin(v.talk * 1.3) * 0.03 : 0)); p.shadow.scale.set(1, 1);
      }
      if (p.ring) p.ring.alpha = 0.5 + 0.4 * Math.sin(performance.now() / 300);
    } else {
      p.body.scale.x = v.face;
      if (moving) { p.walk += Math.min(0.45, v.spd * 0.36); p.legL.rotation = Math.sin(p.walk) * 0.6; p.legR.rotation = -Math.sin(p.walk) * 0.6; p.body.y = -Math.abs(Math.sin(p.walk)) * 1.5; }
      else { p.legL.rotation *= 0.7; p.legR.rotation *= 0.7; p.body.y = -Math.max(0, v.hop) * 6; }
      if (p.ring) { p.ring.alpha = 0.5 + 0.4 * Math.sin(performance.now() / 300); }
    }
    v.view.zIndex = v.dy;
  }
  // de-overlap name tags within a cluster: stack each label above the lower-id ones
  // nearby, so a group's names stay readable without moving the characters
  for (const [id, v] of chars) {
    const p = v.view._p; if (!p.nm) continue;
    let above = 0;
    for (const [oid, ov] of chars) { if (oid >= id) continue; const dx = ov.dx - v.dx, dy = ov.dy - v.dy; if (dx * dx + dy * dy < 26 * 26) above++; }
    p.nm.y = (p.isSprite ? -56 : -38) - Math.min(above, 4) * 13;
  }
  // animate emote puffs: rise and fade
  for (const e of [...fxL.children]) { e.y -= dt * 24; e.life -= dt; e.alpha = Math.max(0, Math.min(1, e.life * 1.6)); e.scale.set(1 + (1.2 - e.life) * 0.25); if (e.life <= 0) fxL.removeChild(e); }
  charL.children.sort((a, b) => a.zIndex - b.zIndex);
  // bubbles follow their character, stacked upward so a group stays readable
  const placed = [];
  const bs = bubbleL.children.map(b => ({ b, v: chars.get(+b.name.slice(1)) })).filter(x => x.v).sort((a, z) => a.v.dy - z.v.dy);
  for (const { b, v } of bs) {
    const w = b._w || 80, h = b._h || 24;
    const bx = Math.max(2, Math.min(W - w - 2, v.dx - w / 2)), natural = v.dy - 40 - h; let by = natural; // keep on-screen horizontally
    let n = 0;
    while (n++ < 8 && placed.some(p => bx < p.x + p.w && bx + w > p.x && by < p.y + p.h && by + h > p.y)) by -= h + 4;
    by = Math.max(4, by); // never let a stacked bubble run off the top edge
    b.position.set(bx, by); placed.push({ x: bx, y: by, w, h });
    // show the tail only when the bubble is at its natural spot and centered on the speaker
    if (b._tail) b._tail.visible = by >= natural - 0.5 && Math.abs(bx + w / 2 - v.dx) < 6;
    const now = performance.now();
    let a = Math.min(1, (now - (b._born || 0)) / 140);                              // fade in when it appears
    if (b._dying) { a = Math.min(a, Math.max(0, 1 - (now - b._dying) / 140)); if (now - b._dying > 150) bubbleL.removeChild(b); } // fade out when it stops
    b.alpha = a;
  }
  // day/night
  const [col, a] = skyTint(phase); night.tint = col; night.alpha = a;
  // lamp glow + fireflies fade in as it gets dark
  const na = nightAmt(phase);
  glowL.alpha = na;
  // the score breathes with the cycle: darker filter and softer pad after dark
  musicNight = na;
  if (audio) { audio.filt.frequency.value = 950 - 470 * na; audio.pad.gain.value = 0.16 - 0.05 * na; }
  fireG.clear();
  for (const f of fireflies) {
    f.x += f.vx * dt; f.y += f.vy * dt; f.ph += dt * 2.2;
    if (f.x < 0) f.x += W; else if (f.x > W) f.x -= W;
    if (f.y < 0) f.y += H; else if (f.y > H) f.y -= H;
    const fa = 0.25 + 0.55 * Math.max(0, Math.sin(f.ph));
    fireG.beginFill(0xffe88a, fa).drawCircle(f.x, f.y, 1.7).endFill();
  }
  // click-to-walk marker: a flat ring that expands and fades at the spot you clicked
  if (clickT > 0) {
    clickT -= dt; const t = 1 - Math.max(0, clickT) / 0.5, r = 6 + t * 16;
    clickMark.clear().lineStyle(2.5, 0xffe07a, Math.max(0, 0.85 * (1 - t))).drawEllipse(0, 0, r, r * 0.5);
    if (clickT <= 0) clickMark.visible = false;
  }
  // selection ring + inspect card for a clicked townsperson
  const sel = selectedId && chars.get(selectedId);
  selRing.clear();
  const insp = $("inspect");
  if (sel) {
    const pr = 17 + Math.sin(performance.now() / 240) * 2;
    selRing.lineStyle(2.5, 0xffe07a, 0.9).drawEllipse(sel.dx, sel.dy + 14, pr, pr * 0.42);
    const r = roster.find(rr => rr.id === selectedId);
    if (r && insp) { insp.style.display = "block"; const where = r.act === "walking" ? "walking" : "at the " + r.act; const kind = r.kind === "you/visitor" ? "a visitor" : r.kind; const said = lastLine.get(selectedId); const saidHtml = said ? `<div class="rl" style="max-width:260px;margin-top:3px;font-style:italic;opacity:.85">“${esc(said)}”</div>` : ""; const bio = bios[selectedId] ? `<div class="rl" style="max-width:260px;margin-top:3px">${esc(bios[selectedId])}</div>` : ""; insp.innerHTML = `<div class="nm">${esc(r.name)}</div><div class="rl">${esc(kind)}, ${esc(where)}</div>${saidHtml}${bio}`; }
  } else if (insp) { insp.style.display = "none"; }
  updateClock();
});
// returns [tintColor, alpha]; applied as a MULTIPLY layer so the scene darkens and
// shifts hue naturally instead of being veiled by a flat overlay.
// keyframes (phase, tint color, multiply alpha) interpolated continuously, so the sky
// grades smoothly through dawn, day, sunset, dusk and night with no banding or pops
const SKY_KEYS = [
  [0.00, 0x2c3a66, 0.58], // deep night (wraps to 1.00)
  [0.10, 0x33456f, 0.32], // dawn lifting
  [0.16, 0xffffff, 0.0],  // day breaks
  [0.50, 0xffffff, 0.0],  // midday, untinted
  [0.62, 0xffd2a0, 0.22], // afternoon, warm
  [0.74, 0xff9a5a, 0.40], // sunset, orange
  [0.83, 0x7d6a8f, 0.50], // dusk, orange fading to violet
  [0.92, 0x36436f, 0.58], // early night
  [1.00, 0x2c3a66, 0.58], // deep night
];
function lerpColor(c0, c1, t) {
  const r = ((c0 >> 16) & 255) + (((c1 >> 16) & 255) - ((c0 >> 16) & 255)) * t;
  const g = ((c0 >> 8) & 255) + (((c1 >> 8) & 255) - ((c0 >> 8) & 255)) * t;
  const b = (c0 & 255) + ((c1 & 255) - (c0 & 255)) * t;
  return (Math.round(r) << 16) | (Math.round(g) << 8) | Math.round(b);
}
function skyTint(p) {
  let i = 0; while (i < SKY_KEYS.length - 1 && p >= SKY_KEYS[i + 1][0]) i++;
  const a = SKY_KEYS[i], b = SKY_KEYS[Math.min(i + 1, SKY_KEYS.length - 1)];
  const t = b[0] > a[0] ? (p - a[0]) / (b[0] - a[0]) : 0;
  return [lerpColor(a[1], b[1], t), a[2] + (b[2] - a[2]) * t];
}
let lastClk = -1;
function updateClock() {
  if (Math.abs(phase - lastClk) < 0.002) return; lastClk = phase;
  // map the cycle so dawn is 06:00 and the dusk/night visuals land at believable
  // evening hours (the small wrap back to dawn happens during the dark quiet phase);
  // the label is derived from the same hour so clock, label, and tint all agree
  const hr = (6 + phase * 17.4) % 24, h = Math.floor(hr), m = Math.floor((hr - h) * 60);
  $("clock").textContent = String(h).padStart(2, "0") + ":" + String(m).padStart(2, "0");
  $("phase").textContent = hr < 8 ? "dawn" : hr < 11 ? "morning" : hr < 14 ? "midday" : hr < 17 ? "afternoon" : hr < 20 ? "evening" : "night";
}
function renderRoster() {
  $("rlist").innerHTML = roster.map(r => {
    const hex = (n) => "#" + (n >>> 0).toString(16).padStart(6, "0");
    // visitors: gold for you, their own ring colour for others, matching the world
    const col = r.kind === "you/visitor"
      ? (r.id === myId ? "#ffd24a" : hex(visitorColor(r.id)))
      : hex(PAL[(r.id - 1) % 12] ? PAL[(r.id - 1) % 12][0] : 0x888888);
    const me = r.id === myId ? " (you)" : "";
    return `<div class="rrow${r.id === selectedId ? " sel" : ""}" data-id="${r.id}"><span class="dot" style="background:${col}"></span><span class="nm">${esc(r.name)}${me}</span><span class="ac">${esc(r.act)}</span></div>`;
  }).join("");
  // surface how many humans share the town (the demo's whole point)
  const visitors = roster.filter(r => r.kind === "you/visitor").length;
  const h = document.querySelector("#roster h4");
  if (h) h.textContent = visitors > 1 ? `Townsfolk · ${visitors} visitors here` : "Townsfolk";
}
// click a roster row to select that resident (rings them in the world + shows the card)
$("rlist").addEventListener("click", (e) => { const row = e.target.closest(".rrow"); if (row && row.dataset.id) { selectedId = +row.dataset.id; renderRoster(); } });
const esc = (s) => (s || "").replace(/[&<>]/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" }[c]));

// live town chatter feed: log each new line a resident says so you can follow the
// town's conversations without standing next to everyone.
const lastLine = new Map();
let chatter = [];
function nameOf(id) { const r = roster.find(r => r.id === id); return r ? r.name : "someone"; }
function logChatter(id, text) {
  if (!text || lastLine.get(id) === text) return;
  lastLine.set(id, text);
  const now = performance.now(), last = chatter[chatter.length - 1];
  // a quick follow-up from the same speaker is the real reply upgrading the instant
  // line, so replace the entry in place rather than logging a duplicate
  if (last && last.id === id && now - last.t < 4000) { last.text = text; last.t = now; }
  else { chatter.push({ id, name: nameOf(id), text, t: now }); if (chatter.length > 9) chatter.shift(); }
  renderChatter();
}
// arrivals and departures of other visitors, so a shared town feels populated
function logSystem(text) {
  chatter.push({ id: -1, text, t: performance.now(), sys: true });
  if (chatter.length > 9) chatter.shift();
  renderChatter();
}
function renderChatter() {
  const el = $("chatter"); if (!el) return;
  el.innerHTML = chatter.map(c => c.sys ? `<div class="cl sys"><i>${esc(c.text)}</i></div>` : `<div class="cl"><b>${esc(c.name)}</b> ${esc(c.text)}</div>`).join("");
}

// ---- input ----------------------------------------------------------------
app.view.addEventListener("click", (e) => {
  if (!started) return;
  const r = app.view.getBoundingClientRect();
  const x = (e.clientX - r.left) / r.width * W, y = (e.clientY - r.top) / r.height * H;
  // click a townsperson to inspect them; click the ground to walk there
  let hit = 0, hd = 28 * 28;
  for (const [id, v] of chars) { if (id === myId) continue; const d = (v.dx - x) ** 2 + (v.dy - y) ** 2; if (d < hd) { hd = d; hit = id; } }
  if (hit) { selectedId = hit; }
  else { selectedId = 0; ws && ws.readyState === 1 && ws.send(`go ${x.toFixed(0)} ${y.toFixed(0)}`); clickMark.position.set(x, y); clickT = 0.5; clickMark.visible = true; }
});
$("say").addEventListener("keydown", (e) => {
  if (e.key === "Enter" && e.target.value.trim()) { ws && ws.readyState === 1 && ws.send("say " + e.target.value.trim()); e.target.value = ""; }
});
// Escape dismisses an inspected resident and unfocuses the chat box
addEventListener("keydown", (e) => { if (e.key === "Escape") { selectedId = 0; if (document.activeElement === $("say")) $("say").blur(); } });
// name who is within earshot so it is clear walking up to someone starts a chat
setInterval(() => {
  const me = chars.get(myId), say = $("say"); if (!started || !me || !say || document.activeElement === say) return;
  // match the server's grouping (within 70px of a location stand point) so the hint only
  // names people who will actually answer, not someone walking past or at the next node
  const R2 = 70 * 70;
  let myNode = -1;
  for (let i = 0; i < LOCS.length; i++) { if ((me.dx - LOCS[i].x) ** 2 + (me.dy - LOCS[i].y) ** 2 < R2) { myNode = i; break; } }
  const near = [];
  if (myNode >= 0) for (const [id, v] of chars) { if (id === myId) continue; if ((v.dx - LOCS[myNode].x) ** 2 + (v.dy - LOCS[myNode].y) ** 2 < R2) near.push(nameOf(id)); }
  say.placeholder = near.length ? ("talk to " + near.slice(0, 3).join(", ") + (near.length > 3 ? " and others here" : " here")) : "walk up to someone, then type to talk to them";
}, 600);
function afterAtlas(nick) {
  connect();
  const wait = setInterval(() => { if (ws && ws.readyState === 1) { if (nick) ws.send("name " + nick); clearInterval(wait); $("say").focus(); } }, 100);
}
// ---- ambient music (generated live with Web Audio, no asset, no dependency) ----
// A soft evolving pad plus a slow pentatonic music box, so the town has a cosy
// score. Starts on the first user gesture (the join click) to satisfy autoplay.
let audio = null, musicOn = true, musicNight = 0; // musicNight 0..1 dims the score at night
function startMusic() {
  if (audio) return;
  const Ctx = window.AudioContext || window.webkitAudioContext; if (!Ctx) return;
  const ctx = new Ctx();
  const master = ctx.createGain(); master.gain.value = 0; master.connect(ctx.destination);
  const filt = ctx.createBiquadFilter(); filt.type = "lowpass"; filt.frequency.value = 850; filt.Q.value = 0.6;
  const pad = ctx.createGain(); pad.gain.value = 0.16; filt.connect(pad); pad.connect(master);
  // sustained chord (A major, low and warm)
  for (const f of [110, 164.81, 220, 277.18]) { const o = ctx.createOscillator(); o.type = "sine"; o.frequency.value = f; const g = ctx.createGain(); g.gain.value = 0.22; o.connect(g); g.connect(filt); o.start(); }
  // slow filter sweep so the pad breathes
  const lfo = ctx.createOscillator(); lfo.frequency.value = 0.05; const lg = ctx.createGain(); lg.gain.value = 320; lfo.connect(lg); lg.connect(filt.frequency); lfo.start();
  // gentle music-box plucks on a pentatonic scale
  const scale = [440, 523.25, 587.33, 659.25, 783.99, 880, 1046.5];
  function pluck() {
    if (!audio) return;
    if (musicOn) {
      const f = scale[(Math.random() * scale.length) | 0];
      const o = ctx.createOscillator(); o.type = "triangle"; o.frequency.value = f;
      const g = ctx.createGain(); const t = ctx.currentTime;
      const peak = 0.09 - 0.05 * musicNight;                              // softer after dark
      g.gain.setValueAtTime(0.0001, t); g.gain.exponentialRampToValueAtTime(peak, t + 0.02); g.gain.exponentialRampToValueAtTime(0.0001, t + 1.6);
      o.connect(g); g.connect(pad); o.start(t); o.stop(t + 1.7);
    }
    setTimeout(pluck, (2200 + Math.random() * 3600) * (1 + 1.3 * musicNight)); // sparser at night
  }
  setTimeout(pluck, 1200);
  audio = { ctx, master, filt, pad };
  setMusic(lsGet("hearth_muted") !== "1"); // honour a saved mute preference
}
// small, failure-safe localStorage helpers (private mode can throw)
const lsGet = (k) => { try { return localStorage.getItem(k); } catch (e) { return null; } };
const lsSet = (k, v) => { try { localStorage.setItem(k, v); } catch (e) { /* ignore */ } };
function setMusic(on) {
  musicOn = on;
  lsSet("hearth_muted", on ? "0" : "1");
  if (audio) audio.master.gain.setTargetAtTime(on ? 0.85 : 0.0, audio.ctx.currentTime, 0.4);
  const b = $("mute"); if (b) b.textContent = on ? "🔊" : "🔈";
}
$("mute").onclick = () => { if (!audio) startMusic(); else setMusic(!musicOn); };

function start() {
  const nick = $("nick").value.trim();
  lsSet("hearth_nick", nick); // remember the name for next time
  started = true; $("join").style.display = "none";
  startMusic();
  const img = new Image();
  img.onload = () => { atlasBase = PIXI.BaseTexture.from(img); atlasBase.scaleMode = PIXI.SCALE_MODES.NEAREST; afterAtlas(nick); };
  img.onerror = () => afterAtlas(nick);
  img.src = "/town-sprites.png"; // no cache-buster, so the browser can cache the atlas
}
$("go").onclick = start;
$("nick").addEventListener("keydown", (e) => { if (e.key === "Enter") start(); });
{ const sn = lsGet("hearth_nick"); if (sn) $("nick").value = sn; } // prefill the remembered name
$("nick").focus(); // ready to type your name on load; the chat box focuses after you join
