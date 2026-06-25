/* Native C-ABI smoke test for libopcusdb_ffi, the same surface Unity/Godot use.
 *
 * Build the cdylib first:  cargo build --release -p opcusdb-ffi
 * Then (from repo root):
 *   cc bindings/ffi/native/verify.c target/release/libopcusdb_ffi.dylib \
 *      -I bindings/ffi/native -o /tmp/opcusdb_verify && /tmp/opcusdb_verify
 */
#include "opcusdb.h"
#include <stdio.h>
#include <stdlib.h>

int main(void) {
    int rc = 0;

    /* swarm: step and read positions from the library-owned buffer */
    OpSwarm *sw = swarm_new(1000, 7);
    for (int i = 0; i < 50; i++) swarm_step(sw);
    uint32_t n = swarm_len(sw);
    const int32_t *pos = swarm_positions_ptr(sw);
    int in_bounds = 1;
    for (uint32_t i = 0; i < n; i++) {
        int32_t x = pos[i * 2], y = pos[i * 2 + 1];
        if (x < 0 || x >= field_width() || y < 0 || y >= field_height()) in_bounds = 0;
    }
    printf("swarm: %u entities, sample (%d,%d), in_bounds=%d\n", n, pos[0], pos[1], in_bounds);
    if (n != 1000 || !in_bounds) rc = 1;
    swarm_free(sw);

    /* netcode session: prediction leads under latency, converges on drain */
    OpSession *s = session_new(8, 8, 0, 1);
    for (int i = 0; i < 6; i++) session_tick(s, 1, 10, 0); /* move right */
    int pred = session_predicted_x(s), srv = session_server_x(s);
    uint32_t pend = session_pending(s);
    printf("session under lag: predicted_x=%d server_x=%d pending=%u\n", pred, srv, pend);
    if (!(pred > srv && pend == 6)) rc = 1; /* client must be ahead */
    for (int i = 0; i < 40; i++) session_tick(s, 0, 0, 0); /* drain */
    int pred2 = session_predicted_x(s), srv2 = session_server_x(s);
    printf("after drain: predicted_x=%d server_x=%d pending=%u\n", pred2, srv2, session_pending(s));
    if (pred2 != srv2) rc = 1; /* must converge */
    session_free(s);

    printf(rc == 0 ? "NATIVE VERIFY OK\n" : "NATIVE VERIFY FAILED\n");
    return rc;
}
