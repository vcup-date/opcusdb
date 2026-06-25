// Browser client for the opcusdb shared-world server. Connects over WebSocket,
// sends this tab's cursor + clicks, and renders the authoritative state the
// server broadcasts, so every connected tab sees the same live world.

const CANVAS = 700;
let W = 1000, H = 1000, myId = 0, scale = CANVAS / 1000;

const status = document.getElementById("status");
const app = new PIXI.Application({ width: CANVAS, height: CANVAS, background: 0x04060c, antialias: true });
app.view.id = "stage";
document.body.appendChild(app.view);
const g = new PIXI.Graphics();
app.stage.addChild(g);
const labels = new PIXI.Container();
app.stage.addChild(labels);

let entities = []; // {kind, owner, x, y}

const ws = new WebSocket(`ws://${location.host}/ws`);
ws.onopen = () => (status.textContent = "connected, open another tab to see shared state");
ws.onclose = () => (status.textContent = "disconnected (is the server running?)");
ws.onmessage = (ev) => {
  const msg = ev.data;
  if (msg[0] === "w") {
    const [, id, w, h] = msg.split(" ");
    myId = +id; W = +w; H = +h; scale = CANVAS / W;
    return;
  }
  // state frame: "c <owner> <x> <y>;d 0 <x> <y>;..."
  entities = msg.split(";").filter(Boolean).map((e) => {
    const [k, owner, x, y] = e.split(" ");
    return { kind: k, owner: +owner, x: +x, y: +y };
  });
};

// send this tab's cursor (throttled to animation frames) + spawn on click
let pending = null;
app.view.addEventListener("pointermove", (e) => {
  const r = app.view.getBoundingClientRect();
  pending = { x: Math.round(((e.clientX - r.left) / r.width) * W), y: Math.round(((e.clientY - r.top) / r.height) * H) };
});
app.view.addEventListener("pointerdown", (e) => {
  const r = app.view.getBoundingClientRect();
  const x = Math.round(((e.clientX - r.left) / r.width) * W);
  const y = Math.round(((e.clientY - r.top) / r.height) * H);
  if (ws.readyState === 1) ws.send(`s ${x} ${y}`);
});

const COLORS = [0x7dcfff, 0xffd24a, 0x6ee7b7, 0xff7eb6, 0xc4a7ff, 0xff9f5a];

app.ticker.add(() => {
  if (pending && ws.readyState === 1) { ws.send(`c ${pending.x} ${pending.y}`); pending = null; }
  g.clear();
  labels.removeChildren();
  let players = 0;
  for (const e of entities) {
    if (e.kind === "d") {
      g.beginFill(0x39506b).drawCircle(e.x * scale, e.y * scale, 2).endFill();
    }
  }
  for (const e of entities) {
    if (e.kind === "c") {
      players++;
      const mine = e.owner === myId;
      const col = mine ? 0xffffff : COLORS[e.owner % COLORS.length];
      g.beginFill(col).drawCircle(e.x * scale, e.y * scale, mine ? 9 : 7).endFill();
      const t = new PIXI.Text(mine ? "you" : `p${e.owner}`, { fontSize: 12, fill: col });
      t.position.set(e.x * scale + 10, e.y * scale - 8);
      labels.addChild(t);
    }
  }
  status.textContent = `connected · you are p${myId} · ${players} player(s) online · ${entities.length - players} shared dots`;
});
