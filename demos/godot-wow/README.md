# opcusdb Townfall, Godot 4 MMO town demo

A tiny **3D MMO-style town** built in **Godot 4** that talks to the opcusdb
authoritative server over WebSocket. A small town with **NPC quest givers**, a pack
of **wolves** to kill, a **quest** ("Cull the Wolves, slay 5"), and **chat**, and **multiple players see each other** move and fight in one shared world.

The Godot client is a *thin* client: it renders the world the Rust server
broadcasts and sends input. The whole simulation (movement, wolf AI, combat,
quests, chat) lives in [`demos/server/src/wow.rs`](../server/src/wow.rs).

## Run

1. Start the server (from the repo root):
   ```sh
   cargo run -p opcusdb-server --bin opcusdb-wow      # listens on :9007
   ```
2. Open this folder (`demos/godot-wow`) in **Godot 4.4+** and press **Play**.
   Launch it **two or more times** (or run multiple copies / `--path` instances)
   to see players share the world.

## Controls

| | |
|---|---|
| **WASD / arrows** | move |
| **Space** | attack the nearest wolf |
| **E** | talk to an NPC (accept / turn in the quest) |
| **Enter** | chat |

## How it works

- `Main.gd` builds the 3D town procedurally (no imported assets, flat-shaded
  blocky meshes for a simple "pixel" look), connects with `WebSocketPeer`, and on
  each frame applies the server snapshot (players / wolves / NPCs) with
  interpolation, then sends your held-key input.
- The server runs at 20 Hz: it integrates movement, drives wolf wander/aggro/bite
  AI, resolves your melee swings, tracks per-player quest progress, and relays
  chat, exactly the authoritative model used by the other opcusdb demos, just
  with a Godot front-end instead of a browser one.

> Tip: the screenshot in the root README was captured headlessly by running the
> project with a `WOW_SHOT=<path>` environment variable set (see `Main.gd`).
