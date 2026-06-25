#!/usr/bin/env bash
# Build the opcusdb WASM core and copy it next to the web demo.
# Run from the repo root: bash bindings/ffi/build.sh
set -euo pipefail

cargo build --release --target wasm32-unknown-unknown -p opcusdb-ffi
cp target/wasm32-unknown-unknown/release/opcusdb_ffi.wasm bindings/ffi/web/opcusdb_ffi.wasm

echo "built bindings/ffi/web/opcusdb_ffi.wasm"
echo "headless check : node bindings/ffi/web/verify.mjs"
echo "browser demo   : (cd bindings/ffi/web && python3 -m http.server 8080) then open http://localhost:8080"
