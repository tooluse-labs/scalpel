# Required M0 Jobs

Status: M0 required-job manifest

The default branch is `main`.

Mark these GitHub Actions jobs as required for M0 branches:

- `contract`
- `c-asan-ubsan`
- `tsan`
- `fuzz-smoke`

The `contract` job runs the local M0 gate:

- `cargo fmt --check`
- `cargo clippy --workspace -- -D warnings`
- `cargo test --workspace`
- `python3 scripts/check_pdbg_shim_abi_snapshot.py`
- `python3 scripts/check_notices.py`
- `sh scripts/test_fz_try_gate.sh`
- `sh scripts/run_m0_fuzz_smoke.sh`

The sanitizer jobs are checked in as CI definitions. Local M0 verification may
run the stable local gate only; TSan requires nightly Rust and `rust-src`.
