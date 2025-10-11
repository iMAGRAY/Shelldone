#!/usr/bin/env python3
"""
Verify that architectural contracts (ADR files) credit imagray as owner.
"""
from __future__ import annotations

import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
ADR_DIR = ROOT / "docs" / "architecture" / "adr"
OWNER_SIGNATURES = {
    "imagray <magraytlinov@gmail.com>",
    "imagray `<magraytlinov@gmail.com>`",
}


def main() -> None:
    missing: list[str] = []
    for md in sorted(ADR_DIR.glob("*.md")):
        text = md.read_text(encoding="utf-8")
        if not any(sig in text for sig in OWNER_SIGNATURES):
            missing.append(md.relative_to(ROOT).as_posix())
    if missing:
        raise SystemExit(
            "[contracts] owner signature missing in: " + ", ".join(missing)
        )
    print("[contracts] ADR ownership verified")


if __name__ == "__main__":
    try:
        main()
    except SystemExit as exc:
        raise
    except Exception as exc:
        print(f"[contracts] unexpected error: {exc}", file=sys.stderr)
        raise SystemExit(1)
