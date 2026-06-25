// opcusdb Co-Board, collaborative vector canvas. Elements (rect/ellipse/line/
// arrow/text/note/pen) live in an OrSet CRDT on the server; editing an element is
// an upsert (same id). Selection + resize handles, live presence, offline-merge.

const $ = (id) => document.getElementById(id);
const cv = $("cv"), ctx = cv.getContext("2d");
const LW = 1600, LH = 1000; // logical artboard size

let ws = null, myId = 0, name = "", offline = false, started = false, seq = 0;
const els = new Map();        // id -> element (insertion order = z-order)
const pending = [];           // ops buffered while offline
let presence = [];
let tool = "select";
let style = { stroke: "#5b9dff", fill: null, sw: 4 };
let sel = null;               // selected id
let drag = null;              // { mode, id, sx, sy, orig, handle }
let lastSent = 0;

// ---- camera (fit artboard centred) ----------------------------------------
let cam = { s: 1, ox: 0, oy: 0 };
function resize() {
  cv.width = innerWidth; cv.height = innerHeight - 48;
  cam.s = Math.min(cv.width / LW, cv.height / LH) * 0.92;
  cam.ox = (cv.width - LW * cam.s) / 2;
  cam.oy = (cv.height - LH * cam.s) / 2;
  redraw();
}
addEventListener("resize", resize);
const toScreen = (x, y) => [cam.ox + x * cam.s, cam.oy + y * cam.s];
const toLogic = (sx, sy) => [(sx - cam.ox) / cam.s, (sy - cam.oy) / cam.s];
function evPos(e) { const r = cv.getBoundingClientRect(); return toLogic(e.clientX - r.left, e.clientY - r.top); }

// ---- networking -----------------------------------------------------------
function connect() {
  ws = new WebSocket(`ws://${location.host}/ws`);
  ws.onopen = () => { ws.send(`join ${name} ${style.stroke}`); for (const op of pending.splice(0)) ws.send(op); };
  ws.onmessage = (e) => {
    for (const line of e.data.split("\n")) {
      if (!line) continue;
      const i = line.indexOf("\t"), tag = i < 0 ? line : line.slice(0, i), rest = i < 0 ? "" : line.slice(i + 1);
      if (tag === "w") myId = +rest;
      else if (tag === "d") { const j = rest.indexOf("\t"); try { els.set(rest.slice(0, j), JSON.parse(rest.slice(j + 1))); } catch (_) {} }
      else if (tag === "e") { els.delete(rest); if (sel === rest) selectEl(null); }
      else if (tag === "x") { els.clear(); selectEl(null); }
      else if (tag === "p") parsePresence(rest);
    }
    redraw(); renderPresence();
  };
}
function send(op) { if (offline || !ws || ws.readyState !== 1) pending.push(op); else ws.send(op); }
function push(el, throttle) {
  els.set(el.id, el);
  const op = `draw ${el.id} ${JSON.stringify(el)}`;
  if (throttle) { const now = performance.now(); if (now - lastSent < 40) return; lastSent = now; }
  send(op);
}
function erase(id) { els.delete(id); if (sel === id) selectEl(null); send(`erase ${id}`); }
function parsePresence(rest) {
  presence = (rest || "").split(";").filter(Boolean).map(s => { const a = s.split(":"); return { id: +a[0], x: +a[1], y: +a[2], color: a[3], name: a.slice(4).join(":") }; });
}

// ---- element geometry helpers ---------------------------------------------
function bbox(el) {
  if (el.t === "line" || el.t === "arrow") return { x: Math.min(el.x, el.x2), y: Math.min(el.y, el.y2), w: Math.abs(el.x2 - el.x), h: Math.abs(el.y2 - el.y) };
  if (el.t === "pen") { let xs = el.pts.map(p => p[0]), ys = el.pts.map(p => p[1]); const x = Math.min(...xs), y = Math.min(...ys); return { x, y, w: Math.max(...xs) - x, h: Math.max(...ys) - y }; }
  if (el.t === "text") return { x: el.x, y: el.y - el.size, w: (el.text.length || 1) * el.size * 0.6, h: el.size * 1.3 };
  return { x: Math.min(el.x, el.x + el.w), y: Math.min(el.y, el.y + el.h), w: Math.abs(el.w), h: Math.abs(el.h) };
}
function hit(el, x, y) {
  const b = bbox(el), m = 8 / cam.s;
  if (el.t === "line" || el.t === "arrow") { return distToSeg(x, y, el.x, el.y, el.x2, el.y2) < (el.sw + 8) / cam.s; }
  if (el.t === "pen") { for (let i = 1; i < el.pts.length; i++) if (distToSeg(x, y, el.pts[i-1][0], el.pts[i-1][1], el.pts[i][0], el.pts[i][1]) < (el.sw + 8) / cam.s) return true; return false; }
  return x >= b.x - m && x <= b.x + b.w + m && y >= b.y - m && y <= b.y + b.h + m;
}
function distToSeg(px, py, x1, y1, x2, y2) { const dx = x2-x1, dy = y2-y1, l2 = dx*dx+dy*dy; let t = l2 ? ((px-x1)*dx+(py-y1)*dy)/l2 : 0; t = Math.max(0, Math.min(1, t)); return Math.hypot(px-(x1+t*dx), py-(y1+t*dy)); }
// resize handles for the selected element (logical coords)
function handles(el) {
  if (el.t === "line" || el.t === "arrow") return [{ k: "p1", x: el.x, y: el.y }, { k: "p2", x: el.x2, y: el.y2 }];
  const b = bbox(el);
  return [["nw",0,0],["n",.5,0],["ne",1,0],["e",1,.5],["se",1,1],["s",.5,1],["sw",0,1],["w",0,.5]].map(([k,fx,fy]) => ({ k, x: b.x + b.w*fx, y: b.y + b.h*fy }));
}

// ---- rendering ------------------------------------------------------------
function redraw() {
  ctx.clearRect(0, 0, cv.width, cv.height);
  // artboard
  const [ax, ay] = toScreen(0, 0);
  ctx.fillStyle = "#0f1422"; ctx.fillRect(ax, ay, LW*cam.s, LH*cam.s);
  ctx.strokeStyle = "#243049"; ctx.lineWidth = 1.5; ctx.strokeRect(ax, ay, LW*cam.s, LH*cam.s);
  ctx.save(); ctx.beginPath(); ctx.rect(ax, ay, LW*cam.s, LH*cam.s); ctx.clip();
  // grid
  ctx.strokeStyle = "#161d2c"; ctx.lineWidth = 1; ctx.beginPath();
  for (let gx = 0; gx <= LW; gx += 40) { const [sx] = toScreen(gx, 0); ctx.moveTo(sx, ay); ctx.lineTo(sx, ay+LH*cam.s); }
  for (let gy = 0; gy <= LH; gy += 40) { const [, sy] = toScreen(0, gy); ctx.moveTo(ax, sy); ctx.lineTo(ax+LW*cam.s, sy); }
  ctx.stroke();
  for (const el of els.values()) drawEl(el);
  if (drag && drag.preview) drawEl(drag.preview);
  ctx.restore();
  if (sel && els.has(sel)) drawSelection(els.get(sel));
}
function drawEl(el) {
  ctx.lineWidth = (el.sw || 2) * cam.s; ctx.lineCap = "round"; ctx.lineJoin = "round"; ctx.strokeStyle = el.stroke || "#fff";
  if (el.t === "rect" || el.t === "note") {
    const [x, y] = toScreen(Math.min(el.x, el.x+el.w), Math.min(el.y, el.y+el.h)); const w = Math.abs(el.w)*cam.s, h = Math.abs(el.h)*cam.s;
    if (el.t === "note") { ctx.fillStyle = el.fill || "#ffd86b"; roundRect(x, y, w, h, 6*cam.s); ctx.fill(); }
    else if (el.fill) { ctx.fillStyle = el.fill; roundRect(x, y, w, h, 6*cam.s); ctx.fill(); }
    if (el.t !== "note" || true) { roundRect(x, y, w, h, 6*cam.s); ctx.stroke(); }
    if (el.t === "note" && el.text) { drawText(el.text, el.x+10, el.y+24, 17, "#3a2c00", Math.abs(el.w)-20); }
  } else if (el.t === "ellipse") {
    const cx2 = el.x + el.w/2, cy2 = el.y + el.h/2; const [sx, sy] = toScreen(cx2, cy2);
    ctx.beginPath(); ctx.ellipse(sx, sy, Math.abs(el.w)/2*cam.s, Math.abs(el.h)/2*cam.s, 0, 0, 7);
    if (el.fill) { ctx.fillStyle = el.fill; ctx.fill(); } ctx.stroke();
  } else if (el.t === "line" || el.t === "arrow") {
    const [x1, y1] = toScreen(el.x, el.y), [x2, y2] = toScreen(el.x2, el.y2);
    ctx.beginPath(); ctx.moveTo(x1, y1); ctx.lineTo(x2, y2); ctx.stroke();
    if (el.t === "arrow") { const a = Math.atan2(y2-y1, x2-x1), h = 14*cam.s; ctx.beginPath(); ctx.moveTo(x2, y2); ctx.lineTo(x2-h*Math.cos(a-0.4), y2-h*Math.sin(a-0.4)); ctx.moveTo(x2, y2); ctx.lineTo(x2-h*Math.cos(a+0.4), y2-h*Math.sin(a+0.4)); ctx.stroke(); }
  } else if (el.t === "pen") {
    ctx.beginPath(); el.pts.forEach((p, i) => { const [sx, sy] = toScreen(p[0], p[1]); i ? ctx.lineTo(sx, sy) : ctx.moveTo(sx, sy); }); ctx.stroke();
  } else if (el.t === "text") {
    drawText(el.text || " ", el.x, el.y, el.size, el.stroke);
  }
}
function roundRect(x, y, w, h, r) { r = Math.min(r, Math.abs(w)/2, Math.abs(h)/2); ctx.beginPath(); ctx.moveTo(x+r, y); ctx.arcTo(x+w, y, x+w, y+h, r); ctx.arcTo(x+w, y+h, x, y+h, r); ctx.arcTo(x, y+h, x, y, r); ctx.arcTo(x, y, x+w, y, r); ctx.closePath(); }
function drawText(t, lx, ly, size, color, maxw) {
  const [sx, sy] = toScreen(lx, ly); ctx.fillStyle = color; ctx.font = `600 ${size*cam.s}px ui-sans-serif,system-ui,sans-serif`; ctx.textBaseline = "alphabetic";
  const lines = String(t).split("\n");
  lines.forEach((ln, i) => ctx.fillText(ln, sx, sy + i*size*1.25*cam.s));
}
function drawSelection(el) {
  const b = bbox(el); const [x, y] = toScreen(b.x, b.y);
  ctx.strokeStyle = "#5b9dff"; ctx.lineWidth = 1.5; ctx.setLineDash([5, 4]); ctx.strokeRect(x-2, y-2, b.w*cam.s+4, b.h*cam.s+4); ctx.setLineDash([]);
  for (const hd of handles(el)) { const [hx, hy] = toScreen(hd.x, hd.y); ctx.fillStyle = "#fff"; ctx.strokeStyle = "#5b9dff"; ctx.lineWidth = 1.5;
    ctx.beginPath(); ctx.rect(hx-4, hy-4, 8, 8); ctx.fill(); ctx.stroke(); }
}

// ---- interaction ----------------------------------------------------------
cv.addEventListener("mousedown", (e) => {
  if (!started || e.button !== 0) return;
  const [x, y] = evPos(e);
  if (tool === "select") {
    // handle?
    if (sel && els.has(sel)) { for (const hd of handles(els.get(sel))) { const [hx, hy] = toScreen(hd.x, hd.y); if (Math.hypot(hx-(e.clientX-cv.getBoundingClientRect().left), hy-(e.clientY-cv.getBoundingClientRect().top)) < 9) { drag = { mode: "resize", id: sel, handle: hd.k, orig: clone(els.get(sel)) }; return; } } }
    const id = topAt(x, y);
    selectEl(id);
    if (id) drag = { mode: "move", id, sx: x, sy: y, orig: clone(els.get(id)) };
    return;
  }
  if (tool === "eraser") { const id = topAt(x, y); if (id) erase(id); return; }
  if (tool === "text") { const el = mkEl("text", { x, y, text: "", size: 28, stroke: style.stroke }); selectEl(el.id); openText(el); return; }
  // creation drag
  const base = { x, y };
  let el;
  if (tool === "pen") el = mkEl("pen", { pts: [[x, y]], stroke: style.stroke, sw: style.sw });
  else if (tool === "line" || tool === "arrow") el = mkEl(tool, { x, y, x2: x, y2: y, stroke: style.stroke, sw: style.sw });
  else if (tool === "note") el = mkEl("note", { x, y, w: 1, h: 1, fill: "#ffd86b", stroke: "#caa23a", sw: 1, text: "" });
  else el = mkEl(tool, { x, y, w: 1, h: 1, stroke: style.stroke, fill: style.fill, sw: style.sw });
  drag = { mode: "create", id: el.id, sx: x, sy: y, preview: el, base };
});
cv.addEventListener("mousemove", (e) => {
  if (!started) return;
  const [x, y] = evPos(e);
  if (ws && ws.readyState === 1 && !offline) ws.send(`cursor ${x.toFixed(1)} ${y.toFixed(1)}`);
  if (!drag) { cv.style.cursor = tool === "select" ? "default" : "crosshair"; return; }
  if (drag.mode === "create") {
    const el = drag.preview;
    if (el.t === "pen") el.pts.push([x, y]);
    else if (el.t === "line" || el.t === "arrow") { el.x2 = x; el.y2 = y; }
    else { el.w = x - drag.base.x; el.h = y - drag.base.y; }
    push(el, true); redraw();
  } else if (drag.mode === "move") {
    const el = clone(drag.orig); const dx = x - drag.sx, dy = y - drag.sy; translate(el, dx, dy); push(el, true); redraw();
  } else if (drag.mode === "resize") {
    const el = clone(drag.orig); resizeEl(el, drag.handle, x, y); push(el, true); redraw();
  }
});
addEventListener("mouseup", () => {
  if (!drag) return;
  const el = els.get(drag.id);
  if (el) {
    if (el.t === "pen" && el.pts.length < 2) el.pts.push([el.pts[0][0]+0.5, el.pts[0][1]]);
    push(el, false); // final, unthrottled
    if (drag.mode === "create" && tool !== "pen") { selectEl(el.id); setTool("select"); }
  }
  drag = null; redraw();
});
cv.addEventListener("dblclick", (e) => {
  const [x, y] = evPos(e); const id = topAt(x, y);
  if (id && (els.get(id).t === "text" || els.get(id).t === "note")) { selectEl(id); openText(els.get(id)); }
});
addEventListener("keydown", (e) => {
  if ($("txtedit").style.display === "block") return;
  if (!started) return;
  if ((e.key === "Delete" || e.key === "Backspace") && sel) { erase(sel); e.preventDefault(); }
  const m = { v:"select", p:"pen", r:"rect", o:"ellipse", l:"line", a:"arrow", t:"text", n:"note", e:"eraser" }[e.key.toLowerCase()];
  if (m && !e.metaKey && !e.ctrlKey) setTool(m);
});

function topAt(x, y) { const ids = [...els.keys()]; for (let i = ids.length-1; i >= 0; i--) if (hit(els.get(ids[i]), x, y)) return ids[i]; return null; }
function clone(o) { return JSON.parse(JSON.stringify(o)); }
function mkEl(t, props) { const el = Object.assign({ id: `${myId||0}:${++seq}`, t }, props); els.set(el.id, el); return el; }
function translate(el, dx, dy) {
  if (el.t === "pen") el.pts = el.pts.map(p => [p[0]+dx, p[1]+dy]);
  else if (el.t === "line" || el.t === "arrow") { el.x+=dx; el.y+=dy; el.x2+=dx; el.y2+=dy; }
  else { el.x+=dx; el.y+=dy; }
}
function resizeEl(el, k, x, y) {
  if (el.t === "line" || el.t === "arrow") { if (k === "p1") { el.x = x; el.y = y; } else { el.x2 = x; el.y2 = y; } return; }
  let x0 = el.x, y0 = el.y, x1 = el.x + el.w, y1 = el.y + el.h;
  if (k.includes("w")) x0 = x; if (k.includes("e")) x1 = x; if (k.includes("n")) y0 = y; if (k.includes("s")) y1 = y;
  el.x = x0; el.y = y0; el.w = x1 - x0; el.h = y1 - y0;
}

// ---- text editing overlay -------------------------------------------------
function openText(el) {
  const te = $("txtedit"); const [sx, sy] = toScreen(el.x, el.y - (el.t === "text" ? el.size : 0));
  const r = cv.getBoundingClientRect();
  te.style.display = "block"; te.style.left = (r.left + sx) + "px"; te.style.top = (r.top + sy) + "px";
  te.style.fontSize = ((el.t === "text" ? el.size : 17) * cam.s) + "px"; te.style.minWidth = "120px";
  te.value = el.text || ""; te.focus();
  te.oninput = () => { el.text = te.value; push(el, true); redraw(); };
  te.onblur = te.onkeydown = (ev) => {
    if (ev && ev.type === "keydown" && !(ev.key === "Enter" && !ev.shiftKey) && ev.key !== "Escape") return;
    el.text = te.value; te.style.display = "none";
    if ((el.t === "text") && !el.text.trim()) erase(el.id); else push(el, false);
    redraw();
  };
}

// ---- presence -------------------------------------------------------------
function renderPresence() {
  $("users").innerHTML = presence.map(u => `<span class="av"><span class="dot" style="background:${esc(u.color)}"></span>${esc(u.name)}${u.id===myId?" (you)":""}</span>`).join("");
  $("cursors").innerHTML = presence.filter(u => u.id !== myId).map(u => { const [sx, sy] = toScreen(u.x, u.y);
    return `<div class="cur" style="left:${sx}px;top:${sy}px"><svg width="16" height="20"><path d="M0 0 L0 16 L4 12 L7 19 L9 18 L6 11 L11 11 Z" fill="${esc(u.color)}" stroke="#000a"/></svg><span>${esc(u.name)}</span></div>`; }).join("");
}
const esc = (s) => (s||"").replace(/[&<>"]/g, c => ({ "&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;" }[c]));

// ---- toolbar / properties -------------------------------------------------
const PALETTE = ["#e7ecf6","#5b9dff","#36d27a","#ffd24a","#ff7a3d","#ff5d6c","#c07dff","#11151d"];
function buildSwatches() {
  $("strokeSw").innerHTML = PALETTE.map(c => `<div class="sw" data-c="${c}" style="background:${c}"></div>`).join("");
  $("fillSw").innerHTML = `<div class="sw none" data-c="none" title="no fill"></div>` + PALETTE.map(c => `<div class="sw" data-c="${c}" style="background:${c}"></div>`).join("");
  $("strokeSw").querySelectorAll(".sw").forEach(s => s.onclick = () => setStroke(s.dataset.c));
  $("fillSw").querySelectorAll(".sw").forEach(s => s.onclick = () => setFill(s.dataset.c === "none" ? null : s.dataset.c));
  markSwatches();
}
function markSwatches() {
  $("strokeSw").querySelectorAll(".sw").forEach(s => s.classList.toggle("act", s.dataset.c === style.stroke));
  $("fillSw").querySelectorAll(".sw").forEach(s => s.classList.toggle("act", (s.dataset.c === "none" && !style.fill) || s.dataset.c === style.fill));
}
function setStroke(c) { style.stroke = c; $("strokeCustom").value = c; markSwatches(); applyStyleToSel("stroke", c); }
function setFill(c) { style.fill = c; markSwatches(); applyStyleToSel("fill", c); }
function applyStyleToSel(k, v) { if (sel && els.has(sel)) { const el = clone(els.get(sel)); el[k] = v; push(el, false); redraw(); } }
$("strokeCustom").oninput = () => setStroke($("strokeCustom").value);
$("weights").querySelectorAll("button").forEach(b => b.onclick = () => {
  $("weights").querySelectorAll("button").forEach(x => x.classList.remove("on")); b.classList.add("on");
  style.sw = +b.dataset.w; applyStyleToSel("sw", style.sw);
});
function setTool(t) { tool = t; document.querySelectorAll(".tool").forEach(b => b.classList.toggle("sel", b.dataset.tool === t)); if (t !== "select") selectEl(null); }
document.querySelectorAll(".tool").forEach(b => b.onclick = () => setTool(b.dataset.tool));
function selectEl(id) {
  sel = id;
  $("selbox").style.display = id ? "block" : "none";
  if (id && els.has(id)) { const el = els.get(id); $("selnote").textContent = `${el.t} · ${el.id}`; }
  redraw();
}
$("del").onclick = () => { if (sel) erase(sel); };
$("toFront").onclick = () => { if (sel && els.has(sel)) { const el = els.get(sel); els.delete(sel); els.set(sel, el); redraw(); } };
$("toBack").onclick = () => { if (sel && els.has(sel)) { const el = els.get(sel); const all = [...els]; els.clear(); els.set(sel, el); for (const [k, v] of all) if (k !== sel) els.set(k, v); redraw(); } };

// ---- offline + start ------------------------------------------------------
$("offline").onclick = () => {
  offline = !offline; const b = $("offline");
  if (offline) { b.textContent = "go online"; b.classList.remove("off"); b.classList.add("on"); $("offbanner").style.display = "block"; if (ws) ws.close(); }
  else { b.textContent = "go offline"; b.classList.add("off"); b.classList.remove("on"); $("offbanner").style.display = "none"; connect(); }
};
$("nick").value = "designer" + (Math.random()*900+100|0);
function start() { name = $("nick").value.trim() || "designer"; started = true; $("join").style.display = "none"; buildSwatches(); resize(); connect(); }
$("go").onclick = start;
$("nick").addEventListener("keydown", e => { if (e.key === "Enter") start(); });
resize();
