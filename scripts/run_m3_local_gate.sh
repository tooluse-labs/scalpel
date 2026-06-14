#!/bin/sh
set -eu

cargo test -p scalpel-core search::tests::
cargo test -p scalpel-core diagnostics::tests::
cargo test -p scalpel-core shim::tests::fake_text_options_enforce_character_and_block_limits
cargo test -p scalpel-contract-tests text_coordinate_normalization_golden_is_top_left_page_space
cargo test -p scalpel-app --features gui gui_object_search_navigates_headless_fake_hit
cargo test -p scalpel-app --features gui real_tree_search_hit_row_preserves_search_node
cargo test -p scalpel-app --features gui gui_text_search_runs_async_caches_and_selects_hit
cargo test -p scalpel-app --features gui diagnostics_model_includes_text_page_errors_and_filters_codes

sh scripts/run_m0_local_gate.sh
