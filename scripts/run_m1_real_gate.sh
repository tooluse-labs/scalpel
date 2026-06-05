#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
echo "scripts/run_m1_real_gate.sh is deprecated; use scripts/run_real_gate.sh" >&2
exec sh "$SCRIPT_DIR/run_real_gate.sh" "$@"
