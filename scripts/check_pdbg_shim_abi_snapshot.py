#!/usr/bin/env python3
"""Drift check between frozen pdbg_shim.h and checked-in raw Rust bindings.

This M0 check intentionally avoids bindgen and MuPDF headers. It verifies the
load-bearing ABI facts that are already frozen: explicit enum discriminants and
exported pdbg_* function names.
"""

from __future__ import annotations

import pathlib
import re
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
HEADER = ROOT / "crates/pdbg-shim/include/pdbg_shim.h"
RAW = ROOT / "crates/pdbg-shim/src/raw.rs"

HEADER_ENUM = re.compile(r"typedef\s+enum\s+(\w+)\s*\{(?P<body>.*?)\}\s*\1\s*;", re.S)
RUST_ENUM = re.compile(r"pub\s+enum\s+(\w+)\s*\{(?P<body>.*?)\}", re.S)
VARIANT = re.compile(r"\b([A-Z][A-Z0-9_]+)\s*=\s*([0-9]+)")
HEADER_FN = re.compile(r"\b(pdbg_[a-zA-Z0-9_]+)\s*\([^;]*\)\s*;", re.S)
RUST_FN = re.compile(r"pub\s+fn\s+(pdbg_[a-zA-Z0-9_]+)\s*\(")


def enum_map(text: str, pattern: re.Pattern[str]) -> dict[str, dict[str, int]]:
    out: dict[str, dict[str, int]] = {}
    for match in pattern.finditer(text):
        name = match.group(1)
        body = match.group("body")
        out[name] = {variant: int(value) for variant, value in VARIANT.findall(body)}
    return out


def main() -> int:
    header = HEADER.read_text(encoding="utf-8")
    raw = RAW.read_text(encoding="utf-8")

    header_enums = enum_map(header, HEADER_ENUM)
    rust_enums = enum_map(raw, RUST_ENUM)
    errors: list[str] = []

    for name, expected in sorted(header_enums.items()):
        actual = rust_enums.get(name)
        if actual is None:
            errors.append(f"missing Rust enum: {name}")
        elif actual != expected:
            errors.append(f"enum drift for {name}: header={expected} raw={actual}")

    header_without_typedefs = re.sub(r"typedef\s+.*?;", "", header, flags=re.S)
    header_fns = set(HEADER_FN.findall(header_without_typedefs))
    rust_fns = set(RUST_FN.findall(raw))
    missing = sorted(header_fns - rust_fns)
    extra = sorted(rust_fns - header_fns)
    if missing:
        errors.append(f"missing Rust extern fns: {', '.join(missing)}")
    if extra:
        errors.append(f"extra Rust extern fns: {', '.join(extra)}")

    for error in errors:
        print(error)
    return 1 if errors else 0


if __name__ == "__main__":
    raise SystemExit(main())
