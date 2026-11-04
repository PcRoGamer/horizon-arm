#!/usr/bin/env python3
"""CI compatibility entrypoint for the full Android/Windows touch geometry contract."""
from pathlib import Path
import runpy

runpy.run_path(str(Path(__file__).with_name("test_touch_layout_geometry.py")), run_name="__main__")