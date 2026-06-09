#!/bin/sh
set -eu

cargo fmt --check
cargo clippy -p pdbg-shim -- -D warnings
cargo clippy -p pdbg-core -- -D warnings
cargo clippy -p pdbg-mcp -- -D warnings
cargo clippy -p pdbg-contract-tests -- -D warnings
cargo clippy -p pdbg-app --no-default-features --features gui -- -D warnings
cargo test -p pdbg-shim
cargo test -p pdbg-core
cargo test -p pdbg-mcp
cargo test -p pdbg-contract-tests
cargo test -p pdbg-app --no-default-features --features gui
cargo run -p pdbg-app --no-default-features --quiet
python3 scripts/check_pdbg_shim_abi_snapshot.py
python3 scripts/check_notices.py
sh scripts/test_fz_try_gate.sh
sh scripts/run_m0_fuzz_smoke.sh
