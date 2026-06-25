// Browser client for the opcusdb human+AI chatroom. Connects over WebSocket,
// sends this user's messages, and renders the shared channel the server hosts
// (humans + AI chatters powered by OpenRouter).

const $ = (id) => document.getElementById(id);
let ws = null;
const colorFor = (name) => {
  let h = 0;
  for (const c of name) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return "c" + (h % 6);
};

function addLine(html, cls = "") {
  const log = $("log");
  const atBottom = log.scrollHeight - log.scrollTop - log.clientHeight < 40;
  const div = document.createElement("div");
  div.className = "line " + cls;
  div.innerHTML = html;
  log.appendChild(div);
  if (atBottom) log.scrollTop = log.scrollHeight;
}

const esc = (s) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));

function join() {
  const nick = $("nick").value.trim() || "guest";
  ws = new WebSocket(`ws://${location.host}/ws`);
  ws.onopen = () => {
    ws.send("login " + nick);
    $("join").style.display = "none";
    $("app").style.display = "flex";
    $("msg").focus();
    $("status").textContent = "· connected";
  };
  ws.onclose = () => { $("status").textContent = "· disconnected"; };
  ws.onmessage = (e) => {
    const parts = e.data.split("\t");
    if (parts[0] === "m") {
      const [, author, kind] = parts;
      const text = parts.slice(3).join("\t");
      if (kind === "s") {
        addLine(esc(text), "sys");
      } else {
        const bot = kind === "1";
        addLine(`<span class="nick ${colorFor(author)}">${esc(author)}</span> ${esc(text)}`, bot ? "bot" : "");
      }
    } else if (parts[0] === "u") {
      const users = $("users");
      users.innerHTML = "";
      for (const u of parts[1].split(",").filter(Boolean)) {
        const [name, kind] = u.split(":");
        const d = document.createElement("div");
        d.className = kind === "1" ? "u-bot" : "";
        d.textContent = (kind === "1" ? "🤖 " : "• ") + name;
        users.appendChild(d);
      }
    } else if (parts[0] === "t") {
      const names = (parts[1] || "").split(",").filter(Boolean);
      const el = $("typing");
      el.textContent = names.length === 0 ? ""
        : names.length === 1 ? `${names[0]} is typing…`
        : names.length === 2 ? `${names[0]} and ${names[1]} are typing…`
        : `${names.length} people are typing…`;
    }
  };
}

$("go").onclick = join;
$("nick").addEventListener("keydown", (e) => { if (e.key === "Enter") join(); });
$("msg").addEventListener("keydown", (e) => {
  if (e.key === "Enter" && ws && ws.readyState === 1 && e.target.value.trim()) {
    ws.send("msg " + e.target.value);
    e.target.value = "";
  }
});
