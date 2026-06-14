#!/bin/sh
set -eu

MUPDF_SOURCE_DIR="${SCALPEL_MUPDF_SOURCE_DIR:-}"
if [ -z "$MUPDF_SOURCE_DIR" ]; then
  echo "SCALPEL_MUPDF_SOURCE_DIR is not set." >&2
  echo "Run: sh scripts/setup-mupdf.sh && . third_party/mupdf.env" >&2
  exit 1
fi

MUTOOL="${SCALPEL_MUTOOL_PATH:-$MUPDF_SOURCE_DIR/build/release/mutool}"
if [ ! -x "$MUTOOL" ]; then
  echo "mutool not found at $MUTOOL" >&2
  echo "build it with: make build=release build/release/mutool" >&2
  echo "or run: sh scripts/setup-mupdf.sh && . third_party/mupdf.env" >&2
  echo "or set SCALPEL_MUTOOL_PATH=/path/to/mutool" >&2
  exit 1
fi
export SCALPEL_MUPDF_SOURCE_DIR="$MUPDF_SOURCE_DIR"
export SCALPEL_MUTOOL_PATH="$MUTOOL"

cargo fmt --check
sh scripts/test_fz_try_gate.sh

cargo test -p scalpel-shim --no-default-features --features real-mupdf
cargo test -p scalpel-core --no-default-features --features real-mupdf
cargo test -p scalpel-app --no-default-features --features gui,real-mupdf

cargo clippy -p scalpel-shim --no-default-features --features real-mupdf -- -D warnings
cargo clippy -p scalpel-core --no-default-features --features real-mupdf -- -D warnings
cargo clippy -p scalpel-app --no-default-features --features gui,real-mupdf -- -D warnings

cargo run -p scalpel-app --no-default-features --features real-mupdf -- --pdf fixtures/synthetic/minimal.pdf

sh scripts/run_m0_local_gate.sh
