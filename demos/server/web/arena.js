// Browser client for opcusdb Arena (multiplayer snake). Thin client: it sends
// steering inputs and renders the authoritative state the Rust server broadcasts.

const $ = (id) => document.getElementById(id);
const COLORS = [0x7dcfff, 0xffd24a, 0x6ee7b7, 0xff7eb6, 0xc4a7ff, 0xff9f5a];
const CSS = ["#7dcfff", "#ffd24a", "#6ee7b7", "#ff7eb6", "#c4a7ff", "#ff9f5a"];

let ws = null, myId = 0, grid = 32, cell = 20;
let snakes = [], food = [], leaderboard = [];

const app = new PIXI.Application({ width: 640, height: 640, background: 0x070b14, antialias: true });
$("stage").replaceWith(app.view);
app.view.id = "stage";
const gfx = new PIXI.Graphics();
app.stage.addChild(gfx);

function sizeBoard() {
  cell = Math.floor(620 / grid);
  const px = cell * grid;
  app.renderer.resize(px, px);
}

// --- networking ------------------------------------------------------------
function connect(nick, code) {
  ws = new WebSocket(`ws://${location.host}/ws`);
  ws.onopen = () => { ws.send(`join ${code} ${nick}`); $("lobby").style.display = "none"; $("roomTag").textContent = code; };
  ws.onclose = () => { $("hud").textContent = "disconnected"; };
  ws.onmessage = (e) => parse(e.data);
}

function parse(data) {
  const newSnakes = [];
  for (const line of data.split("\n")) {
    if (!line) continue;
    const p = line.split("\t");
    if (p[0] === "w") { myId = +p[1]; }
    else if (p[0] === "g") { grid = +p[1]; sizeBoard(); }
    else if (p[0] === "f") { food = p[1] ? p[1].split(";").map(c => c.split(",").map(Number)) : []; }
    else if (p[0] === "s") {
      newSnakes.push({
        id: +p[1], color: +p[2], alive: p[3] === "1", score: +p[4], nick: p[5],
        body: p[6] ? p[6].split(";").map(c => c.split(",").map(Number)) : [],
      });
    } else if (p[0] === "l") {
      leaderboard = (p[1] || "").split(",").filter(Boolean).map(x => { const [n, s] = x.split(":"); return [n, +s]; });
    }
  }
  if (data.includes("\ns\t") || data.startsWith("s\t")) snakes = newSnakes;
  renderSidebar();
}

// --- rendering -------------------------------------------------------------
app.ticker.add(() => {
  gfx.clear();
  // grid
  gfx.lineStyle(1, 0x121d33, 0.8);
  for (let i = 0; i <= grid; i++) {
    gfx.moveTo(i * cell, 0).lineTo(i * cell, grid * cell);
    gfx.moveTo(0, i * cell).lineTo(grid * cell, i * cell);
  }
  gfx.lineStyle(0);
  // food (pulsing)
  const pulse = 0.5 + 0.5 * Math.sin(performance.now() / 200);
  for (const [x, y] of food) {
    gfx.beginFill(0xff5577, 0.9).drawCircle(x * cell + cell / 2, y * cell + cell / 2, cell * (0.28 + 0.08 * pulse)).endFill();
  }
  // snakes
  let meDead = false;
  for (const s of snakes) {
    const col = COLORS[s.color % COLORS.length];
    const mine = s.id === myId;
    if (mine && !s.alive) meDead = true;
    const a = s.alive ? 1 : 0.25;
    s.body.forEach(([x, y], i) => {
      const head = i === 0;
      const pad = head ? 1 : 2;
      gfx.beginFill(col, a * (head ? 1 : 0.85));
      gfx.drawRoundedRect(x * cell + pad, y * cell + pad, cell - pad * 2, cell - pad * 2, 4);
      gfx.endFill();
      if (mine && head) { gfx.lineStyle(2, 0xffffff, 0.9).drawRoundedRect(x*cell+pad, y*cell+pad, cell-pad*2, cell-pad*2, 4); gfx.lineStyle(0); }
    });
  }
  $("over").style.display = meDead ? "flex" : "none";
  const me = snakes.find(s => s.id === myId);
  $("hud").textContent = me ? `your score: ${me.score}` : ", ";
});

function renderSidebar() {
  const ps = $("players");
  ps.innerHTML = "";
  [...snakes].sort((a, b) => b.score - a.score).forEach(s => {
    const d = document.createElement("div");
    d.className = "row" + (s.id === myId ? " me" : "");
    d.innerHTML = `<span><span class="dot" style="background:${CSS[s.color % 6]}"></span>${escapeHtml(s.nick)}${s.id===myId?" (you)":""}</span><span>${s.score}</span>`;
    ps.appendChild(d);
  });
  const lb = $("leaderboard");
  lb.innerHTML = leaderboard.length ? "" : '<div style="color:#6b7a99">no scores yet, be the first!</div>';
  leaderboard.forEach(([n, s], i) => {
    const d = document.createElement("div");
    d.className = "row";
    d.innerHTML = `<span>${i + 1}. ${escapeHtml(n)}</span><span>${s}</span>`;
    lb.appendChild(d);
  });
}
const escapeHtml = (s) => s.replace(/[&<>]/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" }[c]));

// --- input -----------------------------------------------------------------
const KEYS = { ArrowUp: "u", ArrowDown: "d", ArrowLeft: "l", ArrowRight: "r", w: "u", s: "d", a: "l", d: "r", W: "u", S: "d", A: "l", D: "r" };
addEventListener("keydown", (e) => {
  const dir = KEYS[e.key];
  if (dir && ws && ws.readyState === 1) { ws.send("dir " + dir); e.preventDefault(); }
});

// --- lobby -----------------------------------------------------------------
const rndCode = () => Array.from({ length: 4 }, () => "ABCDEFGHJKLMNPQRSTUVWXYZ23456789"[Math.floor(Math.random() * 32)]).join("");
$("nick").value = "player" + Math.floor(Math.random() * 1000);
$("create").onclick = () => { $("code").value = rndCode(); connect(val("nick"), $("code").value); };
$("join").onclick = () => { if (!$("code").value.trim()) $("code").value = rndCode(); connect(val("nick"), $("code").value.trim()); };
const val = (id) => $(id).value.trim() || $(id).placeholder;
// support ?room=CODE for share links
const q = new URLSearchParams(location.search).get("room");
if (q) $("code").value = q.toUpperCase();
