// Headless verification of the opcusdb WASM binding in Node (no browser needed).
// Instantiates the .wasm, creates a swarm, steps it, and reads positions straight
// from linear memory, the same contract the PixiJS demo uses.
//
// Run after building:
//   cargo build --release --target wasm32-unknown-unknown -p opcusdb-ffi
//   node bindings/wasm/web/verify.mjs

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const wasmPath = join(here, "../../../target/wasm32-unknown-unknown/release/opcusdb_ffi.wasm");

const bytes = readFileSync(wasmPath);
const { instance } = await WebAssembly.instantiate(bytes, {});
const ex = instance.exports;

function positions(handle) {
  const n = ex.swarm_len(handle);
  const ptr = ex.swarm_positions_ptr(handle);
  // Re-create the view each time: wasm memory may grow and detach old buffers.
  return new Int32Array(ex.memory.buffer, ptr, n * 2);
}

const W = ex.field_width();
const H = ex.field_height();
console.log(`field ${W}x${H}`);

const N = 1000;
const handle = ex.swarm_new(N, 7);
console.log(`created swarm of ${ex.swarm_len(handle)} entities`);

// Determinism: two handles with the same seed must match after the same steps.
const handle2 = ex.swarm_new(N, 7);
for (let t = 0; t < 50; t++) {
  ex.swarm_step(handle);
  ex.swarm_step(handle2);
}
const a = positions(handle);
const b = positions(handle2);
let identical = a.length === b.length;
for (let i = 0; identical && i < a.length; i++) identical &&= a[i] === b[i];

// Sanity: every position is on the field.
let inBounds = true;
for (let i = 0; i < a.length; i += 2) {
  if (a[i] < 0 || a[i] >= W || a[i + 1] < 0 || a[i + 1] >= H) inBounds = false;
}

const center = ex.swarm_count_in_region(handle, (W / 4) | 0, (H / 4) | 0, ((3 * W) / 4) | 0, ((3 * H) / 4) | 0);
console.log(`after 50 steps: sample pos (${a[0]}, ${a[1]}); ${center}/${N} in center half`);
console.log(`deterministic across handles: ${identical}`);
console.log(`all positions in bounds: ${inBounds}`);

ex.swarm_free(handle);
ex.swarm_free(handle2);

if (!identical || !inBounds) {
  console.error("VERIFY FAILED");
  process.exit(1);
}
console.log("VERIFY OK");
