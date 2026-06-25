// Browser client for opcusdb Gomoku (five-in-a-row). Renders the board the
// authoritative server broadcasts and sends moves; the server enforces the rules.

const $ = (id) => document.getElementById(id);
const cv = $("board"), ctx = cv.getContext("2d");
let ws = null, myId = 0, role = "s";
let N = 15, board = [], toMove = 1, winner = 0, last = -1, winLine = [];
let blackName = ", ", whiteName = ", ", lb = [];
let hover = -1, joined = false;

const SIZE = 600, PAD = 28;
const step = () => (SIZE - 2 * PAD) / (N - 1);
const px = (i) => PAD + i * step();
const myColor = () => (role === "b" ? 1 : role === "w" ? 2 : 0);

// connect once on load; we start in the lobby (server streams the room list)
function boot() {
  ws = new WebSocket(`ws://${location.host}/ws`);
  ws.onclose = () => { $("status").textContent = "disconnected"; };
  ws.onmessage = (e) => {
    for (const line of e.data.split("\n")) {
      if (!line) continue;
      const p = line.split("\t");
      if (p[0] === "L") { if (!joined) renderRooms(p[1] || ""); }
      else if (p[0] === "w") { myId = +p[1]; role = p[2]; }
      else if (p[0] === "s") {
        N = +p[1]; board = [...p[2]].map(Number); toMove = +p[3]; winner = +p[4];
        last = +p[5]; blackName = p[6]; whiteName = p[7];
        winLine = p[8] === "-" ? [] : p[8].split(",").map(Number);
      } else if (p[0] === "l") {
        lb = (p[1] || "").split(",").filter(Boolean).map(x => { const [n, w] = x.split(":"); return [n, +w]; });
      }
    }
    if (joined) { render(); sidebar(); }
  };
}

function joinRoom(code) {
  if (!ws || ws.readyState !== 1 || !code) return;
  joined = true;
  ws.send(`join ${code} ${val("nick")}`);
  $("lobby").style.display = "none";
  $("roomTag").textContent = code;
}

function renderRooms(listStr) {
  const rooms = listStr.split(",").filter(Boolean).map(x => { const [code, n, status] = x.split(":"); return { code, n: +n, status }; });
  const el = $("roomList");
  if (!rooms.length) { el.innerHTML = '<div class="hint">no open rooms yet, create one!</div>'; return; }
  el.innerHTML = "";
  rooms.forEach(r => {
    const row = document.createElement("div"); row.className = "lobrow";
    row.innerHTML = `<span><b>${esc(r.code)}</b><span class="s">${r.n}/2 · ${r.status}</span></span>`;
    const btn = document.createElement("button");
    btn.className = "mini " + (r.status === "waiting" ? "primary" : "ghost");
    btn.textContent = r.status === "waiting" ? "join" : "watch";
    btn.onclick = () => joinRoom(r.code);
    row.appendChild(btn); el.appendChild(row);
  });
}

function render() {
  // wood
  const g = ctx.createLinearGradient(0, 0, SIZE, SIZE);
  g.addColorStop(0, "#e9b766"); g.addColorStop(1, "#d49a48");
  ctx.fillStyle = g; ctx.fillRect(0, 0, SIZE, SIZE);
  // grid
  ctx.strokeStyle = "#5a3a18"; ctx.lineWidth = 1;
  ctx.beginPath();
  for (let i = 0; i < N; i++) {
    ctx.moveTo(px(0), px(i)); ctx.lineTo(px(N - 1), px(i));
    ctx.moveTo(px(i), px(0)); ctx.lineTo(px(i), px(N - 1));
  }
  ctx.stroke();
  // star points (hoshi) for 15x15: 3,7,11
  ctx.fillStyle = "#5a3a18";
  for (const sx of [3, 7, 11]) for (const sy of [3, 7, 11]) {
    ctx.beginPath(); ctx.arc(px(sx), px(sy), 3.5, 0, 7); ctx.fill();
  }
  // hover preview
  if (hover >= 0 && board[hover] === 0 && winner === 0 && myColor() === toMove) {
    const hx = hover % N, hy = (hover / N) | 0;
    stone(hx, hy, myColor(), 0.35);
  }
  // stones
  for (let i = 0; i < board.length; i++) if (board[i]) stone(i % N, (i / N) | 0, board[i], 1);
  // last-move marker
  if (last >= 0) {
    ctx.strokeStyle = board[last] === 1 ? "#fff" : "#000"; ctx.lineWidth = 2;
    ctx.beginPath(); ctx.arc(px(last % N), px((last / N) | 0), step() * 0.16, 0, 7); ctx.stroke();
  }
  // winning line highlight
  for (const i of winLine) {
    ctx.strokeStyle = "#ff3b6b"; ctx.lineWidth = 3;
    ctx.beginPath(); ctx.arc(px(i % N), px((i / N) | 0), step() * 0.46, 0, 7); ctx.stroke();
  }
}

function stone(x, y, color, alpha) {
  const r = step() * 0.45, cx = px(x), cy = px(y);
  ctx.globalAlpha = alpha;
  ctx.beginPath(); ctx.arc(cx + 1.5, cy + 2, r, 0, 7); ctx.fillStyle = "rgba(0,0,0,.35)"; ctx.fill(); // shadow
  const rg = ctx.createRadialGradient(cx - r * 0.4, cy - r * 0.4, r * 0.1, cx, cy, r);
  if (color === 1) { rg.addColorStop(0, "#555"); rg.addColorStop(1, "#0a0a0a"); }
  else { rg.addColorStop(0, "#fff"); rg.addColorStop(1, "#cfd6e0"); }
  ctx.beginPath(); ctx.arc(cx, cy, r, 0, 7); ctx.fillStyle = rg; ctx.fill();
  ctx.globalAlpha = 1;
}

function sidebar() {
  $("blackName").textContent = blackName; $("whiteName").textContent = whiteName;
  const winOf = (n) => (lb.find(([x]) => x === n) || [, 0])[1];
  $("blackWins").textContent = winOf(blackName); $("whiteWins").textContent = winOf(whiteName);
  const st = $("status");
  const waiting = blackName === ", " || whiteName === ", ";
  if (winner === 3) { st.className = ""; st.textContent = "draw, board full"; }
  else if (winner !== 0) {
    const iWon = winner === myColor();
    st.className = iWon ? "win" : "lose";
    st.textContent = iWon ? "🎉 you win!" : `${winner === 1 ? "black" : "white"} wins`;
  } else if (waiting) { st.className = "wait"; st.textContent = "waiting for opponent…"; }
  else if (myColor() === 0) { st.className = ""; st.textContent = `spectating · ${toMove === 1 ? "black" : "white"} to move`; }
  else if (myColor() === toMove) { st.className = "turn"; st.textContent = "your turn"; }
  else { st.className = ""; st.textContent = "opponent's turn…"; }
  $("rematch").style.display = winner !== 0 ? "block" : "none";
  const el = $("leaderboard");
  el.innerHTML = lb.length ? "" : '<div style="color:#6b7a99">no wins yet</div>';
  lb.forEach(([n, w], i) => { const d = document.createElement("div"); d.className = "row";
    d.innerHTML = `<span>${i + 1}. ${esc(n)}</span><span>${w}</span>`; el.appendChild(d); });
}
const esc = (s) => s.replace(/[&<>]/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" }[c]));

function cellAt(ev) {
  const r = cv.getBoundingClientRect();
  const mx = (ev.clientX - r.left) * (SIZE / r.width), my = (ev.clientY - r.top) * (SIZE / r.height);
  const x = Math.round((mx - PAD) / step()), y = Math.round((my - PAD) / step());
  if (x < 0 || y < 0 || x >= N || y >= N) return -1;
  return y * N + x;
}
cv.addEventListener("mousemove", (e) => { const i = cellAt(e); if (i !== hover) { hover = i; render(); } });
cv.addEventListener("mouseleave", () => { hover = -1; render(); });
cv.addEventListener("click", (e) => {
  const i = cellAt(e);
  if (i < 0 || !ws || ws.readyState !== 1) return;
  if (winner === 0 && myColor() === toMove && board[i] === 0) ws.send(`place ${i % N} ${(i / N) | 0}`);
});
$("rematch").onclick = () => ws && ws.send("rematch");

// lobby
const rnd = () => Array.from({ length: 4 }, () => "ABCDEFGHJKLMNPQRSTUVWXYZ23456789"[Math.floor(Math.random() * 32)]).join("");
$("nick").value = "player" + Math.floor(Math.random() * 1000);
const val = (id) => $(id).value.trim() || $(id).placeholder;
$("create").onclick = () => joinRoom(rnd());
$("joinCode").onclick = () => joinRoom(($("code").value.trim() || rnd()).toUpperCase());
$("code").addEventListener("keydown", (e) => { if (e.key === "Enter") $("joinCode").click(); });
const q = new URLSearchParams(location.search).get("room"); if (q) $("code").value = q.toUpperCase();
render();
boot();
