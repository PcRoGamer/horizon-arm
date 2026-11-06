package dev.horizon.android;

import android.app.AlertDialog;
import android.content.Intent;
import android.content.SharedPreferences;
import android.graphics.Bitmap;
import android.graphics.BitmapFactory;
import android.graphics.LinearGradient;
import android.graphics.Shader;
import android.graphics.Canvas;
import android.graphics.Paint;
import android.graphics.Path;
import android.graphics.Rect;
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

import java.io.File;

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
        scheme = Math.max(0, Math.min(3, prefs.getInt("scheme", 2)));
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
            // Horizon's native first-run bootstrap populates this cache. Reuse
            // the same verified Mojang asset tree, never a second downloader.
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
        // Reusable draw rectangles avoid dozens of allocations per touch frame.
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

        // Twelve original 24px controller glyphs, shared with Rust.
        private final int[][] PIXEL_GLYPHS = {
    {0x000000, 0x000000, 0x000000, 0x0FFFF8, 0x1FFFF8, 0x180018, 0x180018, 0x180018, 0x180018, 0x188898, 0x180018, 0x180018, 0x180018, 0x180018, 0x180018, 0x1E7FF8, 0x1EFFF8, 0x07C000, 0x0F8000, 0x0F0000, 0x0E0000, 0x0C0000, 0x000000, 0x000000},
    {0x000000, 0x000000, 0x000000, 0x0781E0, 0x0781E0, 0x0781E0, 0x0781E0, 0x0781E0, 0x0781E0, 0x0781E0, 0x0781E0, 0x0781E0, 0x0781E0, 0x0781E0, 0x0781E0, 0x0781E0, 0x0781E0, 0x0781E0, 0x0781E0, 0x0781E0, 0x0781E0, 0x000000, 0x000000, 0x000000},
    {0x000000, 0x000000, 0x0003C0, 0x0003C0, 0x0003C0, 0x0003C0, 0x000000, 0x000400, 0x3C0E00, 0x3C7E00, 0x00FFC0, 0x00FFE0, 0x787FE0, 0x787E00, 0x00FF80, 0x00EFC0, 0x19E3E0, 0x1BC0F0, 0x038070, 0x018078, 0x000038, 0x000030, 0x000000, 0x000000},
    {0x000000, 0x000000, 0x000000, 0x001800, 0x003C00, 0x007E00, 0x00FF00, 0x01FF80, 0x03FFC0, 0x07FFE0, 0x0FFFF0, 0x1FFFF8, 0x00FF00, 0x00FF00, 0x00FF00, 0x00FF00, 0x00FF00, 0x00FF00, 0x000000, 0x000000, 0x000000, 0x1FFFF8, 0x000000, 0x000000},
    {0x000000, 0x000000, 0x000000, 0x0003C0, 0x0003C0, 0x0003C0, 0x0003C0, 0x000000, 0x000600, 0x001F00, 0x003F80, 0x00FFE0, 0x01FBE0, 0x01F000, 0x03FE00, 0x07FFF0, 0x0FBFF0, 0x0FC7F0, 0x0FE000, 0x07E000, 0x006000, 0x3FFFFC, 0x000000, 0x000000},
    {0x000000, 0x000000, 0x000030, 0x000078, 0x0000F8, 0x0001F8, 0x0003F0, 0x0007E0, 0x000FC0, 0x001F80, 0x003F00, 0x01FE00, 0x01FC00, 0x00F800, 0x01F000, 0x03F800, 0x07DC00, 0x0F8800, 0x1F0000, 0x3E0000, 0x3C0000, 0x180000, 0x000000, 0x000000},
    {0x000000, 0x001800, 0x007E00, 0x00F780, 0x03C3C0, 0x0F00F0, 0x3C003C, 0x3C003C, 0x3F007C, 0x3781EC, 0x31E78C, 0x307E0C, 0x301C0C, 0x30180C, 0x30180C, 0x30180C, 0x30180C, 0x3C183C, 0x0F18F8, 0x07D9E0, 0x01FF80, 0x007E00, 0x001800, 0x000000},
    {0x000000, 0x000000, 0x000000, 0x000800, 0x001800, 0x003C00, 0x003E00, 0x007E00, 0x00FF00, 0x00FF80, 0x01FF80, 0x03FFC0, 0x03FFE0, 0x07FFE0, 0x0FFFF0, 0x0FFFF8, 0x1FFFF8, 0x3FFFFC, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000},
    {0x000000, 0x000000, 0x040000, 0x070000, 0x078000, 0x07E000, 0x07F000, 0x07FC00, 0x07FF00, 0x07FF80, 0x07FFE0, 0x07FFF8, 0x07FFF8, 0x07FFE0, 0x07FFC0, 0x07FF00, 0x07FC00, 0x07F800, 0x07E000, 0x07C000, 0x070000, 0x040000, 0x000000, 0x000000},
    {0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x3FFFFC, 0x1FFFF8, 0x1FFFF8, 0x0FFFF0, 0x07FFE0, 0x07FFE0, 0x03FFC0, 0x01FF80, 0x01FF80, 0x00FF00, 0x007E00, 0x007E00, 0x003C00, 0x001800, 0x001800, 0x000000, 0x000000, 0x000000},
    {0x000000, 0x000000, 0x000020, 0x000060, 0x0001E0, 0x0007E0, 0x000FE0, 0x003FE0, 0x007FE0, 0x01FFE0, 0x07FFE0, 0x0FFFE0, 0x1FFFE0, 0x07FFE0, 0x01FFE0, 0x00FFE0, 0x003FE0, 0x001FE0, 0x0007E0, 0x0001E0, 0x0000E0, 0x000020, 0x000000, 0x000000},
    {0x001800, 0x007E00, 0x01E780, 0x0781E0, 0x1E0078, 0x38001E, 0x78001E, 0x7E007E, 0x6781E6, 0x61E786, 0x607E06, 0x601806, 0x601806, 0x601806, 0x601806, 0x601806, 0x601806, 0x601806, 0x78181E, 0x1E1878, 0x0799E0, 0x01FF80, 0x007E00, 0x001800}
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
                case "perspective": return 11;
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



        /**
         * One restrained, Bedrock-inspired control material shared with the
         * native Rust HUD preview. GUI surfaces, never block textures.
         * Alpha comes from the user-controlled HUD-opacity setting.
         */
        private void chromeButton(Canvas c, float r, boolean pressed, boolean sprintLocked) {
            float bezel = Math.max(1.2f, r * .054f);
            float corner = Math.max(3f, r * .255f);
            float shift = pressed ? Math.max(1f, r * .025f) : 0f;
            c.save();
            c.translate(0f, shift);
            float l = -r, t = -r, right = r, bottom = r;
            // Lifted silhouette, rounded charcoal outline and cool metallic rim.
            color(alpha(0xFF111A24, 1.15f), Paint.Style.FILL, 0f);
            c.drawRoundRect(l, t + bezel * .66f, right, bottom + bezel * .66f,
                    corner, corner, paint);
            color(alpha(0xFF293746, 1.25f), Paint.Style.FILL, 0f);
            c.drawRoundRect(l, t, right, bottom, corner, corner, paint);
            color(alpha(0xFFDCE7F1, 1.12f), Paint.Style.FILL, 0f);
            c.drawRoundRect(l + bezel, t + bezel * .82f,
                    right - bezel, bottom - bezel * 1.02f,
                    Math.max(2f, corner - bezel), Math.max(2f, corner - bezel), paint);
            float faceInset = bezel * 1.75f;
            float faceCorner = Math.max(2f, corner - faceInset);
            color(0xFFFFFFFF, Paint.Style.FILL, 0f);
            int top = alpha(sprintLocked ? 0xFFC1EACB
                    : pressed ? 0xFFA2B1BE : 0xFFDDE9F4, .99f);
            int base = alpha(sprintLocked ? 0xFF7CA68F
                    : pressed ? 0xFF778999 : 0xFF9EADC2, .99f);
            paint.setShader(new android.graphics.LinearGradient(0f, t + faceInset, 0f, bottom - faceInset,
                    top, base, android.graphics.Shader.TileMode.CLAMP));
            c.drawRoundRect(l + faceInset, t + faceInset,
                    right - faceInset, bottom - faceInset,
                    faceCorner, faceCorner, paint);
            paint.setShader(null);
            // Narrow high-specular top reflection and dark lower seat.
            color(alpha(0xFFF6FCFF, pressed ? .28f : .95f),
                    Paint.Style.STROKE, Math.max(1f, bezel * .48f));
            paint.setAntiAlias(true);
            c.drawRoundRect(l + faceInset + bezel * .36f,
                    t + faceInset + bezel * .33f,
                    right - faceInset - bezel * .36f,
                    bottom - faceInset - bezel * .58f,
                    Math.max(1f, faceCorner - bezel * .3f),
                    Math.max(1f, faceCorner - bezel * .3f), paint);
            color(alpha(0xFF405062, .68f), Paint.Style.STROKE,
                    Math.max(1f, bezel * .36f));
            c.drawRoundRect(l + bezel * .55f, t + bezel * .55f,
                    right - bezel * .55f, bottom - bezel * .55f,
                    corner - bezel * .45f, corner - bezel * .45f, paint);
            c.restore();
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
            c.translate(Math.round(x), Math.round(y) + (pressed ? Math.max(1f, r * .028f) : 0f));
            chromeButton(c, r, pressed, name.equals("sprint") && pressed);
            glyph(c, name, r);
            c.restore();
        }

        // A quiet recessed neutral centre joins four arrow keys into one
        // familiar D-pad cluster. It draws no icon and owns no touch action.
        private void dpadHub(Canvas c, float cx, float cy, float r) {
            float corner = Math.max(3f, r * .22f);
            color(alpha(0xFF283748, .65f), Paint.Style.FILL, 0f);
            c.drawRoundRect(cx - r, cy - r, cx + r, cy + r,
                    corner, corner, paint);
            float edge = Math.max(1.2f, r * .048f);
            color(alpha(0xFFA8BBD0, .70f), Paint.Style.FILL, 0f);
            c.drawRoundRect(cx - r + edge, cy - r + edge,
                    cx + r - edge, cy + r - edge,
                    corner - edge, corner - edge, paint);
            color(alpha(0xFF8DA0B6, .56f), Paint.Style.FILL, 0f);
            c.drawRoundRect(cx - r + edge * 1.8f, cy - r + edge * 1.8f,
                    cx + r - edge * 1.8f, cy + r - edge * 1.8f,
                    corner - edge * 1.5f, corner - edge * 1.5f, paint);
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
            // Soft, slate-toned movement pad, consistent with the right controls.
            color(alpha(0xFF1B2B39, .54f), Paint.Style.FILL, 0f);
            c.drawCircle(x, y, r, paint);
            color(alpha(0xFFB8C9D8, 1.15f), Paint.Style.STROKE, Math.max(2f, r * .055f));
            c.drawCircle(x, y, r * .91f, paint);
            color(alpha(0xFF6D8398, .65f), Paint.Style.STROKE, Math.max(1f, r * .016f));
            c.drawCircle(x, y, r * .78f, paint);
            float tx = x + joyX * r * .66f;
            float ty = y - joyY * r * .66f;
            color(alpha(0xFF1C2B3B, 1.20f), Paint.Style.FILL, 0f);
            c.drawCircle(tx, ty + r * .04f, r * .40f, paint);
            color(alpha(0xFFDAE5EF, 1.22f), Paint.Style.FILL, 0f);
            c.drawCircle(tx, ty, r * .34f, paint);
            color(alpha(0xFF8193A8, .80f), Paint.Style.STROKE, Math.max(1f, r * .035f));
            c.drawCircle(tx, ty, r * .33f, paint);
        }

        // The D-pad adapts to tablet widths without colliding with the genuine
        // raised Rust hotbar. Shared constants are validated in Python and Rust.
        private float[] dpadGeometry() {
            float w = getWidth(), h = getHeight();
            float r = .052f * h * scale;
            float off = .112f * h * scale;
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
                dpadHub(canvas, cx, cy, r);
                String[] directions = {"up", "right", "down", "left"};
                float[][] vectors = {{0, -1}, {1, 0}, {0, 1}, {-1, 0}};
                for (int i = 0; i < 4; ++i) {
                    float xx = cx + vectors[i][0] * off;
                    float yy = cy + vectors[i][1] * off;
                    control(canvas, xx / getWidth(), yy / h, r / (h * scale), directions[i]);
                }
                // Four-way movement is kept separate from action keys.
                // Sprint and crouch belong with the right-hand action cluster.
                control(canvas, 1f - ((scheme & 1) == 1 ? .40f : .29f) / aspect,
                        (scheme & 1) == 1 ? .45f : .55f, .035f, "sprint");
                control(canvas, 1f - .290f / aspect, .73f, .040f, "sneak");
            }
            control(canvas, 1f - .133f / aspect, .72f, .066f, "jump");
            if ((scheme & 1) == 1) {
                control(canvas, 1f - .270f / aspect, .56f, .042f, "hit");
                control(canvas, 1f - .145f / aspect, .52f, .042f, "use");
            }
            // Inventory is the genuine Rust hotbar end-cap, not a floating key.
            float topStep = .084f * scale;
            control(canvas, .5f + (.050f - topStep) / aspect, .058f, .036f, "perspective");
            control(canvas, .5f + .050f / aspect, .058f, .036f, "chat");
            control(canvas, .5f + (.050f + topStep) / aspect, .058f, .036f, "pause");
        }
    }
}
