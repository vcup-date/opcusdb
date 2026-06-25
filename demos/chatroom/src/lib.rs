//! `chatroom` — a serverless CRDT-mesh chat with human + AI peers.
//!
//! Demonstrates the opcusdb sync algebra in a P2P/agent setting (`DESIGN.md`
//! §6/§7/§9) without any network yet: each peer holds a replica of the chat
//! state and "gossips" by merging CRDTs. The chat log is an [`Rga`] (ordered,
//! mergeable) and presence is an [`OrSet`]. Because both are lattices, replicas
//! that diverge during an **offline partition** re-converge exactly when merged.
//!
//! The AI participant is just another peer: it **perceives** the message log and
//! **acts** by appending — see [`agent_reply`], a rule-based stand-in shaped so a
//! real model (tool-call → append) drops in unchanged.

use opcusdb_algebra::{Lattice, OpId, OrSet, Rga};

/// A chat message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    /// Display name of the author.
    pub author: String,
    /// Message text.
    pub text: String,
}

/// The replicated chat state: an ordered message log + a presence set. Both
/// fields are CRDTs, so the whole thing is a [`Lattice`].
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ChatState {
    /// The conversation, as a mergeable ordered sequence.
    pub messages: Rga<Message>,
    /// Who is present (add-wins observed-remove set of names).
    pub presence: OrSet<String>,
}

impl Lattice for ChatState {
    fn merge(&mut self, other: &Self) {
        self.messages.merge(&other.messages);
        self.presence.merge(&other.presence);
    }
}

impl ChatState {
    /// The conversation as `"name: text"` lines, in convergent order.
    pub fn transcript(&self) -> Vec<String> {
        self.messages
            .to_vec()
            .into_iter()
            .map(|m| format!("{}: {}", m.author, m.text))
            .collect()
    }

    /// Present participants, sorted.
    pub fn present(&self) -> Vec<&String> {
        let mut v: Vec<&String> = self.presence.iter().collect();
        v.sort();
        v
    }
}

/// A participant's replica: its identity plus a Lamport clock for unique,
/// causally-ordered operation ids.
#[derive(Clone, Debug)]
pub struct Peer {
    /// Replica id (distinct per peer); also the high bits of operation ids.
    pub id: u64,
    /// Display name.
    pub name: String,
    /// Lamport clock — advanced on every local op and on merge.
    pub lamport: u64,
    /// This peer's view of the chat.
    pub state: ChatState,
}

impl Peer {
    /// A fresh peer with empty state.
    pub fn new(id: u64, name: &str) -> Self {
        Self {
            id,
            name: name.to_string(),
            lamport: 0,
            state: ChatState::default(),
        }
    }

    fn tick(&mut self) -> u64 {
        self.lamport += 1;
        self.lamport
    }

    /// Announce presence.
    pub fn join(&mut self) {
        let l = self.tick();
        self.state.presence.add(self.name.clone(), (self.id, l));
    }

    /// Leave (tombstones this name's presence tags).
    pub fn leave(&mut self) {
        self.state.presence.remove(&self.name);
    }

    /// Post a message, appended after this replica's current last message.
    pub fn say(&mut self, text: &str) {
        let l = self.tick();
        self.state.messages.append(
            Message {
                author: self.name.clone(),
                text: text.to_string(),
            },
            OpId::new(l, self.id),
        );
    }

    /// Merge another peer's state into this one (one direction of a gossip).
    pub fn merge_from(&mut self, other: &Peer) {
        self.state.merge(&other.state);
        self.lamport = self.lamport.max(other.lamport);
    }
}

/// Rule-based AI participant: looks at the latest message and optionally replies.
/// Shaped so a real LLM call (perceive log → tool-call → text) drops in here.
pub fn agent_reply(state: &ChatState, agent_name: &str) -> Option<String> {
    let msgs = state.messages.to_vec();
    let last = msgs.last()?;
    if last.author == agent_name {
        return None; // never reply to itself
    }
    if last.text.trim_end().ends_with('?') {
        Some(format!("Good question, {}!", last.author))
    } else if last.text.to_lowercase().contains(&agent_name.to_lowercase()) {
        Some("You called?".to_string())
    } else {
        None
    }
}

/// Fully synchronize a connected group of peers (gossip to convergence): merge
/// everyone's state into one, then hand the merged state back to each.
pub fn gossip(peers: &mut [Peer], group: &[usize]) {
    let mut combined = ChatState::default();
    let mut max_lamport = 0;
    for &i in group {
        combined.merge(&peers[i].state);
        max_lamport = max_lamport.max(peers[i].lamport);
    }
    for &i in group {
        peers[i].state = combined.clone();
        peers[i].lamport = peers[i].lamport.max(max_lamport);
    }
}

/// The scripted demo: three peers (two humans + one AI) chat, split into an
/// offline partition where each side talks independently, then re-converge.
/// Returns the peers in their final (converged) state.
pub fn run_scenario() -> Vec<Peer> {
    let mut peers = vec![Peer::new(1, "alice"), Peer::new(2, "bob"), Peer::new(3, "ai")];
    let all = [0, 1, 2];

    // Everyone joins and syncs presence.
    for p in &mut peers {
        p.join();
    }
    gossip(&mut peers, &all);

    // Alice greets; everyone (incl. the agent) sees it.
    peers[0].say("hello everyone");
    gossip(&mut peers, &all);

    // --- PARTITION: {alice, ai} cannot reach {bob} ---
    peers[0].say("ai, are you there?"); // alice, in the alice+ai partition
    peers[1].say("bob here, working offline"); // bob, isolated

    // The agent, connected to alice, perceives and responds within its partition.
    gossip(&mut peers, &[0, 2]);
    if let Some(reply) = agent_reply(&peers[2].state, "ai") {
        peers[2].say(&reply);
    }
    gossip(&mut peers, &[0, 2]);

    // --- HEAL: the partition is repaired; everyone gossips. ---
    gossip(&mut peers, &all);
    peers
}

#[cfg(test)]
mod tests {
    use super::*;
    use opcusdb_algebra::join;

    #[test]
    fn all_peers_converge_after_partition() {
        let peers = run_scenario();
        // Every replica ends with identical state.
        for p in &peers[1..] {
            assert_eq!(p.state, peers[0].state, "peer {} diverged", p.name);
        }
        // The transcript contains messages from both sides of the partition.
        let t = peers[0].state.transcript();
        assert!(t.iter().any(|l| l.contains("hello everyone")));
        assert!(t.iter().any(|l| l.starts_with("alice:")));
        assert!(t.iter().any(|l| l.starts_with("bob:")));
        assert!(t.iter().any(|l| l.starts_with("ai:")), "agent participated");
    }

    #[test]
    fn presence_reflects_all_joins() {
        let peers = run_scenario();
        assert_eq!(
            peers[0].state.present(),
            vec![&"ai".to_string(), &"alice".to_string(), &"bob".to_string()]
        );
    }

    #[test]
    fn agent_replies_to_questions_only() {
        let mut s = ChatState::default();
        let mut alice = Peer::new(1, "alice");
        alice.say("ai, are you there?");
        s.merge(&alice.state);
        assert_eq!(
            agent_reply(&s, "ai"),
            Some("Good question, alice!".to_string())
        );

        // A non-question, non-mention -> no reply.
        let mut bob = Peer::new(2, "bob");
        bob.say("just chatting");
        let mut s2 = ChatState::default();
        s2.merge(&bob.state);
        assert_eq!(agent_reply(&s2, "ai"), None);
    }

    #[test]
    fn merge_is_order_independent() {
        // Two peers diverge concurrently; merging in either order converges.
        let mut a = Peer::new(1, "alice");
        let mut b = Peer::new(2, "bob");
        a.join();
        b.join();
        a.say("from alice");
        b.say("from bob");
        let ab = join(a.state.clone(), &b.state);
        let ba = join(b.state.clone(), &a.state);
        assert_eq!(ab, ba);
        assert_eq!(ab.transcript().len(), 2);
    }

    #[test]
    fn scenario_is_deterministic() {
        let a = run_scenario();
        let b = run_scenario();
        assert_eq!(a[0].state, b[0].state);
        assert_eq!(a[0].state.transcript(), b[0].state.transcript());
    }
}
