#!/usr/bin/env bash
# Build and smoke every autoagent-bingen backend end-to-end (B4-T4).
# Each `cargo build --features X --lib` overwrites the single cdylib, so a
# backend's smoke runs immediately after its build, before the next overwrites.
set -euo pipefail

cd "$(dirname "$0")/../../.." # repo root
ROOT="$PWD"

case "$(uname -s)" in
  Darwin) EXT=dylib ;;
  Linux) EXT=so ;;
  *) EXT=dll ;;
esac
LIB="$ROOT/target/release/lib autoagent_bingen.$EXT"
LIB="${LIB// /}" # libautoagent_bingen.<ext>

echo "== rust unit + integration tests (registry/wrappers/approval/mutating/parity/equivalence) =="
cargo test -p autoagent-bingen

echo "== drift guard =="
cargo run -q -p autoagent-bingen --bin bingen -- check

echo "== napi (Node primary): build + smoke + approval =="
cargo run -q -p autoagent-bingen --bin bingen -- smoke
node --test "$ROOT/crates/autoagent-bingen/__test__/approval.test.mjs"

echo "== node-bindgen (Node alternative): needs -C link-dead-code =="
RUSTFLAGS="-C link-dead-code" cargo build -q -p autoagent-bingen --features node-bindgen --release --lib
node "$ROOT/crates/autoagent-bingen/__test__/node_bindgen.smoke.mjs"

echo "== pyo3 (Python primary): maturin + pytest =="
( cd "$ROOT/crates/autoagent-bingen" \
    && python3 -m venv .venv 2>/dev/null || true \
    && . .venv/bin/activate \
    && pip install -q maturin pytest \
    && maturin develop --features py-pyo3 \
    && pytest tests_py -q )

echo "== rustpython (Python alternative): in-VM smoke =="
cargo test -q -p autoagent-bingen --features py-rustpython --test rustpython

echo "== deno_bindgen (Deno primary): build + codegen + smoke =="
cargo build -q -p autoagent-bingen --features deno-bindgen --release --lib
META=$(find "$ROOT/target/release/build" -path '*autoagent-bingen*/out/bindings.json' | head -1)
deno run -A "$ROOT/crates/autoagent-bingen/deno/gen.ts" "$META" "../../../target/release"
AUTOAGENT_BINGEN_LIB="$LIB" deno run --allow-ffi --allow-read --allow-write --allow-env \
  "$ROOT/crates/autoagent-bingen/deno/smoke_bindgen.ts"

echo "== raw FFI (Deno alternative): build + smoke =="
cargo build -q -p autoagent-bingen --features deno-ffi --release --lib
AUTOAGENT_BINGEN_LIB="$LIB" deno run --allow-ffi --allow-read --allow-write --allow-env \
  "$ROOT/crates/autoagent-bingen/deno/smoke.ts"

echo ""
echo "ALL SIX BACKENDS OK: napi, node-bindgen, pyo3, rustpython, deno_bindgen, raw-ffi"
