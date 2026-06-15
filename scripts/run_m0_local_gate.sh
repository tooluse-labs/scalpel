#!/bin/sh
set -eu

cargo fmt --check
cargo clippy -p scalpel-shim -- -D warnings
cargo clippy -p scalpel-core -- -D warnings
cargo clippy -p scalpel-mcp -- -D warnings
cargo clippy -p scalpel-contract-tests -- -D warnings
cargo clippy -p scalpel-app --no-default-features --features gui -- -D warnings
cargo test -p scalpel-shim
cargo test -p scalpel-core
cargo test -p scalpel-mcp
cargo test -p scalpel-contract-tests
cargo test -p scalpel-app --no-default-features --features gui
cargo run -p scalpel-app --no-default-features --quiet -- --render-max-dimension 1
python3 scripts/check_pdbg_shim_abi_snapshot.py
python3 scripts/check_notices.py
sh scripts/test_fz_try_gate.sh
sh scripts/run_m0_fuzz_smoke.sh
