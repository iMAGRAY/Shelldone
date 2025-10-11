#!/usr/bin/env python3
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from scripts.lib.architecture_tool import main as architecture_main

if __name__ == "__main__":
    raise SystemExit(architecture_main(["check"]))
