# opcusdb native FFI (Unity / Godot / C)

The same `opcusdb-ffi` cdylib that powers the web demos also builds as a **native
dynamic library**, so Unity, Godot, or any C-ABI host can drive the Rust sim via
P/Invoke. One ABI (`opcusdb.h`), two build targets (WASM + native).

## Build & verify (native)

```sh
cargo build --release -p opcusdb-ffi
# -> target/release/libopcusdb_ffi.{dylib,so,dll}

# Runnable C smoke test of the exact surface Unity/Godot use:
cc bindings/ffi/native/verify.c target/release/libopcusdb_ffi.dylib \
   -I bindings/ffi/native -o /tmp/opcusdb_verify && /tmp/opcusdb_verify
```

Expected:

```
swarm: 1000 entities, sample (246,128), in_bounds=1
session under lag: predicted_x=560 server_x=500 pending=6
after drain: predicted_x=560 server_x=560 pending=0
NATIVE VERIFY OK
```

(Note the numbers match the WASM build — one deterministic core, two FFI targets.)

## Unity

1. Build the cdylib (above) and copy it into `Assets/Plugins/`
   (`libopcusdb_ffi.dylib` / `opcusdb_ffi.dll` / `libopcusdb_ffi.so`).
2. Add `Unity_OpcusdbSwarm.cs` to your project.
3. Put it on a GameObject with a `MeshFilter` + `MeshRenderer` (a point/vertex
   material). Press Play → the swarm renders as a point cloud, stepped by Rust.

## Godot 4 (.NET / C#)

1. Build the cdylib and make it loadable by the project (next to the binary, a
   system lib path, or via an export preset).
2. Add `Godot_OpcusdbSwarm.cs`, attach to a `Node2D`, run the scene.

> GDScript can't P/Invoke; for a pure-GDScript binding, wrap this same C-ABI in a
> **GDExtension** (godot-cpp). The C-ABI (`opcusdb.h`) is ready for it.

## What's verified here vs. not

- **Verified, runnable:** the native cdylib and its C-ABI (`verify.c` above), i.e.
  the exact functions Unity/Godot call.
- **Provided as glue (not auto-run, no engine in CI):** the Unity/Godot C#
  scripts — standard `[DllImport]` over the verified ABI.
