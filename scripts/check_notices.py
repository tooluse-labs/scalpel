#!/usr/bin/env python3
"""Verify that NOTICES covers the M0 workspace and MuPDF placeholder."""

from __future__ import annotations

import pathlib
import re
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
LOCK = ROOT / "Cargo.lock"
NOTICES = ROOT / "NOTICES"

PACKAGE = re.compile(r'^name = "([^"]+)"$', re.M)


def main() -> int:
    notices = NOTICES.read_text(encoding="utf-8")
    lock = LOCK.read_text(encoding="utf-8")
    package_names = sorted(set(PACKAGE.findall(lock)))

    missing: list[str] = []
    for needle in ["AGPL-3.0-only", "MuPDF", "real-mupdf"]:
        if needle not in notices:
            missing.append(needle)

    for package in package_names:
        if package not in notices:
            missing.append(package)

    for item in missing:
        print(f"NOTICES missing: {item}")
    return 1 if missing else 0


if __name__ == "__main__":
    raise SystemExit(main())
