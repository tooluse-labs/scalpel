#!/usr/bin/env python3
"""Conservative M0 scanner for forbidden exits inside fz_try/fz_always blocks."""

from __future__ import annotations

import pathlib
import re
import sys


FORBIDDEN = re.compile(r"\b(return|goto)\b|\blongjmp\s*\(")
MACRO = re.compile(r"\bfz_(try|always)\s*\(")


def iter_c_files(args: list[str]) -> list[pathlib.Path]:
    files: list[pathlib.Path] = []
    for arg in args:
        path = pathlib.Path(arg)
        if path.is_dir():
            files.extend(path.rglob("*.c"))
            files.extend(path.rglob("*.h"))
        else:
            files.append(path)
    return sorted(set(files))


def scan_file(path: pathlib.Path) -> list[str]:
    violations: list[str] = []
    active = False
    pending_macro = False
    depth = 0

    for lineno, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not active and MACRO.search(line):
            pending_macro = True

        if pending_macro and "{" in line:
            active = True
            pending_macro = False
            depth = 0

        if active and FORBIDDEN.search(line):
            violations.append(f"{path}:{lineno}: forbidden exit inside fz_try/fz_always: {line.strip()}")

        if active:
            depth += line.count("{")
            depth -= line.count("}")
            if depth <= 0:
                active = False
                pending_macro = False

    return violations


def main() -> int:
    args = sys.argv[1:] or ["crates/pdbg-shim/c"]
    violations: list[str] = []
    for path in iter_c_files(args):
        violations.extend(scan_file(path))

    for violation in violations:
        print(violation)
    return 1 if violations else 0


if __name__ == "__main__":
    raise SystemExit(main())

