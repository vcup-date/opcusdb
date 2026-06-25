//! `chatroom` CLI, run the serverless CRDT-mesh chat scenario and print the
//! converged transcript. Demonstrates offline partition + merge and an AI peer.

use opcusdb_chatroom::run_scenario;

fn main() {
    let peers = run_scenario();
    let state = &peers[0].state;

    println!("== participants ==");
    for name in state.present() {
        println!("  - {name}");
    }

    println!("\n== converged transcript (all {} peers agree) ==", peers.len());
    for line in state.transcript() {
        println!("  {line}");
    }

    // Confirm convergence explicitly.
    let converged = peers[1..].iter().all(|p| p.state == *state);
    println!(
        "\nall replicas converged after the offline partition: {}",
        if converged { "yes" } else { "NO" }
    );
}
