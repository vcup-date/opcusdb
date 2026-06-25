// opcusdb Co-Board — CRDT whiteboard client. Strokes form an OrSet on the server
// (opcusdb-algebra); concurrent draws all survive and offline edits merge on
// reconnect. This client draws locally for instant feedback, sends ops, buffers
// them while "offline", and flushes + resyncs when it reconnects.

const $ = (id) => document.getElementById(id);
const cv = $("board"), ctx = cv.getContext("2d");

let ws = null, myId = 0, name = "", color = "#ff5d5d", offline = false, started = false;
let seq = 0;
const strokes = new Map();   // id -> {color, width, pts:[[x,y]...]} (normalized 0..1)
const pending = [];          // ops buffered while offline
let presence = [];           // [{id,x,y,color,name}]
let drawing = null;          // current stroke being drawn
let eraser = false;

function resize(){ cv.width = innerWidth; cv.height = innerHeight - 52; redraw(); }
addEventListener("resize", resize);

// ---- networking -----------------------------------------------------------
function connect() {
  ws = new WebSocket(`ws://${location.host}/ws`);
  ws.onopen = () => {
    ws.send(`join ${name} ${color}`);
    // flush anything drawn while offline -> server merges it (CRDT)
    for (const op of pending.splice(0)) ws.send(op);
  };
  ws.onclose = () => {};
  ws.onmessage = (e) => {
    for (const line of e.data.split("\n")) {
      if (!line) continue;
      const i = line.indexOf("\t");
      const tag = i < 0 ? line : line.slice(0, i);
      const rest = i < 0 ? "" : line.slice(i + 1);
      if (tag === "w") myId = +rest;
      else if (tag === "d") { const j = rest.indexOf("\t"); upsert(rest.slice(0, j), rest.slice(j + 1)); }
      else if (tag === "e") strokes.delete(rest);
      else if (tag === "x") strokes.clear();
      else if (tag === "p") parsePresence(rest);
    }
    redraw();
    renderPresence();
  };
}
function send(op){ if (offline || !ws || ws.readyState !== 1) pending.push(op); else ws.send(op); }

function upsert(id, payload){
  // payload = "color width x,y;x,y;..."
  const sp = payload.indexOf(" "), sp2 = payload.indexOf(" ", sp + 1);
  const col = payload.slice(0, sp), w = +payload.slice(sp + 1, sp2);
  const pts = payload.slice(sp2 + 1).split(";").filter(Boolean).map(p => p.split(",").map(Number));
  strokes.set(id, { color: col, width: w, pts });
}
function parsePresence(rest){
  presence = (rest || "").split(";").filter(Boolean).map(s => {
    const a = s.split(":"); return { id:+a[0], x:+a[1], y:+a[2], color:a[3], name:a.slice(4).join(":") };
  });
}

// ---- drawing --------------------------------------------------------------
function pos(e){ const r = cv.getBoundingClientRect(); return [(e.clientX-r.left)/r.width, (e.clientY-r.top)/r.height]; }
cv.addEventListener("mousedown", (e) => {
  if (!started) return;
  const p = pos(e);
  if (eraser) { eraseAt(p); return; }
  drawing = { color, width: +$("size").value, pts: [p] };
});
cv.addEventListener("mousemove", (e) => {
  if (!started) return;
  const p = pos(e);
  if (ws && ws.readyState === 1 && !offline) ws.send(`cursor ${p[0].toFixed(3)} ${p[1].toFixed(3)}`);
  if (eraser && e.buttons) { eraseAt(p); return; }
  if (drawing) { drawing.pts.push(p); redraw(); }
});
addEventListener("mouseup", () => {
  if (!drawing) return;
  if (drawing.pts.length >= 1) {
    const id = `${myId||0}:${++seq}`;
    strokes.set(id, drawing);
    const pts = drawing.pts.map(p => `${p[0].toFixed(3)},${p[1].toFixed(3)}`).join(";");
    send(`draw ${id} ${drawing.color} ${drawing.width} ${pts}`);
  }
  drawing = null; redraw();
});
function eraseAt(p){
  for (const [id, s] of [...strokes].reverse()) {
    if (s.pts.some(q => Math.hypot((q[0]-p[0])*cv.width, (q[1]-p[1])*cv.height) < (s.width+8))) {
      strokes.delete(id); send(`erase ${id}`); redraw(); break;
    }
  }
}

function drawStroke(s){
  if (!s.pts.length) return;
  ctx.strokeStyle = s.color; ctx.lineWidth = s.width; ctx.lineCap = "round"; ctx.lineJoin = "round";
  ctx.beginPath();
  ctx.moveTo(s.pts[0][0]*cv.width, s.pts[0][1]*cv.height);
  for (let i=1;i<s.pts.length;i++) ctx.lineTo(s.pts[i][0]*cv.width, s.pts[i][1]*cv.height);
  if (s.pts.length===1) ctx.lineTo(s.pts[0][0]*cv.width+0.1, s.pts[0][1]*cv.height);
  ctx.stroke();
}
function redraw(){
  ctx.clearRect(0,0,cv.width,cv.height);
  for (const s of strokes.values()) drawStroke(s);
  if (drawing) drawStroke(drawing);
}
function renderPresence(){
  // toolbar user list
  $("users").innerHTML = presence.map(u => `<span class="u"><span class="dot" style="background:${esc(u.color)}"></span>${esc(u.name)}${u.id===myId?" (you)":""}</span>`).join("");
  // floating cursors
  const box = $("cursors");
  box.innerHTML = presence.filter(u => u.id!==myId && u.x>=0).map(u =>
    `<div class="cur" style="left:${u.x*cv.width}px;top:${u.y*cv.height}px"><b style="background:${esc(u.color)}"></b><span>${esc(u.name)}</span></div>`).join("");
}
const esc = (s)=>(s||"").replace(/[&<>"]/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;"}[c]));

// ---- toolbar --------------------------------------------------------------
$("color").oninput = () => color = $("color").value;
$("eraser").onclick = () => { eraser = !eraser; $("eraser").classList.toggle("on", eraser); cv.style.cursor = eraser?"cell":"crosshair"; };
$("clear").onclick = () => { strokes.clear(); redraw(); send("clear"); };
$("offline").onclick = () => {
  offline = !offline;
  const b = $("offline");
  if (offline) { b.textContent = "go online"; b.classList.remove("off"); b.classList.add("on"); $("offbanner").style.display="block"; if (ws) ws.close(); }
  else { b.textContent = "go offline"; b.classList.add("off"); b.classList.remove("on"); $("offbanner").style.display="none"; connect(); }
};

// ---- start ----------------------------------------------------------------
$("nick").value = "artist" + (Math.random()*900+100|0);
function start(){ name = $("nick").value.trim() || "artist"; color = $("color").value; started = true; $("join").style.display="none"; resize(); connect(); }
$("go").onclick = start;
$("nick").addEventListener("keydown", e => { if (e.key==="Enter") start(); });
resize();
