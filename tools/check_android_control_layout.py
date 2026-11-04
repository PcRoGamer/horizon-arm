#!/usr/bin/env python3
"""Compatibility entry point for Android's visual/input geometry regression.

The canonical suite also verifies Windows ARM64 GPU preview parity, physical
safe areas, scale variants, tablet D-pad layouts, and hotbar clearance.
Run with: python3 tools/check_android_control_layout.py
"""
import test_touch_layout_geometry  # noqa: F401  (executes its assertions)
