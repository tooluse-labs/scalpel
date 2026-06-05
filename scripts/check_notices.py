#!/usr/bin/env python3
"""Verify that NOTICES covers the default M0 resolve graph and MuPDF placeholder."""

from __future__ import annotations

import json
import pathlib
import subprocess
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
NOTICES = ROOT / "NOTICES"


def main() -> int:
    notices = NOTICES.read_text(encoding="utf-8")
    package_names = default_resolve_package_names()

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


def default_resolve_package_names() -> list[str]:
    metadata = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked"],
        cwd=ROOT,
        check=True,
        stdout=subprocess.PIPE,
        text=True,
    )
    parsed = json.loads(metadata.stdout)
    packages_by_id = {package["id"]: package["name"] for package in parsed["packages"]}
    nodes = parsed.get("resolve", {}).get("nodes", [])
    return sorted({packages_by_id[node["id"]] for node in nodes})


if __name__ == "__main__":
    raise SystemExit(main())
