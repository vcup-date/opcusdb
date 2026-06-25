# opcusdb servers

Two authoritative servers that let many clients share one live world, both over a
**hand-rolled, dependency-free WebSocket** (`src/ws.rs`, RFC 6455 handshake +
framing in std Rust). This is the answer to "how do multiple people see the same
thing?": clients send inputs; the server simulates authoritatively and broadcasts
state to everyone.

## 1. Shared-world game (`opcusdb-server`)

```sh
cargo run -p opcusdb-server          # open http://localhost:9001 in 2+ tabs
```

The server runs the real `opcusdb_core::World` (ECS) at 25 Hz. Each browser tab is
a player: move the mouse to move your cursor, click to spawn dots the server
simulates for everyone. The `World` lives only on the sim thread; clients reach it
via an `mpsc` channel (inputs) and a shared snapshot (broadcast).

## 2. Human + AI chatroom (`opcusdb-chat`)

An IRC-style `#lobby`. Anyone logs in with a nick; **10 AI chatters** powered by
OpenRouter (`deepseek/deepseek-v4-flash`) talk with humans and each other.

```sh
export OPENROUTER_API_KEY=sk-or-...
cargo run -p opcusdb-server --bin opcusdb-chat     # open http://localhost:9002
# or, locally:  bash run-chat.sh   (gitignored helper that sets the key)
```

- The API key is read from **`OPENROUTER_API_KEY`** and is **never stored in the
  repo** (`.env` and `run-chat.sh` are gitignored; copy `.env.example`).
- The AI HTTPS calls go through the system `curl` (no TLS dependency).
- To conserve credits, the bots only chat while ≥1 human is connected.

## Tests

```sh
cargo test -p opcusdb-server
```

Covers the WebSocket handshake (SHA-1 / base64, RFC 6455 vector), the JSON
encode/extract used for OpenRouter, the shared-world logic, and the chatroom
transcript/user-list. The live paths were checked with concurrent clients.
