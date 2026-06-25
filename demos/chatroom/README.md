# chatroom

A **serverless, CRDT-mesh chat** with human + AI peers — the P2P / agent track
from `DESIGN.md` §6/§7/§9, with no network layer yet (peers gossip in-process by
merging CRDT state).

## What it shows

- **CRDT mesh state.** Each peer holds a replica of `ChatState`:
  - the message log is an `Rga<Message>` (ordered, mergeable),
  - presence is an `OrSet<String>` (add-wins).
  Both are lattices, so `ChatState` merges conflict-free.
- **Offline partition + heal.** The scenario splits the peers into `{alice, ai}`
  and `{bob}`; each side talks independently, then the partition heals and a final
  gossip makes **every replica converge to the identical transcript** — proven by
  a test that asserts all peers are byte-equal afterwards.
- **AI agent as a peer.** The `ai` participant is just another peer: it
  **perceives** the message log and **acts** by appending (`agent_reply`). It is a
  rule-based stand-in shaped so a real model (perceive log → tool-call → append)
  drops in unchanged.

## Run it

```sh
cargo run -p opcusdb-chatroom --bin chatroom
```

```
== participants ==
  - ai
  - alice
  - bob

== converged transcript (all 3 peers agree) ==
  alice: hello everyone
  bob: bob here, working offline
  alice: ai, are you there?
  ai: Good question, alice!

all replicas converged after the offline partition: yes
```

## Tests

```sh
cargo test -p opcusdb-chatroom
```

- `all_peers_converge_after_partition` — every replica is identical after heal.
- `merge_is_order_independent` — gossip converges regardless of order.
- `agent_replies_to_questions_only` — the AI peer's perceive→act behavior.
- `presence_reflects_all_joins` / `scenario_is_deterministic`.

## Next (see repo `TODO.md`)

Wire this onto a real transport (WebRTC datachannels for browser P2P, QUIC for
native) so the mesh runs across machines instead of in-process.
