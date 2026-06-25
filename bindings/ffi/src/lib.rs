//! FFI binding for the opcusdb sims, one minimal **C-ABI** serving two targets
//! from a single `cdylib`:
//! - **WebAssembly** (`--target wasm32-unknown-unknown`): a browser renderer
//!   (PixiJS/Three) reads positions straight from wasm linear memory.
//! - **Native** (host build → `.dylib`/`.so`/`.dll`): Unity (C# P/Invoke), Godot,
//!   or any C caller links the same symbols.
//!
//! No `wasm-bindgen`, no other deps. Two sims are exposed: `swarm_*` (the load-test
//! drifters) and `field_*` (the interactive attractor/swirl particle field).
//!
//! Safety: the FFI boundary requires raw pointers, so `unsafe` is allowed here
//! (localized to this thin shim), the rest of the workspace stays safe.
#![allow(unsafe_code)]

use opcusdb_loadtest::{Swarm, HEIGHT, WIDTH};
use opcusdb_netcode::net::{Session, SIZE as NET_SIZE};
use opcusdb_particles::{Field, Mode};

/// A swarm plus a reusable flat position buffer (`[x0, y0, x1, y1, ...]`).
pub struct WasmSwarm {
    swarm: Swarm,
    buf: Vec<i32>,
}

impl WasmSwarm {
    fn refresh(&mut self) {
        self.swarm.write_positions(&mut self.buf);
    }
}

/// Create a swarm of `n` entities seeded by `seed`. Returns an opaque handle the
/// other functions take; release it with [`swarm_free`].
///
/// # Safety
/// The returned pointer must eventually be passed to [`swarm_free`] exactly once
/// and not used after that.
#[no_mangle]
pub extern "C" fn swarm_new(n: u32, seed: u32) -> *mut WasmSwarm {
    let swarm = Swarm::new(n, seed as u64);
    let mut ws = Box::new(WasmSwarm {
        swarm,
        buf: Vec::with_capacity(n as usize * 2),
    });
    ws.refresh();
    Box::into_raw(ws)
}

/// Free a swarm handle.
///
/// # Safety
/// `handle` must come from [`swarm_new`] and not have been freed already.
#[no_mangle]
pub unsafe extern "C" fn swarm_free(handle: *mut WasmSwarm) {
    if !handle.is_null() {
        drop(Box::from_raw(handle));
    }
}

/// Advance the swarm one tick and refresh its position buffer.
///
/// # Safety
/// `handle` must be a live handle from [`swarm_new`].
#[no_mangle]
pub unsafe extern "C" fn swarm_step(handle: *mut WasmSwarm) {
    if let Some(ws) = handle.as_mut() {
        ws.swarm.step();
        ws.refresh();
    }
}

/// Number of entities (each contributes an `x,y` pair to the buffer).
///
/// # Safety
/// `handle` must be a live handle from [`swarm_new`].
#[no_mangle]
pub unsafe extern "C" fn swarm_len(handle: *const WasmSwarm) -> u32 {
    match handle.as_ref() {
        Some(ws) => ws.swarm.len(),
        None => 0,
    }
}

/// Pointer to the flat `[x0, y0, ...]` i32 buffer (read `swarm_len*2` elements).
/// Valid until the next [`swarm_step`] or [`swarm_free`]; re-read after stepping
/// because wasm memory may have grown (detaching JS views).
///
/// # Safety
/// `handle` must be a live handle from [`swarm_new`].
#[no_mangle]
pub unsafe extern "C" fn swarm_positions_ptr(handle: *const WasmSwarm) -> *const i32 {
    match handle.as_ref() {
        Some(ws) => ws.buf.as_ptr(),
        None => core::ptr::null(),
    }
}

/// Count entities in `[x0,x1) × [y0,y1)` (an interest-region query).
///
/// # Safety
/// `handle` must be a live handle from [`swarm_new`].
#[no_mangle]
pub unsafe extern "C" fn swarm_count_in_region(
    handle: *const WasmSwarm,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
) -> u32 {
    match handle.as_ref() {
        Some(ws) => ws.swarm.count_in_region(x0, y0, x1, y1) as u32,
        None => 0,
    }
}

/// Compute the interest set within `radius` of `(cx, cy)` via the spatial grid,
/// marking per-entity flags. Returns the size of the set. Read the flags with
/// [`swarm_flags_ptr`].
///
/// # Safety
/// `handle` must be a live handle from [`swarm_new`].
#[no_mangle]
pub unsafe extern "C" fn swarm_mark_near(
    handle: *mut WasmSwarm,
    cx: i32,
    cy: i32,
    radius: i32,
) -> u32 {
    match handle.as_mut() {
        Some(ws) => ws.swarm.mark_near(cx, cy, radius) as u32,
        None => 0,
    }
}

/// Pointer to the per-entity interest-flag buffer (`swarm_len` bytes; 1 = in the
/// last [`swarm_mark_near`] set), aligned with the positions buffer order.
///
/// # Safety
/// `handle` must be a live handle from [`swarm_new`].
#[no_mangle]
pub unsafe extern "C" fn swarm_flags_ptr(handle: *const WasmSwarm) -> *const u8 {
    match handle.as_ref() {
        Some(ws) => ws.swarm.flags().as_ptr(),
        None => core::ptr::null(),
    }
}

/// Deterministic checksum of all positions. Equal across native and WASM builds
/// for the same seed/steps, the cross-target determinism gate.
///
/// # Safety
/// `handle` must be a live handle from [`swarm_new`].
#[no_mangle]
pub unsafe extern "C" fn swarm_checksum(handle: *const WasmSwarm) -> u64 {
    match handle.as_ref() {
        Some(ws) => ws.swarm.checksum(),
        None => 0,
    }
}

/// The field width (positions are in `0..field_width`).
#[no_mangle]
pub extern "C" fn field_width() -> i32 {
    WIDTH
}

/// The field height.
#[no_mangle]
pub extern "C" fn field_height() -> i32 {
    HEIGHT
}

// ---------------------------------------------------------------------------
// Interactive particle field (`field_*`)
// ---------------------------------------------------------------------------

/// Create a particle field of `n` particles in a `width × height` pixel space.
///
/// # Safety
/// The returned handle must be released once with [`pfield_free`].
#[no_mangle]
pub extern "C" fn pfield_new(n: u32, seed: u32, width: i32, height: i32) -> *mut Field {
    Box::into_raw(Box::new(Field::new(n, seed as u64, width, height)))
}

/// Free a particle field handle.
///
/// # Safety
/// `handle` must come from [`pfield_new`] and not already be freed.
#[no_mangle]
pub unsafe extern "C" fn pfield_free(handle: *mut Field) {
    if !handle.is_null() {
        drop(Box::from_raw(handle));
    }
}

/// Set the attractor to pixel `(x, y)` with `mode`: 0 = off, 1 = attract, 2 = repel.
///
/// # Safety
/// `handle` must be a live handle from [`pfield_new`].
#[no_mangle]
pub unsafe extern "C" fn pfield_set_attractor(handle: *mut Field, x: i32, y: i32, mode: u32) {
    if let Some(f) = handle.as_mut() {
        let m = match mode {
            1 => Mode::Attract,
            2 => Mode::Repel,
            _ => Mode::Off,
        };
        f.set_attractor(x, y, m);
    }
}

/// Advance the field one tick.
///
/// # Safety
/// `handle` must be a live handle from [`pfield_new`].
#[no_mangle]
pub unsafe extern "C" fn pfield_step(handle: *mut Field) {
    if let Some(f) = handle.as_mut() {
        f.step();
    }
}

/// Particle count (the buffer holds `2 * count` i32s).
///
/// # Safety
/// `handle` must be a live handle from [`pfield_new`].
#[no_mangle]
pub unsafe extern "C" fn pfield_len(handle: *const Field) -> u32 {
    match handle.as_ref() {
        Some(f) => f.len(),
        None => 0,
    }
}

/// Pointer to the flat pixel-position buffer `[x0, y0, ...]`. Re-read after each
/// [`pfield_step`] (wasm memory may grow and detach views).
///
/// # Safety
/// `handle` must be a live handle from [`pfield_new`].
#[no_mangle]
pub unsafe extern "C" fn pfield_positions_ptr(handle: *const Field) -> *const i32 {
    match handle.as_ref() {
        Some(f) => f.pixels().as_ptr(),
        None => core::ptr::null(),
    }
}

// ---------------------------------------------------------------------------
// Netcode session (`session_*`), client prediction vs authoritative server
// ---------------------------------------------------------------------------

/// The field size the session moves within (positions are `0..session_size`).
#[no_mangle]
pub extern "C" fn session_size() -> i32 {
    NET_SIZE
}

/// Create a client/server session over a simulated link: `up_latency` delays the
/// client's inputs, `down_latency`/`down_drop` delay/drop the server's snapshots.
///
/// # Safety
/// The returned handle must be released once with [`session_free`].
#[no_mangle]
pub extern "C" fn session_new(
    up_latency: u32,
    down_latency: u32,
    down_drop: u32,
    seed: u32,
) -> *mut Session {
    let s = Session::new(up_latency as u64, down_latency as u64, down_drop, seed as u64);
    Box::into_raw(Box::new(s))
}

/// Free a session handle.
///
/// # Safety
/// `handle` must come from [`session_new`] and not already be freed.
#[no_mangle]
pub unsafe extern "C" fn session_free(handle: *mut Session) {
    if !handle.is_null() {
        drop(Box::from_raw(handle));
    }
}

/// Advance one tick. If `has_move != 0`, the client issues move `(dx, dy)`.
///
/// # Safety
/// `handle` must be a live handle from [`session_new`].
#[no_mangle]
pub unsafe extern "C" fn session_tick(handle: *mut Session, has_move: u32, dx: i32, dy: i32) {
    if let Some(s) = handle.as_mut() {
        s.tick(if has_move != 0 { Some((dx, dy)) } else { None });
    }
}

/// Client predicted X (the responsive, lag-free position the player sees).
///
/// # Safety
/// `handle` must be a live handle from [`session_new`].
#[no_mangle]
pub unsafe extern "C" fn session_predicted_x(handle: *const Session) -> i32 {
    handle.as_ref().map_or(0, |s| s.predicted().0)
}

/// Client predicted Y.
///
/// # Safety
/// `handle` must be a live handle from [`session_new`].
#[no_mangle]
pub unsafe extern "C" fn session_predicted_y(handle: *const Session) -> i32 {
    handle.as_ref().map_or(0, |s| s.predicted().1)
}

/// Authoritative server X (the lagging ground truth).
///
/// # Safety
/// `handle` must be a live handle from [`session_new`].
#[no_mangle]
pub unsafe extern "C" fn session_server_x(handle: *const Session) -> i32 {
    handle.as_ref().map_or(0, |s| s.server().0)
}

/// Authoritative server Y.
///
/// # Safety
/// `handle` must be a live handle from [`session_new`].
#[no_mangle]
pub unsafe extern "C" fn session_server_y(handle: *const Session) -> i32 {
    handle.as_ref().map_or(0, |s| s.server().1)
}

/// Number of unacknowledged inputs in flight (the "lag debt").
///
/// # Safety
/// `handle` must be a live handle from [`session_new`].
#[no_mangle]
pub unsafe extern "C" fn session_pending(handle: *const Session) -> u32 {
    handle.as_ref().map_or(0, |s| s.pending() as u32)
}
