#!/usr/bin/env python3
"""Generate the HUD-only Android View from the actual production TouchOverlay.

No duplicated icon shapes, no rewritten drawing geometry. If production
structure changes, abort instead of silently rendering a misleading mockup.
"""
import argparse
import hashlib
from pathlib import Path

MARKER = "    private final class TouchOverlay extends View {"
CONSTRUCTOR = """        TouchOverlay() {
            super(HorizonActivity.this);
            setLayerType(View.LAYER_TYPE_HARDWARE, null);
        }"""
NEW_CONSTRUCTOR = """        HudPreviewOverlay(Context context) {
            super(context);
            setLayerType(View.LAYER_TYPE_HARDWARE, null);
        }"""
HEADER = """// AUTO-GENERATED. The real HorizonActivity.java contains the drawing code.
package dev.horizon.hudpreview;

import android.content.Context;
import android.graphics.Bitmap;
import android.graphics.BitmapFactory;
import android.graphics.Canvas;
import android.graphics.Paint;
import android.graphics.Path;
import android.graphics.Rect;
import android.graphics.RectF;
import android.view.View;
import java.io.File;

"""

def extract(source: str) -> str:
    assert source.count(MARKER) == 1, "Production TouchOverlay not found exactly once"
    start = source.index(MARKER)
    end = source.rfind("\n    }\n}")
    assert end > start and source[end:] == "\n    }\n}\n", "Production overlay no longer last class"
    body = source[start:end+6]
    assert body.count(CONSTRUCTOR) == 1, "Production constructor changed; review extractor"
    for token in ("controlSprite(String key)", "spritePath(String name, boolean pressed)",
                  "onDraw(Canvas canvas)", "joystick(Canvas c)", "control(Canvas c",
                  '"chat"', '"pause"', '"sprint"', "super.onDraw(canvas);"):
        assert token in body, "Missing production renderer primitive: " + token
    body = body.replace(MARKER, "public final class HudPreviewOverlay extends View {", 1)
    body = body.replace(CONSTRUCTOR, NEW_CONSTRUCTOR, 1)
    assert "HorizonActivity.this" not in body, "Outer Activity reference leaked"
    return HEADER + body + "\n"

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    generated = extract(args.source.read_text(encoding="utf8"))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(generated, encoding="utf8")
    print("Production Java Canvas overlay extracted:", args.output,
          hashlib.sha256(generated.encode()).hexdigest()[:16])

if __name__ == "__main__":
    main()
