#!/bin/sh
set -eu

python3 scripts/fz_try_gate.py scripts/fz_try_fixtures/good.c

if python3 scripts/fz_try_gate.py scripts/fz_try_fixtures/bad_return.c >/tmp/fz_try_gate_bad.out 2>&1; then
  echo "expected bad_return.c to fail fz_try gate" >&2
  exit 1
fi

grep -q "forbidden exit inside fz_try/fz_always" /tmp/fz_try_gate_bad.out

