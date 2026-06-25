// Visual netcode: client prediction vs authoritative server over a simulated
// laggy/lossy link. The whole client/server loop runs in the Rust/WASM core
// (Session); this file just feeds the mouse as movement input and draws the two
// positions so you can SEE the lag and the reconciliation.

const CANVAS = 600;

const bytes = await (await fetch("./opcusdb_ffi.wasm")).arrayBuffer();
const { instance } = await WebAssembly.instantiate(bytes, {});
const ex = instance.exports;

const SIZE = ex.session_size();
const scale = CANVAS / SIZE;

// (Re)create the session from the current slider values.
let params = { up: 8, down: 8, drop: 0 };
let session = ex.session_new(params.up, params.down, params.drop, 1);
function rebuild() {
  ex.session_free(session);
  session = ex.session_new(params.up, params.down, params.drop, 1);
}
for (const [id, key] of [["up", "up"], ["down", "down"], ["drop", "drop"]]) {
  const el = document.getElementById(id);
  const out = document.getElementById(id + "v");
  el.addEventListener("input", () => {
    params[key] = +el.value;
    out.textContent = el.value;
    rebuild();
  });
}

const app = new PIXI.Application({ width: CANVAS, height: CANVAS, background: 0x04060c, antialias: true });
app.view.id = "stage";
document.body.appendChild(app.view);

const g = new PIXI.Graphics();
app.stage.addChild(g);
const stats = document.getElementById("stats");

let mouse = { x: SIZE / 2, y: SIZE / 2 };
app.view.addEventListener("pointermove", (e) => {
  const r = app.view.getBoundingClientRect();
  mouse.x = ((e.clientX - r.left) / r.width) * SIZE;
  mouse.y = ((e.clientY - r.top) / r.height) * SIZE;
});

app.ticker.add(() => {
  // Steer the client toward the mouse; the sim clamps step server-side.
  const px = ex.session_predicted_x(session);
  const py = ex.session_predicted_y(session);
  ex.session_tick(session, 1, Math.round(mouse.x - px), Math.round(mouse.y - py));

  const cx = ex.session_predicted_x(session) * scale;
  const cy = ex.session_predicted_y(session) * scale;
  const sx = ex.session_server_x(session) * scale;
  const sy = ex.session_server_y(session) * scale;
  const pending = ex.session_pending(session);

  g.clear();
  // gap line: how far behind the authoritative state is
  g.lineStyle(1, 0x33425f, 0.8).moveTo(sx, sy).lineTo(cx, cy);
  // server ghost (authoritative)
  g.lineStyle(0).beginFill(0xffa057, 0.35).drawCircle(sx, sy, 12).endFill();
  // predicted client (what the player sees)
  g.beginFill(0x3aa0ff, 0.95).drawCircle(cx, cy, 7).endFill();

  stats.textContent =
    `up ${params.up}t · down ${params.down}t · loss ${params.drop}% · ${pending} inputs in flight · ${app.ticker.FPS.toFixed(0)} fps`;
});
