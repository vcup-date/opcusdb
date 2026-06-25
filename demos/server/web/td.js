// opcusdb Rampart — tower-defense client. The Rust server is the authoritative
// game (creeps, towers, projectiles, waves, gold/lives); this renders state and
// sends two commands: build a tower, start a wave. Co-op: open more tabs.

const $ = (id) => document.getElementById(id);
const cv = $("cv"), ctx = cv.getContext("2d");

let ws = null;
let COLS = 20, ROWS = 12, TILE = 48;
let road = new Set();         // "c,r"
let base = [19, 10];
let enemies = [], towers = [], projs = [];
let gold = 0, lives = 0, wave = 0, maxWave = 12, state = 0;
let pick = 0;                 // selected tower kind
let hover = null;             // [c,r]

const COST = [50, 110, 75], RANGE = [120, 135, 105];
const TCOL = ["#6fb0ff", "#9a6bff", "#49d6ff"];
const ECOL = ["#ff5d5d", "#ffd24a", "#b87bff"]; // normal, fast, tank

function connect() {
  ws = new WebSocket(`ws://${location.host}/ws`);
  ws.onmessage = (e) => {
    for (const line of e.data.split("\n")) {
      if (!line) continue;
      const i = line.indexOf("\t"), tag = i < 0 ? line : line.slice(0, i), rest = i < 0 ? "" : line.slice(i + 1);
      if (tag === "map") {
        const p = rest.split("\t"); COLS = +p[0]; ROWS = +p[1]; TILE = +p[2];
        road = new Set((p[3] || "").split(";").filter(Boolean));
        base = (p[4] || "19,10").split(",").map(Number);
        cv.width = COLS * TILE; cv.height = ROWS * TILE;
      } else if (tag === "s") {
        const p = rest.split("\t"); gold = +p[0]; lives = +p[1]; wave = +p[2]; maxWave = +p[3]; state = +p[4];
        updateHud();
      } else if (tag === "e") {
        enemies = rest.split(";").filter(Boolean).map(s => { const a = s.split(","); return { x:+a[0], y:+a[1], hp:+a[2], kind:+a[3], slow:+a[4] }; });
      } else if (tag === "t") {
        towers = rest.split(";").filter(Boolean).map(s => { const a = s.split(","); return { c:+a[0], r:+a[1], kind:+a[2] }; });
      } else if (tag === "p") {
        projs = rest.split(";").filter(Boolean).map(s => { const a = s.split(","); return { x:+a[0], y:+a[1], kind:+a[2] }; });
      }
    }
  };
}

function updateHud() {
  $("gold").innerHTML = gold + '<small>GOLD</small>';
  $("lives").innerHTML = lives + '<small>LIVES</small>';
  $("wavn").innerHTML = wave + '/' + maxWave + '<small>WAVE</small>';
  const wb = $("wave");
  if (state === 0 && wave < maxWave) { wb.className = "ready"; wb.textContent = "▶ Start Wave " + (wave + 1); wb.disabled = false; }
  else if (state === 1) { wb.className = ""; wb.textContent = "Wave " + wave + " in progress…"; wb.disabled = true; }
  else { wb.className = ""; wb.textContent = "—"; wb.disabled = true; }
  $("over").style.display = (state >= 2) ? "flex" : "none";
  if (state >= 2) { $("otxt").textContent = state === 2 ? "VICTORY 🏆" : "DEFEAT"; $("otxt").style.color = state === 2 ? "#3ec46a" : "#ff5d5d"; }
  // palette affordability
  document.querySelectorAll(".tw").forEach(el => el.classList.toggle("poor", gold < COST[+el.dataset.k]));
}

// ---- render ---------------------------------------------------------------
function loop() {
  requestAnimationFrame(loop);
  if (!cv.width) return;
  // grass + checker
  for (let r = 0; r < ROWS; r++) for (let c = 0; c < COLS; c++) {
    ctx.fillStyle = (c + r) % 2 ? "#15351c" : "#163a1f"; ctx.fillRect(c*TILE, r*TILE, TILE, TILE);
  }
  // road
  for (const k of road) { const [c, r] = k.split(",").map(Number); ctx.fillStyle = "#5b4a33"; ctx.fillRect(c*TILE, r*TILE, TILE, TILE);
    ctx.fillStyle = "#4a3c29"; ctx.fillRect(c*TILE+TILE*0.5-2, r*TILE+6, 4, TILE-12); }
  // keep (base)
  { const [c, r] = base; const x = c*TILE, y = r*TILE; ctx.fillStyle = "#8a8f9c"; ctx.fillRect(x+6, y+6, TILE-12, TILE-12);
    ctx.fillStyle = "#c0c6d2"; for (let i=0;i<3;i++) ctx.fillRect(x+8+i*(TILE-16)/3, y+2, (TILE-16)/4, 8);
    ctx.fillStyle = "#ffc24a"; ctx.font = "bold 20px system-ui"; ctx.textAlign="center"; ctx.textBaseline="middle"; ctx.fillText("♜", x+TILE/2, y+TILE/2+2); }

  // hover preview + range
  if (hover && state < 2) {
    const [c, r] = hover; const key = c+","+r;
    const ok = !road.has(key) && !towers.some(t => t.c===c && t.r===r) && gold >= COST[pick] && c>=0 && c<COLS && r>=0 && r<ROWS;
    ctx.fillStyle = ok ? "rgba(110,230,140,0.30)" : "rgba(255,90,90,0.25)";
    ctx.fillRect(c*TILE, r*TILE, TILE, TILE);
    ctx.strokeStyle = ok ? "rgba(160,255,180,0.6)" : "rgba(255,120,120,0.6)"; ctx.lineWidth=2; ctx.strokeRect(c*TILE+1, r*TILE+1, TILE-2, TILE-2);
    ctx.beginPath(); ctx.arc(c*TILE+TILE/2, r*TILE+TILE/2, RANGE[pick], 0, 7); ctx.strokeStyle="rgba(255,255,255,0.18)"; ctx.lineWidth=1.5; ctx.stroke();
  }

  // towers
  for (const t of towers) { const x = t.c*TILE+TILE/2, y = t.r*TILE+TILE/2;
    ctx.fillStyle = "#2b3346"; ctx.beginPath(); ctx.arc(x, y, TILE*0.40, 0, 7); ctx.fill();
    ctx.fillStyle = TCOL[t.kind]; ctx.beginPath(); ctx.arc(x, y, TILE*0.27, 0, 7); ctx.fill();
    ctx.strokeStyle = "#0008"; ctx.lineWidth=2; ctx.stroke();
  }
  // projectiles
  for (const p of projs) { ctx.fillStyle = p.kind===2 ? "#9beaff" : (p.kind===1 ? "#222" : "#ffe9a8");
    ctx.beginPath(); ctx.arc(p.x, p.y, p.kind===1?5:3, 0, 7); ctx.fill(); }
  // enemies
  for (const e of enemies) {
    const rad = e.kind===2 ? 13 : (e.kind===1 ? 7 : 9);
    ctx.fillStyle = ECOL[e.kind]; ctx.beginPath(); ctx.arc(e.x, e.y, rad, 0, 7); ctx.fill();
    ctx.strokeStyle = e.slow ? "#9beaff" : "#0007"; ctx.lineWidth = e.slow?2.5:1.5; ctx.stroke();
    // hp bar
    const w = rad*2, f = e.hp/10; ctx.fillStyle="#000a"; ctx.fillRect(e.x-w/2, e.y-rad-7, w, 4);
    ctx.fillStyle = f>0.5?"#3ec46a":(f>0.25?"#ffd24a":"#ff5d5d"); ctx.fillRect(e.x-w/2, e.y-rad-7, w*f, 4);
  }
}
requestAnimationFrame(loop);

// ---- input ----------------------------------------------------------------
function tileAt(ev) { const r = cv.getBoundingClientRect(); const sx = cv.width/r.width, sy = cv.height/r.height;
  return [Math.floor((ev.clientX-r.left)*sx/TILE), Math.floor((ev.clientY-r.top)*sy/TILE)]; }
cv.addEventListener("mousemove", (ev) => hover = tileAt(ev));
cv.addEventListener("mouseleave", () => hover = null);
cv.addEventListener("click", (ev) => { const [c, r] = tileAt(ev); ws && ws.readyState===1 && ws.send(`place ${pick} ${c} ${r}`); });
document.querySelectorAll(".tw").forEach(el => el.onclick = () => { pick = +el.dataset.k;
  document.querySelectorAll(".tw").forEach(x => x.classList.toggle("sel", x===el)); });
$("wave").onclick = () => ws && ws.readyState===1 && ws.send("wave");
$("again").onclick = () => ws && ws.readyState===1 && ws.send("reset");
addEventListener("keydown", (e) => { if (e.key>="1"&&e.key<="3"){ pick=+e.key-1; document.querySelectorAll(".tw").forEach(x=>x.classList.toggle("sel",+x.dataset.k===pick)); }
  if (e.key===" "){ e.preventDefault(); ws&&ws.readyState===1&&ws.send("wave"); } });

connect();
