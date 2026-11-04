package dev.horizon.android;

import android.app.AlertDialog;
import android.content.Intent;
import android.content.SharedPreferences;
import android.graphics.Canvas;
import android.graphics.Paint;
import android.graphics.Path;
import android.graphics.RectF;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.os.Bundle;
import android.view.Gravity;
import android.view.View;
import android.widget.Button;
import android.widget.FrameLayout;
import android.widget.LinearLayout;
import android.widget.SeekBar;
import android.widget.TextView;

import com.google.androidgamesdk.GameActivity;

/**
 * Bedrock-inspired touch surface. The Canvas overlay never consumes gameplay
 * touches: horizon-winit is the single owner of input and hit testing.
 * Positions, radii and scale are checked against its Rust mirror in CI.
 */
public final class HorizonActivity extends GameActivity {
    private static final String PREFS = "horizon_touch";
    private static final String[] SCHEMES = {
            "Joystick + tap to interact", "Joystick + aim crosshair",
            "D-pad + tap to interact", "D-pad + aim crosshair"
    };
    private TouchOverlay overlay;
    private Button profileButton;
    private Button controlsButton;
    private boolean applied;
    private int scheme;
    private int controlScale;
    private int opacity;

    public static native boolean nativeTouchMenu();
    public static native void nativeSetControls(int scheme, int scale);
    public static native int nativeTouchAxes();
    public static native int nativeTouchButtons();

    private int dp(float x) {
        return Math.round(x * getResources().getDisplayMetrics().density);
    }

    private void immersive() {
        getWindow().getDecorView().setSystemUiVisibility(
                View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
                        | View.SYSTEM_UI_FLAG_FULLSCREEN
                        | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                        | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
                        | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
                        | View.SYSTEM_UI_FLAG_LAYOUT_STABLE);
    }

    private GradientDrawable panel(int color) {
        GradientDrawable bg = new GradientDrawable();
        bg.setColor(color);
        bg.setCornerRadius(dp(3));
        bg.setStroke(dp(2), 0xFF141414);
        return bg;
    }

    private Button topButton(String label, boolean accent) {
        Button button = new Button(this);
        button.setText(label);
        button.setAllCaps(false);
        button.setTypeface(Typeface.create("sans-serif-condensed", Typeface.BOLD));
        button.setTextSize(13);
        button.setTextColor(accent ? 0xFFFFFFFF : 0xFFF0F0F0);
        button.setBackground(panel(accent ? 0xFF387E32 : 0xFF575757));
        button.setElevation(dp(2));
        button.setMinWidth(dp(112));
        button.setMinHeight(dp(42));
        button.setPadding(dp(15), dp(7), dp(15), dp(7));
        return button;
    }

    @Override protected void onCreate(Bundle state) {
        super.onCreate(state);
        immersive();
        SharedPreferences prefs = getSharedPreferences(PREFS, MODE_PRIVATE);
        scheme = Math.max(0, Math.min(3, prefs.getInt("scheme", 0)));
        controlScale = Math.max(80, Math.min(120, prefs.getInt("scale", 100)));
        opacity = Math.max(35, Math.min(100, prefs.getInt("opacity", 72)));

        overlay = new TouchOverlay();
        overlay.setClickable(false);
        overlay.setFocusable(false);
        overlay.setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_NO);
        addContentView(overlay, new FrameLayout.LayoutParams(-1, -1));

        int margin = dp(13);
        profileButton = topButton("PROFILE", false);
        profileButton.setOnClickListener(v ->
                startActivity(new Intent(this, ProfileActivity.class)));
        FrameLayout.LayoutParams profile = new FrameLayout.LayoutParams(-2, dp(44),
                Gravity.TOP | Gravity.END);
        profile.setMargins(margin, margin, margin, margin);
        addContentView(profileButton, profile);

        controlsButton = topButton("CONTROLS", true);
        controlsButton.setOnClickListener(v -> showSchemeDialog());
        FrameLayout.LayoutParams control = new FrameLayout.LayoutParams(-2, dp(44),
                Gravity.TOP | Gravity.START);
        control.setMargins(margin, margin, margin, margin);
        addContentView(controlsButton, control);
        overlay.scheme = scheme;
        overlay.opacity = opacity;
        overlay.scale = controlScale / 100f;
    }

    @Override protected void onResume() {
        super.onResume();
        immersive();
        applied = false; // Android may have recreated the native surface.
        getWindow().getDecorView().removeCallbacks(poll);
        getWindow().getDecorView().post(poll);
    }

    @Override protected void onPause() {
        getWindow().getDecorView().removeCallbacks(poll);
        super.onPause();
    }

    @Override protected void onDestroy() {
        getWindow().getDecorView().removeCallbacks(poll);
        super.onDestroy();
    }

    private void saveControls() {
        getSharedPreferences(PREFS, MODE_PRIVATE).edit()
                .putInt("scheme", scheme)
                .putInt("scale", controlScale)
                .putInt("opacity", opacity)
                .apply();
        overlay.scheme = scheme;
        overlay.opacity = opacity;
        overlay.scale = controlScale / 100f;
        overlay.invalidate();
        applied = false;
        syncNative();
    }

    private void syncNative() {
        if (applied) return;
        try {
            nativeSetControls(scheme, controlScale);
            applied = true;
        } catch (UnsatisfiedLinkError ignored) {
            // Native library may finish loading after Activity.onCreate.
        }
    }

    private boolean inMenu() {
        try { return nativeTouchMenu(); }
        catch (UnsatisfiedLinkError ignored) { return true; }
    }

    private void showSchemeDialog() {
        AlertDialog dialog = new AlertDialog.Builder(this)
                .setTitle("Touch control mode")
                .setSingleChoiceItems(SCHEMES, scheme, (d, selected) -> {
                    scheme = selected;
                    saveControls();
                    d.dismiss();
                })
                .setNeutralButton("Size & opacity", (d, w) -> showAppearanceDialog())
                .setNegativeButton("Cancel", null)
                .create();
        dialog.setOnShowListener(d -> dialog.getWindow().getDecorView()
                .setBackgroundColor(0xFF303030));
        dialog.show();
    }

    private TextView settingLabel(String title) {
        TextView label = new TextView(this);
        label.setText(title);
        label.setTextColor(0xFFEEEEEE);
        label.setTextSize(15);
        label.setPadding(0, dp(14), 0, dp(3));
        return label;
    }

    private void showAppearanceDialog() {
        LinearLayout content = new LinearLayout(this);
        content.setOrientation(LinearLayout.VERTICAL);
        content.setPadding(dp(23), dp(5), dp(23), dp(12));

        TextView sizeLabel = settingLabel("Button size: " + controlScale + "%");
        SeekBar size = new SeekBar(this);
        size.setMax(40);
        size.setProgress(controlScale - 80);
        size.setOnSeekBarChangeListener(new Progress(sizeLabel, "Button size: ", 80));
        content.addView(sizeLabel);
        content.addView(size);

        TextView opacityLabel = settingLabel("Opacity: " + opacity + "%");
        SeekBar alpha = new SeekBar(this);
        alpha.setMax(65);
        alpha.setProgress(opacity - 35);
        alpha.setOnSeekBarChangeListener(new Progress(opacityLabel, "Opacity: ", 35));
        content.addView(opacityLabel);
        content.addView(alpha);

        AlertDialog dialog = new AlertDialog.Builder(this)
                .setTitle("Customize touch controls")
                .setView(content)
                .setNegativeButton("Cancel", null)
                .setNeutralButton("Reset", (d, w) -> {
                    controlScale = 100;
                    opacity = 72;
                    saveControls();
                })
                .setPositiveButton("Apply", (d, w) -> {
                    controlScale = 80 + size.getProgress();
                    opacity = 35 + alpha.getProgress();
                    saveControls();
                }).create();
        dialog.show();
        dialog.getWindow().getDecorView().setBackgroundColor(0xFF303030);
    }

    private static final class Progress implements SeekBar.OnSeekBarChangeListener {
        private final TextView label;
        private final String prefix;
        private final int minimum;
        Progress(TextView label, String prefix, int minimum) {
            this.label = label;
            this.prefix = prefix;
            this.minimum = minimum;
        }
        @Override public void onProgressChanged(SeekBar s, int value, boolean fromUser) {
            label.setText(prefix + (minimum + value) + "%");
        }
        @Override public void onStartTrackingTouch(SeekBar s) {}
        @Override public void onStopTrackingTouch(SeekBar s) {}
    }

    private final Runnable poll = new Runnable() {
        @Override public void run() {
            syncNative();
            boolean menu = inMenu();
            int bits = 0;
            float x = 0f, y = 0f;
            if (!menu) {
                try {
                    bits = nativeTouchButtons();
                    int axes = nativeTouchAxes();
                    x = (short) (axes >>> 16) / 32767f;
                    y = (short) axes / 32767f;
                } catch (UnsatisfiedLinkError ignored) {}
            }
            int visibility = menu ? View.VISIBLE : View.GONE;
            if (profileButton.getVisibility() != visibility) profileButton.setVisibility(visibility);
            if (controlsButton.getVisibility() != visibility) controlsButton.setVisibility(visibility);
            if (overlay.menu != menu || overlay.buttons != bits
                    || Math.abs(overlay.joyX - x) > .012f
                    || Math.abs(overlay.joyY - y) > .012f) {
                overlay.menu = menu;
                overlay.buttons = bits;
                overlay.joyX = x;
                overlay.joyY = y;
                overlay.invalidate();
            }
            getWindow().getDecorView().postDelayed(this, menu ? 160 : 33);
        }
    };

    /** Non-intercepting, hardware-accelerated vector artwork. */
    private final class TouchOverlay extends View {
        private final Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG);
        private final Path path = new Path();
        boolean menu = true;
        int scheme = 0;
        int opacity = 72;
        float scale = 1f;
        int buttons = 0;
        float joyX, joyY;

        TouchOverlay() {
            super(HorizonActivity.this);
            setLayerType(View.LAYER_TYPE_HARDWARE, null);
        }

        private void color(int argb, Paint.Style style, float stroke) {
            paint.reset();
            paint.setAntiAlias(true);
            paint.setColor(argb);
            paint.setStyle(style);
            paint.setStrokeWidth(stroke);
            paint.setStrokeCap(Paint.Cap.SQUARE);
            paint.setStrokeJoin(Paint.Join.MITER);
        }

        private int alpha(int rgb, float factor) {
            return ((Math.round(255 * opacity / 100f * factor) & 0xFF) << 24)
                    | (rgb & 0xFFFFFF);
        }

        private void glyph(Canvas c, String name, float r) {
            c.save();
            c.scale(r * .71f, r * .71f);
            color(alpha(0xFF202427, 1f), Paint.Style.STROKE, .17f);
            switch (name) {
                case "sprint":
                    c.drawLine(-.69f, -.42f, -.12f, 0f, paint);
                    c.drawLine(-.12f, 0f, -.69f, .42f, paint);
                    c.drawLine(-.05f, -.42f, .52f, 0f, paint);
                    c.drawLine(.52f, 0f, -.05f, .42f, paint);
                    c.drawLine(-.67f, -.67f, .50f, -.67f, paint);
                    break;
                case "jump":
                    c.drawLine(-.65f, -.05f, 0f, -.66f, paint);
                    c.drawLine(0f, -.66f, .65f, -.05f, paint);
                    c.drawLine(0f, -.50f, 0f, .52f, paint);
                    c.drawLine(-.55f, .68f, .55f, .68f, paint);
                    break;
                case "sneak":
                    c.drawLine(-.65f, .02f, 0f, .62f, paint);
                    c.drawLine(0f, .62f, .65f, .02f, paint);
                    c.drawLine(0f, -.62f, 0f, .43f, paint);
                    c.drawLine(-.55f, -.70f, .55f, -.70f, paint);
                    break;
                case "hit":
                    c.drawLine(-.48f, .54f, .46f, -.42f, paint);
                    c.drawLine(.14f, -.48f, .65f, -.70f, paint);
                    c.drawLine(.46f, -.42f, .65f, -.70f, paint);
                    c.drawLine(-.66f, .08f, -.12f, .61f, paint);
                    c.drawLine(-.60f, .73f, -.30f, .40f, paint);
                    break;
                case "use":
                    path.reset();
                    path.moveTo(0f, -.72f);
                    path.lineTo(.66f, -.39f);
                    path.lineTo(.66f, .37f);
                    path.lineTo(0f, .73f);
                    path.lineTo(-.66f, .37f);
                    path.lineTo(-.66f, -.39f);
                    path.close();
                    c.drawPath(path, paint);
                    c.drawLine(-.66f, -.39f, 0f, -.07f, paint);
                    c.drawLine(.66f, -.39f, 0f, -.07f, paint);
                    c.drawLine(0f, -.07f, 0f, .70f, paint);
                    break;
                case "inventory":
                    c.drawRect(-.70f, -.53f, .70f, .59f, paint);
                    c.drawLine(-.69f, -.10f, .69f, -.10f, paint);
                    c.drawLine(-.22f, -.50f, -.22f, .58f, paint);
                    c.drawLine(.24f, -.50f, .24f, .58f, paint);
                    break;
                case "pause":
                    color(alpha(0xFF202427, 1f), Paint.Style.FILL, 0);
                    c.drawRect(-.54f, -.70f, -.12f, .70f, paint);
                    c.drawRect(.12f, -.70f, .54f, .70f, paint);
                    break;
                case "up": arrow(c, 0);
                    break;
                case "right": arrow(c, 90);
                    break;
                case "down": arrow(c, 180);
                    break;
                case "left": arrow(c, 270);
                    break;
                default: break;
            }
            c.restore();
        }

        private void arrow(Canvas c, int degrees) {
            c.save();
            c.rotate(degrees);
            path.reset();
            path.moveTo(0f, -.70f);
            path.lineTo(.66f, .06f);
            path.lineTo(.28f, .06f);
            path.lineTo(.28f, .65f);
            path.lineTo(-.28f, .65f);
            path.lineTo(-.28f, .06f);
            path.lineTo(-.66f, .06f);
            path.close();
            color(alpha(0xFF202427, 1f), Paint.Style.FILL, 0);
            c.drawPath(path, paint);
            c.restore();
        }

        private void control(Canvas c, float nx, float ny, float radius, String name) {
            float x = nx * getWidth();
            float y = ny * getHeight();
            float r = radius * getHeight() * scale;
            boolean pressed = (name.equals("jump") && (buttons & 1) != 0)
                    || (name.equals("sneak") && (buttons & (1 << 11)) != 0)
                    || (name.equals("sprint") && (buttons & (1 << 10)) != 0)
                    || (name.equals("inventory") && (buttons & (1 << 3)) != 0)
                    || (name.equals("pause") && (buttons & (1 << 9)) != 0)
                    || (name.equals("hit") && (buttons & (1 << 25)) != 0)
                    || (name.equals("use") && (buttons & (1 << 24)) != 0)
                    || (name.equals("up") && joyY > .28f)
                    || (name.equals("right") && joyX > .28f)
                    || (name.equals("down") && joyY < -.28f)
                    || (name.equals("left") && joyX < -.28f);
            c.save();
            c.translate(x, y + (pressed ? r * .035f : 0));
            // Flat, compact, high-contrast Minecraft-style touch key.
            float corner = r * .035f;
            float edge = r * .075f;
            color(alpha(0xFF101618, .82f), Paint.Style.FILL, 0);
            c.drawRoundRect(new RectF(-r, -r + edge, r, r + edge),
                    corner, corner, paint);
            color(alpha(pressed ? 0xFF88949B : 0xFFC4C8CC, .89f),
                    Paint.Style.FILL, 0);
            c.drawRoundRect(new RectF(-r + edge, -r + edge,
                    r - edge, r - edge), corner * .55f, corner * .55f, paint);
            color(alpha(pressed ? 0xFF171C20 : 0xFF454A4D, 1f),
                    Paint.Style.STROKE, r * .047f);
            c.drawRect(-r + edge * 1.3f, -r + edge * 1.3f,
                    r - edge * 1.3f, r - edge * 1.3f, paint);
            glyph(c, name, r);
            c.restore();
        }

        private void joystick(Canvas c) {
            float h = getHeight();
            float x = .11f * getWidth();
            float y = .718f * h;
            float r = .092f * h * scale;
            color(alpha(0xFF33383C, .38f), Paint.Style.FILL, 0);
            c.drawCircle(x, y, r, paint);
            color(alpha(0xFFDCE1E5, .78f), Paint.Style.STROKE, r * .055f);
            c.drawCircle(x, y, r * .93f, paint);
            color(alpha(0xFFFFFFFF, .22f), Paint.Style.STROKE, r * .025f);
            c.drawCircle(x, y, r * .67f, paint);
            float tx = x + joyX * r * .70f;
            float ty = y - joyY * r * .70f;
            color(alpha(0xFF080808, .86f), Paint.Style.FILL, 0);
            c.drawCircle(tx, ty + r * .032f, r * .42f, paint);
            color(alpha(0xFFC6C9CC, .95f), Paint.Style.FILL, 0);
            c.drawCircle(tx, ty, r * .36f, paint);
            color(alpha(0xFFF1F1F1, .81f), Paint.Style.STROKE, r * .035f);
            c.drawCircle(tx, ty, r * .34f, paint);
        }

        @Override protected void onDraw(Canvas canvas) {
            super.onDraw(canvas);
            if (menu || getWidth() < 1 || getHeight() < 1) return;
            if (scheme < 2) {
                joystick(canvas);
            } else {
                // D-pad's four independent visual keys share one virtual-stick
                // hit region; Rust quantizes movement into directional axes.
                float unit = .068f * getHeight() * scale;
                // A height-based inset prevents clipping on 4:3 tablets
                // when controls are scaled up to 120%.
                float centerX = Math.max(.11f * getWidth(), .205f * getHeight());
                float centerY = .718f * getHeight();
                String[] directions = {"up", "right", "down", "left"};
                float[][] vectors = {{0, -1}, {1, 0}, {0, 1}, {-1, 0}};
                for (int i = 0; i < 4; ++i) {
                    float xx = centerX + vectors[i][0] * unit * 1.48f;
                    float yy = centerY + vectors[i][1] * unit * 1.48f;
                    control(canvas, xx / getWidth(), yy / getHeight(),
                            .043f, directions[i]);
                }
            }
            // Physical-height offsets keep the right-hand cluster coherent
            // on tablets, tall phones, and ultra-wide landscape displays.
            float aspect = getWidth() / (float) getHeight();
            control(canvas, 1f - .160f / aspect, .77f, .051f, "jump");
            control(canvas, 1f - .290f / aspect, .77f, .040f, "sneak");
            if ((scheme & 1) == 1) {
                control(canvas, 1f - .270f / aspect, .56f, .042f, "hit");
                control(canvas, 1f - .145f / aspect, .52f, .042f, "use");
            }
            control(canvas, .11f + .190f / aspect, .60f, .035f, "sprint");
            // Inventory sits beyond the hotbar, not over the held item.
            int guiScale = Math.max(1, Math.min(getWidth() / 320, getHeight() / 240));
            float inventoryX = Math.max(.735f * getWidth(),
                    .5f * getWidth() + 91f * guiScale
                    + .030f * getHeight() * scale
                    + Math.max(14f, .015f * getHeight()));
            control(canvas, inventoryX / getWidth(), .92f, .030f, "inventory");
            control(canvas, .50f, .072f, .032f, "pause");
        }
    }
}
