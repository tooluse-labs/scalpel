#!/bin/sh
set -eu

cargo test -p pdbg-contract-tests fake_shim_operation_surface_uses_c_accessors_and_registry
cargo test -p pdbg-contract-tests serialized_node_id_contract_is_stable_and_token_free
cargo test -p pdbg-contract-tests egress_contract_escapes_pdf_controlled_text
cargo test -p pdbg-core decoded_stream_limit_returns_limit_error_before_buffer_materialization
cargo test -p pdbg-core c_invoked_rust_callback_catches_panic_before_returning_to_c
cargo test -p pdbg-core multiple_sessions_drive_shared_store_from_worker_threads
