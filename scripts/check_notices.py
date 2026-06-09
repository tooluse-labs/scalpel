#!/usr/bin/env python3
"""Verify NOTICES coverage for workspace packages, MuPDF, and GUI fonts."""

from __future__ import annotations

import json
import pathlib
import subprocess
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
NOTICES = ROOT / "NOTICES"


def main() -> int:
    notices = NOTICES.read_text(encoding="utf-8")
    package_names = workspace_package_names()

    missing: list[str] = []
    for needle in ["AGPL-3.0-only", "MuPDF", "real-mupdf"]:
        if needle not in notices:
            missing.append(needle)

    for package in package_names:
        if package not in notices:
            missing.append(package)

    for asset in gui_font_assets():
        if not (ROOT / asset).is_file():
            missing.append(asset)
        if asset not in notices:
            missing.append(asset)

    for item in missing:
        print(f"NOTICES missing: {item}")
    return 1 if missing else 0


def workspace_package_names() -> list[str]:
    metadata = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked", "--no-deps"],
        cwd=ROOT,
        check=True,
        stdout=subprocess.PIPE,
        text=True,
    )
    parsed = json.loads(metadata.stdout)
    workspace_members = set(parsed.get("workspace_members", []))
    return sorted(
        package["name"]
        for package in parsed["packages"]
        if package["id"] in workspace_members
    )


def gui_font_assets() -> list[str]:
    return [
        "crates/pdbg-app/assets/fonts/InterVariable.ttf",
        "crates/pdbg-app/assets/fonts/JetBrainsMono-Regular.ttf",
        "crates/pdbg-app/assets/licenses/Inter-OFL.txt",
        "crates/pdbg-app/assets/licenses/JetBrainsMono-OFL.txt",
    ]


if __name__ == "__main__":
    raise SystemExit(main())
