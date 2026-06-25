// PixiJS renderer for the opcusdb swarm. The simulation runs entirely in the
// Rust core compiled to WASM; this file only steps it and draws positions read
// directly from wasm linear memory. (The WASM contract is verified headlessly by
// verify.mjs; this is the visual counterpart.)

const N = 4000;
const SEED = 7;
const CANVAS = 800;

// Load the WASM core (built to target/.../opcusdb_ffi.wasm; build.sh copies it here).
const bytes = await (await fetch("./opcusdb_ffi.wasm")).arrayBuffer();
const { instance } = await WebAssembly.instantiate(bytes, {});
const ex = instance.exports;

const W = ex.field_width();
const H = ex.field_height();
const scale = CANVAS / W;
const handle = ex.swarm_new(N, SEED);

const app = new PIXI.Application({ width: CANVAS, height: CANVAS, background: 0x0b0e14, antialias: true });
app.view.id = "stage";
document.body.appendChild(app.view);

const dots = new PIXI.Graphics();
app.stage.addChild(dots);
const stats = document.getElementById("stats");

// The interest radius (sim units) follows the mouse — MMO area-of-interest.
const RADIUS = 120;
let mouse = { x: W / 2, y: H / 2 };
app.view.addEventListener("pointermove", (e) => {
  const r = app.view.getBoundingClientRect();
  mouse.x = ((e.clientX - r.left) / r.width) * W;
  mouse.y = ((e.clientY - r.top) / r.height) * H;
});

app.ticker.add(() => {
  ex.swarm_step(handle);

  const n = ex.swarm_len(handle);
  // The Rust spatial grid computes the interest set near the cursor.
  const inSet = ex.swarm_mark_near(handle, mouse.x | 0, mouse.y | 0, RADIUS);

  // Re-create views each frame — wasm memory can grow and detach old buffers.
  const pos = new Int32Array(ex.memory.buffer, ex.swarm_positions_ptr(handle), n * 2);
  const flags = new Uint8Array(ex.memory.buffer, ex.swarm_flags_ptr(handle), n);

  dots.clear();
  // interest-radius outline
  dots.lineStyle(1, 0x3a4a66, 0.9).drawCircle(mouse.x * scale, mouse.y * scale, RADIUS * scale);
  // far entities (dim)
  dots.lineStyle(0).beginFill(0x39506b);
  for (let i = 0; i < n; i++) if (!flags[i]) dots.drawCircle(pos[i * 2] * scale, pos[i * 2 + 1] * scale, 1.5);
  dots.endFill();
  // interest set (bright)
  dots.beginFill(0xffd24a);
  for (let i = 0; i < n; i++) if (flags[i]) dots.drawCircle(pos[i * 2] * scale, pos[i * 2 + 1] * scale, 2.0);
  dots.endFill();

  stats.textContent = `· ${n} entities · ${inSet} in interest set · ${app.ticker.FPS.toFixed(0)} fps`;
});
