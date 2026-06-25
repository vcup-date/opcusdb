# opcusdb-ffi

The opcusdb ECS core compiled to **WebAssembly** for browser clients, the
foundation of the web/PixiJS (and later Three.js) demos (`DESIGN.md` §3, §8).

Deliberately **no `wasm-bindgen` dependency**: the binding is a minimal hand-written
C-ABI, and JS reads entity positions straight from wasm linear memory. Same
dependency-free ethos as the rest of the project; build with plain `cargo`.

## Build

```sh
bash bindings/wasm/build.sh
# -> bindings/wasm/web/opcusdb_ffi.wasm  (~50 KB)
```

## Verify (headless, no browser)

```sh
node bindings/wasm/web/verify.mjs
```

Instantiates the wasm in Node, steps the swarm, reads positions from memory, and
asserts determinism (two same-seed handles match) and in-bounds positions:

```
field 1000x1000
created swarm of 1000 entities
after 50 steps: sample pos (246, 128); 279/1000 in center half
deterministic across handles: true
all positions in bounds: true
VERIFY OK
```

## Cross-target determinism gate

The same deterministic core runs on native *and* WASM, and produces **identical
bytes**. The gate proves it (CORE_SPEC acceptance: "byte-identical across platforms"):

```sh
bash bindings/ffi/determinism_gate.sh           # defaults: 2000 entities, 120 ticks
bash bindings/ffi/determinism_gate.sh 500 60 99 # entities ticks seed
```

```
native (Rust): 0xcd8d89b3520fccd1
wasm   (node): 0xcd8d89b3520fccd1
DETERMINISM GATE: PASS (byte-identical across targets)
```

This is what makes lockstep and replay safe: the server's arithmetic and the
browser's arithmetic agree to the bit.

## Browser demos (PixiJS)

```sh
cd bindings/ffi/web && python3 -m http.server 8080
# open http://localhost:8080
```

- `index.html`, swarm with cursor-following AOI interest set.
- `particles.html`, interactive particle galaxy.
- `netcode.html`, client prediction vs authoritative server with lag sliders.

Each runs its simulation entirely in the Rust core compiled to WASM; JS only renders.

## The C-ABI

| Export | Meaning |
|---|---|
| `swarm_new(n, seed) -> handle` | create a swarm |
| `swarm_step(handle)` | advance one tick, refresh the position buffer |
| `swarm_len(handle) -> u32` | entity count (buffer holds `2*len` i32s) |
| `swarm_positions_ptr(handle) -> *i32` | pointer to `[x0,y0,x1,y1,…]` in `memory` |
| `swarm_count_in_region(handle,x0,y0,x1,y1) -> u32` | interest-region query |
| `field_width()/field_height() -> i32` | field bounds |
| `swarm_free(handle)` | release |

Re-read `memory.buffer` / `swarm_positions_ptr` after each `swarm_step`: wasm
memory can grow and detach earlier typed-array views.
