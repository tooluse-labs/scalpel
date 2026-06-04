#!/bin/sh
set -eu

cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
python3 scripts/check_pdbg_shim_abi_snapshot.py
python3 scripts/check_notices.py
sh scripts/test_fz_try_gate.sh
sh scripts/run_m0_fuzz_smoke.sh
