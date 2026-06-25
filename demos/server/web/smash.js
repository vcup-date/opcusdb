// opcusdb Smackdown, browser client. Thin client: it renders the world the Rust
// server broadcasts (PixiJS) and sends inputs. Fancy pixel fighters, a parallax
// 2-layer scrolling stage, particles, screen-shake, and Web Audio SFX.

const $ = (id) => document.getElementById(id);
const VIEW_W = 960, VIEW_H = 540, PX = 4;

// per-character palette: shirt, accent, skin, hair
const PAL = [
  { s: 0x4f8cff, a: 0xbfe0ff, k: 0xf1c27d, h: 0x26354a },
  { s: 0xff5d5d, a: 0xffc7c7, k: 0xf1c27d, h: 0x3a1414 },
  { s: 0x57d977, a: 0xc9f5d3, k: 0xf1c27d, h: 0x183a22 },
  { s: 0xffd24a, a: 0xfff0bf, k: 0xf1c27d, h: 0x4a3206 },
  { s: 0xc07dff, a: 0xe7d2ff, k: 0xf1c27d, h: 0x301433 },
  { s: 0xff9f5a, a: 0xffdbb8, k: 0xf1c27d, h: 0x3a2106 },
  { s: 0x4de1e6, a: 0xc8f8fa, k: 0xf1c27d, h: 0x06363a },
  { s: 0xf75fb4, a: 0xffccdf, k: 0xf1c27d, h: 0x3a1430 },
  { s: 0xa0e85a, a: 0xdcf7b3, k: 0xf1c27d, h: 0x2a3a10 },
  { s: 0xe2e8f0, a: 0xffffff, k: 0xf1c27d, h: 0x3a3f4a },
];

let ws = null, myId = 0;
let players = new Map();      // id -> {ch,x,y,facing,state,percent,score,name, dispX,dispY}
let stage = { w: 2400, h: 540, floor: 470, plats: [] };
let particles = [];
let shake = 0, camX = 0, started = false;

// ---- Pixi setup -----------------------------------------------------------
const app = new PIXI.Application({ width: VIEW_W, height: VIEW_H, background: 0x0a1224, antialias: false });
$("stageWrap").insertBefore(app.view, $("topbar").nextSibling);

const skyLayer = new PIXI.Container();
const farLayer = new PIXI.Container();
const nearLayer = new PIXI.Container();
const world = new PIXI.Container();   // platforms + fighters + particles (scrolls 1:1)
app.stage.addChild(skyLayer, farLayer, nearLayer, world);

const gFar = new PIXI.Graphics(); farLayer.addChild(gFar);
const gNear = new PIXI.Graphics(); nearLayer.addChild(gNear);
const gPlat = new PIXI.Graphics();
const gFighters = new PIXI.Graphics();
const gParts = new PIXI.Graphics();
world.addChild(gPlat, gParts, gFighters);

// gradient sky via offscreen canvas texture
function gradientTexture(stops) {
  const c = document.createElement("canvas"); c.width = 8; c.height = VIEW_H;
  const g = c.getContext("2d").createLinearGradient(0, 0, 0, VIEW_H);
  stops.forEach(([o, col]) => g.addColorStop(o, col));
  const cx = c.getContext("2d"); cx.fillStyle = g; cx.fillRect(0, 0, 8, VIEW_H);
  return PIXI.Texture.from(c);
}
const sky = new PIXI.Sprite(gradientTexture([[0, "#1b2a52"], [0.45, "#3a4f86"], [0.75, "#8a6db0"], [1, "#e8a07a"]]));
sky.width = VIEW_W; sky.height = VIEW_H; skyLayer.addChild(sky);

// build the parallax mountain/hill layers once (wider than the stage)
function buildBackdrop() {
  gFar.clear();
  const W = stage.w + VIEW_W;
  for (let x = -200; x < W; x += 260) {
    const h = 150 + ((x * 53) % 90);
    gFar.beginFill(0x2b3b66, 1).moveTo(x, VIEW_H).lineTo(x + 130, VIEW_H - h).lineTo(x + 260, VIEW_H).fill();
  }
  gFar.beginFill(0x223056, 0.6).drawRect(-200, VIEW_H - 60, W + 400, 60).endFill();
  gNear.clear();
  for (let x = -200; x < W; x += 200) {
    const h = 90 + ((x * 31) % 70);
    gNear.beginFill(0x16223f, 1).drawRoundedRect(x, VIEW_H - h, 230, h + 40, 60).endFill();
  }
  // a few stars/clouds high up
  for (let i = 0; i < 40; i++) {
    const x = (i * 137) % W, y = 30 + ((i * 71) % 160);
    gFar.beginFill(0xffffff, 0.5).drawRect(x, y, 2, 2).endFill();
  }
}

// ---- networking -----------------------------------------------------------
function connect(nick) {
  ws = new WebSocket(`ws://${location.host}/ws`);
  ws.onopen = () => ws.send(`join ARENA ${nick}`);
  ws.onclose = () => {};
  ws.onmessage = (e) => {
    const seen = new Set();
    for (const line of e.data.split("\n")) {
      if (!line) continue;
      const p = line.split("\t");
      if (p[0] === "w") { myId = +p[1]; }
      else if (p[0] === "g") {
        stage.w = +p[1]; stage.h = +p[2]; stage.floor = +p[3];
        stage.plats = (p[4] || "").split(";").filter(Boolean).map(s => s.split(",").map(Number));
        if (!gFar.geometry.points.length) buildBackdrop();
      } else if (p[0] === "p") {
        const id = +p[1];
        const cur = players.get(id) || { dispX: +p[3], dispY: +p[4] };
        Object.assign(cur, { ch: +p[2], x: +p[3], y: +p[4], facing: +p[5], state: +p[6], percent: +p[7], score: +p[8], atk: +p[9], name: p[10] });
        players.set(id, cur); seen.add(id);
      } else if (p[0] === "x") {
        for (const ev of (p[1] || "").split(";").filter(Boolean)) {
          const [t, x, y] = ev.split(":"); onEvent(t, +x, +y);
        }
      }
    }
    if (seen.size) for (const id of [...players.keys()]) if (!seen.has(id)) players.delete(id);
    drawHUD();
  };
}

// ---- effects: particles + sound -------------------------------------------
function onEvent(t, x, y) {
  if (t === "h") { burst(x, y, [0xffffff, 0xffe066, 0xff8a3d], 16, 7); ring(x, y, 0xfff1a8); shake = Math.min(shake + 9, 22); sfxHit(); }
  else if (t === "k") { burst(x, y, [0xff5d5d, 0xffd24a, 0xffffff, 0x7dcfff], 46, 12); shake = 26; sfxKO(); }
  else if (t === "j") { burst(x, y, [0xcfe3ff], 6, 3, 0.2); sfxJump(); }
  else if (t === "l") { burst(x, y, [0xd8c39a], 8, 3, 0.25); sfxLand(); }
  else if (t === "a") { sfxAtk(); }
}
function burst(x, y, cols, n, sp, grav = 0.35) {
  for (let i = 0; i < n; i++) {
    const ang = Math.random() * Math.PI * 2, s = sp * (0.4 + Math.random());
    particles.push({ x, y, vx: Math.cos(ang) * s, vy: Math.sin(ang) * s - 1, life: 1, decay: 0.02 + Math.random() * 0.03,
      col: cols[(Math.random() * cols.length) | 0], size: 2 + (Math.random() * 3 | 0), grav });
  }
}
function ring(x, y, col) { particles.push({ x, y, ring: 4, col, life: 1, decay: 0.08 }); }

let actx = null, muted = false;
function audio() { if (!actx) actx = new (window.AudioContext || window.webkitAudioContext)(); return actx; }
function tone(f0, f1, dur, type, vol) {
  if (muted) return; const c = audio(), o = c.createOscillator(), g = c.createGain();
  o.type = type; o.frequency.setValueAtTime(f0, c.currentTime); o.frequency.exponentialRampToValueAtTime(Math.max(1, f1), c.currentTime + dur);
  g.gain.setValueAtTime(vol, c.currentTime); g.gain.exponentialRampToValueAtTime(0.001, c.currentTime + dur);
  o.connect(g).connect(c.destination); o.start(); o.stop(c.currentTime + dur);
}
function noise(dur, vol, hp) {
  if (muted) return; const c = audio(), n = Math.floor(c.sampleRate * dur), b = c.createBuffer(1, n, c.sampleRate), d = b.getChannelData(0);
  for (let i = 0; i < n; i++) d[i] = (Math.random() * 2 - 1) * (1 - i / n);
  const s = c.createBufferSource(); s.buffer = b; const g = c.createGain(); g.gain.value = vol;
  const f = c.createBiquadFilter(); f.type = "highpass"; f.frequency.value = hp;
  s.connect(f).connect(g).connect(c.destination); s.start();
}
const sfxJump = () => tone(420, 880, 0.16, "square", 0.12);
const sfxAtk = () => noise(0.10, 0.10, 1200);
const sfxHit = () => { noise(0.14, 0.22, 500); tone(180, 60, 0.18, "sawtooth", 0.18); };
const sfxKO = () => { noise(0.4, 0.25, 300); tone(700, 80, 0.5, "sawtooth", 0.2); };
const sfxLand = () => tone(150, 80, 0.07, "square", 0.06);

// ---- pixel fighter drawing ------------------------------------------------
// origin: feet at (cx, cy); pixel units, +x right, +y down. Mirrors by facing.
function drawFighter(P) {
  const cx = P.dispX, cy = P.dispY, f = P.facing, pal = PAL[P.ch % PAL.length];
  const g = gFighters;
  const now = performance.now();
  const rect = (col, x, y, w, h, a = 1) => {
    const rx = (f > 0 ? x : -x - w);
    g.beginFill(col, a); g.drawRect(cx + rx * PX, cy + y * PX, w * PX, h * PX); g.endFill();
  };
  // shadow
  g.beginFill(0x000000, 0.28); g.drawEllipse(cx, cy + 1, 16, 5); g.endFill();

  if (P.state === 5) return; // respawning: not on stage

  const hitFlash = P.state === 4 && ((now / 60) | 0) % 2 === 0;
  const shirt = hitFlash ? 0xffffff : pal.s;
  const skin = hitFlash ? 0xffffff : pal.k;

  // pose params
  let legA = 0, legB = 0, bodyDy = 0, armFront = 0, armUp = 0, lean = 0;
  if (P.state === 1) { // run
    const ph = Math.sin(now / 70); legA = ph * 2.5; legB = -ph * 2.5; bodyDy = Math.abs(ph) < 0.3 ? -1 : 0;
  } else if (P.state === 2) { // air
    legA = 1.5; legB = -1; armUp = 2;
  } else if (P.state === 0) { // idle bob
    bodyDy = Math.sin(now / 380) < 0 ? -1 : 0;
  } else if (P.state === 4) { lean = 2; }
  const attacking = P.atk > 0;
  if (attacking) { armFront = 7; } // punch extends forward

  const base = -18; // torso/head top offset
  // legs (y from -3..0)
  rect(pal.h, -3 + legB, -3, 3, 3 + Math.max(0, -legB)); // back leg
  rect(0x2a2a33, -3 + legB, -1, 3, 2);                   // back shoe
  rect(pal.h, 1 + legA, -3, 3, 3 + Math.max(0, legA));   // front leg
  rect(0x33333d, 1 + legA, -1, 3, 2);                    // front shoe
  // torso
  rect(shirt, -4, base + 6 + bodyDy, 8, 9 + lean);
  rect(pal.a, -4, base + 6 + bodyDy, 8, 2); // chest stripe
  // back arm
  rect(skin, -5, base + 7 + bodyDy + armUp * -0.5, 2, 6 - armUp);
  // head
  rect(skin, -3, base + bodyDy, 6, 6);
  rect(pal.h, -3, base - 1 + bodyDy, 6, 2);       // hair
  rect(0x101018, 1, base + 2 + bodyDy, 2, 2);     // eye/visor (front)
  // front arm (extends on attack)
  if (attacking) {
    rect(skin, 3, base + 6 + bodyDy, 3 + armFront, 3);   // extended arm
    // fist + slash
    rect(0xffffff, 3 + armFront, base + 5 + bodyDy, 3, 4);
    const sx = cx + (f > 0 ? (6 + armFront) : -(6 + armFront)) * PX;
    const sy = cy + (base + 7 + bodyDy) * PX;
    g.lineStyle(3, 0xffffff, 0.85); g.arc(sx, sy, 22, f > 0 ? -0.9 : Math.PI + 0.9, f > 0 ? 0.9 : Math.PI - 0.9); g.lineStyle(0);
  } else {
    rect(skin, 3, base + 7 + bodyDy, 2, 6 - armUp);
  }

  // name tag + percent above head
  const txt = nameTag(P);
  txt.x = cx; txt.y = cy + (base - 8) * PX;
}

// cached PIXI.Text per player for crisp name tags
const tags = new Map();
function nameTag(P) {
  let t = tags.get(P.id);
  if (!t) {
    t = new PIXI.Text("", { fontFamily: "monospace", fontSize: 12, fontWeight: "700", fill: 0xffffff, stroke: 0x000000, strokeThickness: 3, align: "center" });
    t.anchor.set(0.5, 1); world.addChild(t); tags.set(P.id, t);
  }
  t.text = P.name + (P.id === myId ? " (you)" : "");
  t.style.fill = P.id === myId ? 0x9fe0ff : 0xffffff;
  t.visible = P.state !== 5;
  return t;
}

// ---- main loop ------------------------------------------------------------
app.ticker.add(() => {
  // interpolate displayed positions
  for (const [id, p] of players) {
    p.id = id;
    p.dispX += (p.x - p.dispX) * 0.4;
    p.dispY += (p.y - p.dispY) * 0.4;
  }
  // camera follows local fighter (or stage centre)
  const me = players.get(myId);
  const tx = me ? me.dispX : stage.w / 2;
  camX += ((Math.max(VIEW_W / 2, Math.min(stage.w - VIEW_W / 2, tx)) - VIEW_W / 2) - camX) * 0.12;
  const sh = shake > 0.2 ? (Math.random() - 0.5) * shake : 0;
  const shy = shake > 0.2 ? (Math.random() - 0.5) * shake : 0;
  shake *= 0.86;
  world.x = -camX + sh; world.y = shy;
  farLayer.x = -camX * 0.3; nearLayer.x = -camX * 0.55;

  // platforms: index 0 is the solid ground slab; the rest are thin pass-through
  // ledges (you land on top, jump up through from below, no side wall to clip).
  gPlat.clear();
  stage.plats.forEach(([x0, x1, top], i) => {
    if (i === 0) {
      gPlat.beginFill(0x222c45).drawRect(x0, top, x1 - x0, stage.h - top + 80).endFill();
      gPlat.beginFill(0x35502f).drawRect(x0, top, x1 - x0, 9).endFill();        // earth/grass
      gPlat.beginFill(0x6ee27a, 0.7).drawRect(x0, top, x1 - x0, 2).endFill();   // grass edge
    } else {
      const h = 16;
      gPlat.beginFill(0x000000, 0.22).drawRoundedRect(x0 + 5, top + 6, x1 - x0 - 10, h, 7).endFill(); // drop shadow
      gPlat.beginFill(0x2a3550).drawRoundedRect(x0, top, x1 - x0, h, 7).endFill();
      gPlat.beginFill(0x4de1e6, 0.8).drawRect(x0 + 4, top, x1 - x0 - 8, 2).endFill();                 // neon lip
    }
  });

  // fighters
  gFighters.clear();
  const seenTags = new Set();
  for (const p of players.values()) { drawFighter(p); seenTags.add(p.id); }
  for (const [id, t] of tags) if (!seenTags.has(id)) { t.destroy(); tags.delete(id); }

  // particles
  gParts.clear();
  for (const pt of particles) {
    if (pt.ring !== undefined) {
      pt.ring += 6; pt.life -= pt.decay;
      gParts.lineStyle(3, pt.col, Math.max(0, pt.life)); gParts.drawCircle(pt.x, pt.y, pt.ring); gParts.lineStyle(0);
    } else {
      pt.vy += pt.grav; pt.x += pt.vx; pt.y += pt.vy; pt.life -= pt.decay;
      gParts.beginFill(pt.col, Math.max(0, pt.life)).drawRect(pt.x, pt.y, pt.size, pt.size).endFill();
    }
  }
  particles = particles.filter(p => p.life > 0);
});

// ---- HUD ------------------------------------------------------------------
function drawHUD() {
  const hud = $("hud");
  const ids = [...players.keys()].sort((a, b) => (players.get(b).score - players.get(a).score));
  hud.innerHTML = "";
  for (const id of ids) {
    const p = players.get(id), pal = PAL[p.ch % PAL.length];
    const pct = p.percent;
    const col = pct < 60 ? "#dfe7ff" : pct < 110 ? "#ffd24a" : pct < 160 ? "#ff7a3d" : "#ff4d4d";
    const card = document.createElement("div"); card.className = "pcard";
    card.style.borderColor = "#" + pal.s.toString(16).padStart(6, "0");
    card.innerHTML = `<div class="nm" style="color:#${pal.s.toString(16).padStart(6,'0')}">${esc(p.name)}${id===myId?" ★":""}</div>
      <div class="pct" style="color:${col}">${pct|0}%</div><div class="ko">KO ${p.score}</div>`;
    hud.appendChild(card);
  }
}
const esc = (s) => (s || "").replace(/[&<>]/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" }[c]));

// ---- input ----------------------------------------------------------------
let kl = false, kr = false;
function sendKeys() { if (ws && ws.readyState === 1) ws.send(`keys ${kl ? 1 : 0} ${kr ? 1 : 0}`); }
addEventListener("keydown", (e) => {
  if (!started) return;
  let used = true;
  if (e.key === "ArrowLeft") { if (!kl) { kl = true; sendKeys(); } }
  else if (e.key === "ArrowRight") { if (!kr) { kr = true; sendKeys(); } }
  else if (e.key === "x" || e.key === "X" || e.key === "ArrowUp") { if (!e.repeat && ws) ws.send("jump"); }
  else if (e.key === "z" || e.key === "Z") { if (!e.repeat && ws) ws.send("atk"); }
  else used = false;
  if (used) e.preventDefault();
});
addEventListener("keyup", (e) => {
  if (e.key === "ArrowLeft") { kl = false; sendKeys(); }
  else if (e.key === "ArrowRight") { kr = false; sendKeys(); }
});

// ---- start ----------------------------------------------------------------
$("sound").onclick = () => { muted = !muted; $("sound").textContent = muted ? "🔇 sound off" : "🔊 sound on"; if (!muted) audio().resume(); };
function start() {
  started = true;
  $("overlay").style.display = "none";
  audio(); // unlock on the user gesture
  const nick = $("nick").value.trim() || ("P" + (Math.random() * 900 + 100 | 0));
  connect(nick);
}
$("start").onclick = start;
$("nick").addEventListener("keydown", (e) => { if (e.key === "Enter") start(); });
