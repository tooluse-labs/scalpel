#!/bin/sh
set -eu

if [ -z "${PDBG_MUPDF_SOURCE_DIR:-}" ]; then
  echo "PDBG_MUPDF_SOURCE_DIR is not set." >&2
  echo "Run: sh scripts/setup-mupdf.sh && . third_party/mupdf.env" >&2
  exit 1
fi

MUTOOL="${PDBG_MUTOOL_PATH:-$PDBG_MUPDF_SOURCE_DIR/build/release/mutool}"
if [ ! -x "$MUTOOL" ]; then
  echo "mutool not found at $MUTOOL" >&2
  echo "build it with: make build=release build/release/mutool" >&2
  echo "or run: sh scripts/setup-mupdf.sh && . third_party/mupdf.env" >&2
  echo "or set PDBG_MUTOOL_PATH=/path/to/mutool" >&2
  exit 1
fi
export PDBG_MUTOOL_PATH="$MUTOOL"

cargo fmt --check
sh scripts/test_fz_try_gate.sh

cargo test -p pdbg-shim --no-default-features --features real-mupdf
cargo test -p pdbg-core --no-default-features --features real-mupdf
cargo test -p pdbg-app --no-default-features --features gui,real-mupdf

cargo clippy -p pdbg-shim --no-default-features --features real-mupdf -- -D warnings
cargo clippy -p pdbg-core --no-default-features --features real-mupdf -- -D warnings
cargo clippy -p pdbg-app --no-default-features --features gui,real-mupdf -- -D warnings

cargo run -p pdbg-app --no-default-features --features real-mupdf -- --pdf fixtures/synthetic/minimal.pdf

sh scripts/run_m0_local_gate.sh
