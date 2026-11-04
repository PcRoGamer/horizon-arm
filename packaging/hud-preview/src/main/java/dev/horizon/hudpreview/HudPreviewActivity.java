package dev.horizon.hudpreview;

import android.app.Activity;
import android.content.Intent;
import android.graphics.Color;
import android.os.Bundle;
import android.view.View;
import android.widget.FrameLayout;

/**
 * Production-source Android Canvas overlay, rendered without the game.
 * This Activity has no GameActivity or JNI dependency.
 */
public final class HudPreviewActivity extends Activity {
    private HudPreviewOverlay overlay;

    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        getWindow().getDecorView().setSystemUiVisibility(
                View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
                | View.SYSTEM_UI_FLAG_FULLSCREEN
                | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
                | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
                | View.SYSTEM_UI_FLAG_LAYOUT_STABLE);
        FrameLayout root = new FrameLayout(this);
        root.setBackgroundColor(Color.rgb(24, 30, 34));
        overlay = new HudPreviewOverlay(this);
        overlay.setClickable(false);
        overlay.setFocusable(false);
        root.addView(overlay, new FrameLayout.LayoutParams(-1, -1));
        setContentView(root);
        update(getIntent());
    }

    @Override public void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        setIntent(intent);
        update(intent);
    }

    private void update(Intent intent) {
        overlay.menu = false;
        overlay.scheme = clamp(intent.getIntExtra("scheme", 1), 0, 3);
        overlay.scale = clamp(intent.getIntExtra("scale", 100), 80, 120) / 100f;
        overlay.opacity = clamp(intent.getIntExtra("opacity", 72), 35, 100);
        overlay.buttons = intent.getIntExtra("buttons", 0);
        overlay.joyX = clampFloat(intent.getFloatExtra("joy_x", 0f), -1f, 1f);
        overlay.joyY = clampFloat(intent.getFloatExtra("joy_y", 0f), -1f, 1f);
        overlay.invalidate();
    }

    private static int clamp(int n, int min, int max) {
        return Math.max(min, Math.min(max, n));
    }
    private static float clampFloat(float n, float min, float max) {
        return Math.max(min, Math.min(max, n));
    }
}
