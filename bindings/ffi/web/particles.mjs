// Interactive particle galaxy. The attractor/swirl physics runs entirely in the
// Rust ECS core compiled to WASM; this file feeds the pointer position in and
// renders the particles (read from wasm memory) with additive-blend glow.

const COUNT = 6000;
const SEED = 7;
const SIZE = 800; // logical field == canvas size

const MODE_OFF = 0;
const MODE_ATTRACT = 1;
const MODE_REPEL = 2;

const bytes = await (await fetch("./opcusdb_ffi.wasm")).arrayBuffer();
const { instance } = await WebAssembly.instantiate(bytes, {});
const ex = instance.exports;

const handle = ex.pfield_new(COUNT, SEED, SIZE, SIZE);
ex.pfield_set_attractor(handle, SIZE / 2, SIZE / 2, MODE_ATTRACT);

const app = new PIXI.Application({
  width: SIZE,
  height: SIZE,
  background: 0x04060c,
  antialias: true,
  resolution: Math.min(window.devicePixelRatio || 1, 2),
});
app.view.id = "stage";
document.body.appendChild(app.view);

// Additive blending makes overlapping particles bloom into a bright galactic core.
const dots = new PIXI.Graphics();
dots.blendMode = PIXI.BLEND_MODES.ADD;
app.stage.addChild(dots);
const stats = document.getElementById("stats");

// --- interaction: pointer attracts; holding repels -----------------------
let mouse = { x: SIZE / 2, y: SIZE / 2, down: false, inside: true };

function rectPos(e) {
  const r = app.view.getBoundingClientRect();
  return {
    x: ((e.clientX - r.left) / r.width) * SIZE,
    y: ((e.clientY - r.top) / r.height) * SIZE,
  };
}
app.view.addEventListener("pointermove", (e) => {
  const p = rectPos(e);
  mouse.x = p.x;
  mouse.y = p.y;
  mouse.inside = true;
});
app.view.addEventListener("pointerdown", () => (mouse.down = true));
window.addEventListener("pointerup", () => (mouse.down = false));
app.view.addEventListener("pointerleave", () => (mouse.inside = false));

app.ticker.add(() => {
  const mode = !mouse.inside ? MODE_OFF : mouse.down ? MODE_REPEL : MODE_ATTRACT;
  ex.pfield_set_attractor(handle, mouse.x | 0, mouse.y | 0, mode);
  ex.pfield_step(handle);

  const n = ex.pfield_len(handle);
  const ptr = ex.pfield_positions_ptr(handle);
  const pos = new Int32Array(ex.memory.buffer, ptr, n * 2);

  dots.clear();
  const color = mode === MODE_REPEL ? 0xff7755 : 0x3aa0ff;
  dots.beginFill(color, 0.85);
  for (let i = 0; i < n; i++) {
    dots.drawCircle(pos[i * 2], pos[i * 2 + 1], 1.5);
  }
  dots.endFill();

  stats.textContent = `· ${n} particles · ${app.ticker.FPS.toFixed(0)} fps`;
});
