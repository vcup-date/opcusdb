#!/usr/bin/env bash
# Cross-target determinism gate (CORE_SPEC.md acceptance: "byte-identical across
# platforms"). Runs the SAME deterministic swarm on the native build and the WASM
# build and asserts they produce the identical checksum.
#
# Run from the repo root: bash bindings/ffi/determinism_gate.sh
set -euo pipefail

N=${1:-2000}
T=${2:-120}
S=${3:-12345}

# Build both targets.
cargo build --release -q -p opcusdb-loadtest
bash bindings/ffi/build.sh >/dev/null

NATIVE=$(cargo run --release -q -p opcusdb-loadtest -- --entities "$N" --ticks "$T" --seed "$S" \
  | grep checksum | awk '{print $3}')

WASM=$(node -e "
const fs=require('fs');
const b=fs.readFileSync('bindings/ffi/web/opcusdb_ffi.wasm');
WebAssembly.instantiate(b,{}).then(({instance})=>{
  const ex=instance.exports;
  const h=ex.swarm_new($N,$S);
  for(let i=0;i<$T;i++) ex.swarm_step(h);
  console.log('0x'+BigInt.asUintN(64, ex.swarm_checksum(h)).toString(16).padStart(16,'0'));
  ex.swarm_free(h);
});
")

echo "config: entities=$N ticks=$T seed=$S"
echo "native (Rust): $NATIVE"
echo "wasm   (node): $WASM"
if [ "$NATIVE" = "$WASM" ]; then
  echo "DETERMINISM GATE: PASS (byte-identical across targets)"
else
  echo "DETERMINISM GATE: FAIL"
  exit 1
fi
