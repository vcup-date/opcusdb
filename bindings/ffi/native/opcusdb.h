/* opcusdb FFI — C header for the native dynamic library (libopcusdb_ffi).
 *
 * The same symbols are exported by the WASM build. Link against the cdylib
 * (target/release/libopcusdb_ffi.{dylib,so,dll}) from C, Unity (C# P/Invoke),
 * Godot, or any C-ABI host.
 *
 * Handles are opaque pointers; create/free each exactly once. Position buffers
 * are owned by the library and valid until the next step/free on that handle.
 */
#ifndef OPCUSDB_H
#define OPCUSDB_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ---- swarm (load-test drifters) ---- */
typedef struct OpSwarm OpSwarm;
OpSwarm *swarm_new(uint32_t n, uint32_t seed);
void swarm_free(OpSwarm *h);
void swarm_step(OpSwarm *h);
uint32_t swarm_len(const OpSwarm *h);
const int32_t *swarm_positions_ptr(const OpSwarm *h); /* 2*len i32s: x0,y0,... */
uint32_t swarm_count_in_region(const OpSwarm *h, int32_t x0, int32_t y0, int32_t x1, int32_t y1);
int32_t field_width(void);
int32_t field_height(void);

/* ---- particle field (interactive attractor/swirl) ---- */
typedef struct OpField OpField;
OpField *pfield_new(uint32_t n, uint32_t seed, int32_t width, int32_t height);
void pfield_free(OpField *h);
void pfield_set_attractor(OpField *h, int32_t x, int32_t y, uint32_t mode); /* 0 off,1 attract,2 repel */
void pfield_step(OpField *h);
uint32_t pfield_len(const OpField *h);
const int32_t *pfield_positions_ptr(const OpField *h);

/* ---- netcode session (client prediction vs authoritative server) ---- */
typedef struct OpSession OpSession;
int32_t session_size(void);
OpSession *session_new(uint32_t up_latency, uint32_t down_latency, uint32_t down_drop, uint32_t seed);
void session_free(OpSession *h);
void session_tick(OpSession *h, uint32_t has_move, int32_t dx, int32_t dy);
int32_t session_predicted_x(const OpSession *h);
int32_t session_predicted_y(const OpSession *h);
int32_t session_server_x(const OpSession *h);
int32_t session_server_y(const OpSession *h);
uint32_t session_pending(const OpSession *h);

#ifdef __cplusplus
}
#endif

#endif /* OPCUSDB_H */
