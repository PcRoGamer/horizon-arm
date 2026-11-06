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
import android.widget.CheckBox;
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
    private boolean alwaysSprint;

    public static native boolean nativeTouchMenu();
    public static native void nativeSetControls(int scheme, int scale, boolean alwaysSprint);
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
        alwaysSprint = prefs.getBoolean("always_sprint", false);

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
                .putBoolean("always_sprint", alwaysSprint)
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
            nativeSetControls(scheme, controlScale, alwaysSprint);
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
                .setNeutralButton("HUD & sprint", (d, w) -> showAppearanceDialog())
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

        CheckBox sprint = new CheckBox(this);
        sprint.setText("Always sprint when moving forward");
        sprint.setTextColor(0xFFFFFFFF);
        sprint.setTextSize(15);
        sprint.setChecked(alwaysSprint);
        sprint.setPadding(0, dp(14), 0, dp(6));
        content.addView(sprint);
        TextView sprintHelp = settingLabel(
                "Sprint lock: tap the running button above the joystick to turn "
                + "sprint on or off without double-tapping movement.");
        sprintHelp.setTextColor(0xFFBBBBBB);
        content.addView(sprintHelp);

        AlertDialog dialog = new AlertDialog.Builder(this)
                .setTitle("Customize touch controls")
                .setView(content)
                .setNegativeButton("Cancel", null)
                .setNeutralButton("Reset", (d, w) -> {
                    controlScale = 100;
                    opacity = 35;
                    alwaysSprint = false;
                    saveControls();
                })
                .setPositiveButton("Apply", (d, w) -> {
                    controlScale = 80 + size.getProgress();
                    opacity = 35 + alpha.getProgress();
                    alwaysSprint = sprint.isChecked();
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
            return (Math.min(255, Math.max(0, Math.round(255 * opacity / 100f * factor))) << 24)
                    | (rgb & 0xFFFFFF);
        }

        // Original 24px-grid icons rasterized once into reusable Canvas paths.
        // Pixel masks are identical to the ARM64 Rust GPU preview.
        // Glyph source: identical 11x24 masks in Android Canvas and Rust native preview
        private final int[][] PIXEL_GLYPHS = {
    {0x000000, 0x000000, 0x000000, 0x000000, 0x0FFFF0, 0x1FFFF0, 0x180030, 0x180030, 0x180030, 0x1999F0, 0x1BFFF0, 0x1BFFF0, 0x1999F0, 0x180030, 0x180030, 0x1C7FF0, 0x1DFFF0, 0x0FC000, 0x0F8000, 0x0F0000, 0x0C0000, 0x000000, 0x000000, 0x000000},
    {0x000000, 0x000000, 0x000000, 0x000000, 0x03C3C0, 0x03C3C0, 0x03C3C0, 0x03C3C0, 0x03C3C0, 0x03C3C0, 0x03C3C0, 0x03C3C0, 0x03C3C0, 0x03C3C0, 0x03C3C0, 0x03C3C0, 0x03C3C0, 0x03C3C0, 0x03C3C0, 0x03C3C0, 0x000000, 0x000000, 0x000000, 0x000000},
    {0x000000, 0x000000, 0x000380, 0x000380, 0x0007C0, 0x000380, 0x3F8700, 0x3F8F80, 0x003FC0, 0x7F39F0, 0x7F70FC, 0x00707C, 0x00E000, 0x07E000, 0x0FF800, 0x0FFC00, 0x071E00, 0x0F0E00, 0x0E0E00, 0x1C0E00, 0x180C00, 0x000C00, 0x000000, 0x000000},
    {0x000000, 0x000000, 0x000000, 0x001800, 0x003C00, 0x007E00, 0x007E00, 0x00FF00, 0x01FF80, 0x03FFC0, 0x07FFE0, 0x007E00, 0x007E00, 0x007E00, 0x007E00, 0x007E00, 0x007E00, 0x007E00, 0x000000, 0x000000, 0x1FFFF8, 0x1FFFF8, 0x000000, 0x000000},
    {0x000000, 0x000000, 0x000000, 0x003C00, 0x003C00, 0x003C00, 0x003C00, 0x003C00, 0x003C00, 0x00FF00, 0x03FFC0, 0x03FFC0, 0x01FF80, 0x00FF00, 0x007E00, 0x003C00, 0x001800, 0x000000, 0x000000, 0x000000, 0x07FFE0, 0x07FFE0, 0x000000, 0x000000},
    {0x000000, 0x000000, 0x00000C, 0x00003C, 0x00007C, 0x00007C, 0x0000FC, 0x0001DC, 0x0003F8, 0x0007F8, 0x000FE0, 0x001C00, 0x183800, 0x1C7000, 0x0FE000, 0x07C000, 0x07C000, 0x07C000, 0x0FE000, 0x1C7000, 0x383000, 0x300000, 0x000000, 0x000000},
    {0x000000, 0x000000, 0x003C00, 0x00FF00, 0x01FF80, 0x07C3E0, 0x1F00F8, 0x1E0078, 0x1F00F8, 0x1FC3F8, 0x19FF98, 0x18FF18, 0x183C18, 0x181818, 0x181818, 0x181818, 0x1E1878, 0x1F18F8, 0x07DBE0, 0x01FF80, 0x007F00, 0x003C00, 0x000000, 0x000000},
    {0x000000, 0x000000, 0x000000, 0x001800, 0x003C00, 0x007E00, 0x007E00, 0x00FF00, 0x01FF80, 0x03FFC0, 0x07FFE0, 0x0FFFF0, 0x00FF00, 0x00FF00, 0x00FF00, 0x00FF00, 0x00FF00, 0x00FF00, 0x00FF00, 0x00FF00, 0x00FF00, 0x000000, 0x000000, 0x000000},
    {0x000000, 0x000000, 0x000000, 0x000000, 0x000800, 0x000C00, 0x000E00, 0x000F00, 0x1FFF80, 0x1FFFE0, 0x1FFFF0, 0x1FFFF8, 0x1FFFF8, 0x1FFFF0, 0x1FFFE0, 0x1FFF80, 0x000F00, 0x000E00, 0x000C00, 0x000800, 0x000000, 0x000000, 0x000000, 0x000000},
    {0x000000, 0x000000, 0x000000, 0x00FF00, 0x00FF00, 0x00FF00, 0x00FF00, 0x00FF00, 0x00FF00, 0x00FF00, 0x00FF00, 0x00FF00, 0x0FFFF0, 0x07FFE0, 0x03FFC0, 0x01FF80, 0x00FF00, 0x007E00, 0x007E00, 0x003C00, 0x001800, 0x000000, 0x000000, 0x000000},
    {0x000000, 0x000000, 0x000000, 0x000000, 0x001000, 0x003000, 0x007000, 0x00F000, 0x01FFF8, 0x07FFF8, 0x0FFFF8, 0x1FFFF8, 0x1FFFF8, 0x0FFFF8, 0x07FFF8, 0x01FFF8, 0x00F000, 0x007000, 0x003000, 0x001000, 0x000000, 0x000000, 0x000000, 0x000000}
        };
        private final Path[] pixelPaths = buildPixelPaths();

        private Path[] buildPixelPaths() {
            Path[] result = new Path[PIXEL_GLYPHS.length];
            for (int icon = 0; icon < result.length; icon++) {
                Path path = new Path();
                for (int y = 0; y < 24; y++) {
                    int bits = PIXEL_GLYPHS[icon][y];
                    int x = 0;
                    while (x < 24) {
                        if ((bits & (1 << (23 - x))) == 0) { x++; continue; }
                        int left = x;
                        do { x++; } while (x < 24 && (bits & (1 << (23-x))) != 0);
                        path.addRect(left, y, x, y + 1, Path.Direction.CW);
                    }
                }
                result[icon] = path;
            }
            return result;
        }

        private int pixelIcon(String name) {
            switch (name) {
                case "chat": return 0;
                case "pause": return 1;
                case "sprint": return 2;
                case "jump": return 3;
                case "sneak": return 4;
                case "hit": return 5;
                case "use": return 6;
                case "up": return 7;
                case "right": return 8;
                case "down": return 9;
                case "left": return 10;
                default: return -1;
            }
        }

        private void glyph(Canvas canvas, String name, float radius) {
            int index = pixelIcon(name);
            if (index < 0) return;
            canvas.save();
            // 24x24 pixel glyph fits inside the beveled key's face.
            canvas.scale(radius * .056f, radius * .056f);
            canvas.translate(-12f, -12f);
            color(alpha(0xFF111516, 1.9f), Paint.Style.FILL, 0);
            paint.setAntiAlias(false);
            canvas.drawPath(pixelPaths[index], paint);
            canvas.restore();
        }


        private void control(Canvas c, float nx, float ny, float radius, String name) {
            float x = nx * getWidth();
            float y = ny * getHeight();
            float r = radius * getHeight() * scale;
            boolean pressed = (name.equals("sprint")
                    && (buttons & ((1 << 26) | (1 << 27))) != 0)
                    || (name.equals("jump") && (buttons & 1) != 0)
                    || (name.equals("sneak") && (buttons & (1 << 11)) != 0)
                    || (name.equals("pause") && (buttons & (1 << 9)) != 0)
                    || (name.equals("hit") && (buttons & (1 << 25)) != 0)
                    || (name.equals("use") && (buttons & (1 << 24)) != 0)
                    || (name.equals("up") && joyY > .28f)
                    || (name.equals("right") && joyX > .28f)
                    || (name.equals("down") && joyY < -.28f)
                    || (name.equals("left") && joyX < -.28f);
            c.save();
            // Bedrock controls are translucent windowpanes, not solid concrete blocks.
            // The face is never painted over an opaque backing; only its 1-2px
            // frame, top reflection and bottom lip are separate layers.
            c.translate(Math.round(x), Math.round(y) + (pressed ? Math.max(1f, r * .028f) : 0f));
            float edge = Math.max(1.25f, Math.round(r * .047f));
            float inset = edge * .5f;
            color(alpha(0xFF151B1B, .90f), Paint.Style.STROKE, edge);
            paint.setAntiAlias(false);
            c.drawRect(-r + inset, -r + inset, r - inset, r - inset, paint);
            int face = pressed && name.equals("sprint") ? 0xFF97B39F
                    : pressed ? 0xFF7E8E8A : 0xFFAFB4AE;
            color(alpha(face, .76f), Paint.Style.FILL, 0f);
            paint.setAntiAlias(false);
            c.drawRect(-r + edge, -r + edge, r - edge, r - edge, paint);
            float lip = Math.max(1f, Math.round(r * .024f));
            color(alpha(0xFFE0E4DB, .24f), Paint.Style.FILL, 0f);
            paint.setAntiAlias(false);
            c.drawRect(-r + edge, -r + edge, r - edge, -r + edge + lip, paint);
            color(alpha(0xFF283231, .38f), Paint.Style.FILL, 0f);
            paint.setAntiAlias(false);
            c.drawRect(-r + edge, r - edge - lip, r - edge, r - edge, paint);
            glyph(c, name, r);
            c.restore();
        }

        private float joystickY() {
            float w = getWidth(), h = getHeight();
            float r = .092f * h * scale;
            float y = .705f * h;
            int gui = 1;
            while ((int)w / (gui + 1) >= 320 && (int)h / (gui + 1) >= 240) gui++;
            float gw = (float) Math.ceil(w / gui);
            float gh = (float) Math.ceil(h / gui);
            float barScale = Math.min(1.36f, (gw * .5f - 10f) / (91f + 20f));
            float barLeft = (gw * .5f - 91f * barScale) * gui;
            float barTop = (gh - 15f - 22f * barScale) * gui;
            if (.11f * w + r > barLeft - 9f)
                y = Math.min(y, barTop - 9f - r);
            return y;
        }

        private void joystick(Canvas c) {
            float h = getHeight();
            float x = .11f * getWidth();
            float y = joystickY();
            float r = .092f * h * scale;
            // A clear movement ring, without the unnecessary nested circles.
            color(alpha(0xFF13171B, .22f), Paint.Style.FILL, 0);
            c.drawCircle(x, y, r, paint);
            color(alpha(0xFFF4F7F8, 2.45f), Paint.Style.STROKE, Math.max(2f, r * .041f));
            c.drawCircle(x, y, r * .95f, paint);
            float tx = x + joyX * r * .70f;
            float ty = y - joyY * r * .70f;
            color(alpha(0xFF0C0F11, 1.80f), Paint.Style.FILL, 0);
            c.drawCircle(tx, ty + r * .032f, r * .42f, paint);
            color(alpha(0xFFE4E9EB, 2.15f), Paint.Style.FILL, 0);
            c.drawCircle(tx, ty, r * .36f, paint);
        }

        // The D-pad adapts to tablet widths without colliding with the genuine
        // raised Rust hotbar. Shared constants are validated in Python and Rust.
        private float[] dpadGeometry() {
            float w = getWidth(), h = getHeight();
            float r = .068f * h * scale;
            float off = .147f * h * scale;
            float cx = Math.max(.11f * w, .23f * h * scale);
            float cy = .72f * h;
            int gui = 1;
            while ((int)w / (gui + 1) >= 320 && (int)h / (gui + 1) >= 240) gui++;
            float guiW = (float) Math.ceil(w / gui);
            float guiH = (float) Math.ceil(h / gui);
            // Exactly the same 10-GUI-pixel hotbar edge clamp as Rust.
            float hotbarScale = Math.min(1.36f, (guiW * .5f - 10f) / (91f + 20f));
            float barLeft = (guiW * .5f - 91f * hotbarScale) * gui;
            float barTop = (guiH - 15f - 22f * hotbarScale) * gui;
            if (cx + off + r > barLeft - 9f) cy = Math.min(cy, barTop - 9f - r);
            if (cx + r > barLeft - 9f) cy = Math.min(cy, barTop - 9f - off - r);
            return new float[] {cx, cy, off, r};
        }

        @Override protected void onDraw(Canvas canvas) {
            super.onDraw(canvas);
            if (menu || getWidth() < 1 || getHeight() < 1) return;
            float h = getHeight();
            float aspect = getWidth() / h;
            if (scheme < 2) {
                joystick(canvas);
                control(canvas, 1f - .290f / aspect, .73f, .040f, "sneak");
                control(canvas, .11f + .190f / aspect, .57f, .035f, "sprint");
            } else {
                float[] dpad = dpadGeometry();
                float cx = dpad[0], cy = dpad[1], off = dpad[2], r = dpad[3];
                String[] directions = {"up", "right", "down", "left"};
                float[][] vectors = {{0, -1}, {1, 0}, {0, 1}, {-1, 0}};
                for (int i = 0; i < 4; ++i) {
                    float xx = cx + vectors[i][0] * off;
                    float yy = cy + vectors[i][1] * off;
                    control(canvas, xx / getWidth(), yy / h, r / (h * scale), directions[i]);
                }
                // Bedrock's fifth D-pad key: crouch occupies the neutral centre.
                control(canvas, cx / getWidth(), cy / h, r / (h * scale), "sneak");
                control(canvas, (cx + 1.55f * off) / getWidth(),
                        (cy - 1.45f * off) / h, .035f, "sprint");
            }
            control(canvas, 1f - .160f / aspect, .72f, .051f, "jump");
            if ((scheme & 1) == 1) {
                control(canvas, 1f - .270f / aspect, .56f, .042f, "hit");
                control(canvas, 1f - .145f / aspect, .52f, .042f, "use");
            }
            // Inventory is the genuine Rust hotbar end-cap, not a floating key.
            control(canvas, .5f - .045f / aspect, .045f, .028f, "chat");
            control(canvas, .5f + .045f / aspect, .045f, .028f, "pause");
        }
    }
}
