package dev.horizon.android;

import android.content.Intent;
import android.graphics.Canvas;
import android.graphics.Paint;
import android.graphics.Typeface;
import android.os.Bundle;
import android.view.Gravity;
import android.view.View;
import android.widget.Button;
import android.widget.FrameLayout;

import com.google.androidgamesdk.GameActivity;

/**
 * The Java shell stays deliberately thin: GameActivity owns the game surface
 * and input, while this class adds only a non-intercepting visual touch overlay
 * and an in-app Profile button. Actual touch semantics live in horizon-winit so
 * they remain tied to Horizon's existing controller actions.
 */
public final class HorizonActivity extends GameActivity {
    private TouchOverlay touchOverlay;
    private Button profileButton;

    public static native boolean nativeTouchMenu();

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        getWindow().getDecorView().setSystemUiVisibility(
                View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
                        | View.SYSTEM_UI_FLAG_FULLSCREEN
                        | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                        | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
                        | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
                        | View.SYSTEM_UI_FLAG_LAYOUT_STABLE);

        touchOverlay = new TouchOverlay();
        touchOverlay.setClickable(false);
        touchOverlay.setFocusable(false);
        touchOverlay.setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_NO);
        addContentView(touchOverlay, new FrameLayout.LayoutParams(
                FrameLayout.LayoutParams.MATCH_PARENT,
                FrameLayout.LayoutParams.MATCH_PARENT));

        profileButton = new Button(this);
        profileButton.setText("Profile");
        profileButton.setAllCaps(false);
        profileButton.setOnClickListener(v -> startActivity(new Intent(this, ProfileActivity.class)));
        FrameLayout.LayoutParams profile = new FrameLayout.LayoutParams(
                FrameLayout.LayoutParams.WRAP_CONTENT,
                FrameLayout.LayoutParams.WRAP_CONTENT,
                Gravity.TOP | Gravity.END);
        int margin = Math.round(12 * getResources().getDisplayMetrics().density);
        profile.setMargins(margin, margin, margin, margin);
        addContentView(profileButton, profile);

        getWindow().getDecorView().post(menuPoll);
    }

    @Override
    protected void onDestroy() {
        getWindow().getDecorView().removeCallbacks(menuPoll);
        super.onDestroy();
    }

    private boolean nativeMenu() {
        try {
            return nativeTouchMenu();
        } catch (UnsatisfiedLinkError ignored) {
            return true;
        }
    }

    private final Runnable menuPoll = new Runnable() {
        @Override
        public void run() {
            boolean menu = nativeMenu();
            if (profileButton != null) {
                profileButton.setVisibility(menu ? View.VISIBLE : View.GONE);
            }
            if (touchOverlay != null) {
                touchOverlay.menu = menu;
                touchOverlay.invalidate();
            }
            getWindow().getDecorView().postDelayed(this, 100);
        }
    };

    /** Visuals only. Returning false to hit testing leaves every finger for GameActivity/winit. */
    private final class TouchOverlay extends View {
        private final Paint fill = new Paint(Paint.ANTI_ALIAS_FLAG);
        private final Paint line = new Paint(Paint.ANTI_ALIAS_FLAG);
        private final Paint text = new Paint(Paint.ANTI_ALIAS_FLAG);
        boolean menu = true;

        TouchOverlay() {
            super(HorizonActivity.this);
            fill.setColor(0x44333333);
            line.setColor(0x88FFFFFF);
            line.setStyle(Paint.Style.STROKE);
            line.setStrokeWidth(2 * getResources().getDisplayMetrics().density);
            text.setColor(0xCCFFFFFF);
            text.setTextAlign(Paint.Align.CENTER);
            text.setTypeface(Typeface.create(Typeface.DEFAULT, Typeface.BOLD));
        }

        private void circle(Canvas c, float nx, float ny, float radius, String label) {
            float h = getHeight();
            float x = nx * getWidth();
            float y = ny * h;
            float r = radius * h;
            c.drawCircle(x, y, r, fill);
            c.drawCircle(x, y, r, line);
            if (label != null) {
                text.setTextSize(Math.max(12, r * 0.42f));
                Paint.FontMetrics fm = text.getFontMetrics();
                c.drawText(label, x, y - (fm.ascent + fm.descent) / 2f, text);
            }
        }

        @Override
        protected void onDraw(Canvas canvas) {
            super.onDraw(canvas);
            if (menu || getWidth() == 0 || getHeight() == 0) {
                return;
            }
            // Fixed Bedrock-like joystick and action cluster. Geometry is kept
            // byte-for-byte in sync with horizon-winit's Android touch regions.
            circle(canvas, 0.14f, 0.75f, 0.16f, null);
            circle(canvas, 0.14f, 0.75f, 0.055f, null);
            circle(canvas, 0.90f, 0.74f, 0.085f, "JUMP");
            circle(canvas, 0.79f, 0.84f, 0.065f, "SNEAK");
            circle(canvas, 0.80f, 0.58f, 0.080f, "HIT");
            circle(canvas, 0.92f, 0.51f, 0.075f, "USE");
            circle(canvas, 0.72f, 0.94f, 0.055f, "INV");
            circle(canvas, 0.50f, 0.06f, 0.055f, "II");
        }
    }
}
