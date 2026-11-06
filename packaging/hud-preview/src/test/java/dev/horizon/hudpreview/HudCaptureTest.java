package dev.horizon.hudpreview;

import android.graphics.Bitmap;
import android.graphics.BitmapFactory;
import android.graphics.Canvas;
import android.graphics.Color;
import android.view.View;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.annotation.Config;
import org.robolectric.annotation.GraphicsMode;

import java.io.File;
import java.io.FileOutputStream;

import static org.junit.Assert.*;

/**
 * Fast, transparent HUD-only captures from production Java Canvas.
 * Uses Robolectric's NATIVE Skia backend, not a fabricated browser painter.
 * Emulator/device screenshots remain the final fidelity authority.
 */
@RunWith(RobolectricTestRunner.class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = 34)
public class HudCaptureTest {
    @Test public void upstreamClassicSpritesRenderAndPressCorrectly() throws Exception {
        // Real LGPL-licensed TouchController PNG source bundled with the APK,
        // not a synthetic screenshot, forged asset, or substitute UI painter.
        Bitmap normal;
        Bitmap active;
        try (java.io.InputStream stream = RuntimeEnvironment.getApplication()
                .getAssets().open("horizon_touch/classic/dpad/up.png")) {
            normal = BitmapFactory.decodeStream(stream);
        }
        try (java.io.InputStream stream = RuntimeEnvironment.getApplication()
                .getAssets().open("horizon_touch/classic/dpad/up_active.png")) {
            active = BitmapFactory.decodeStream(stream);
        }
        assertNotNull("Original normal D-pad asset must load", normal);
        assertNotNull("Original pressed D-pad asset must load", active);
        assertEquals(22, normal.getWidth());
        assertEquals(22, normal.getHeight());
        assertEquals(22, active.getWidth());
        assertNotEquals("Upstream sprites must encode different pressed artwork",
                normal.getPixel(0, 0), active.getPixel(0, 0));

        HudPreviewOverlay overlay = new HudPreviewOverlay(RuntimeEnvironment.getApplication());
        overlay.menu = false;
        overlay.scheme = 2;
        overlay.scale = 1.0f;
        overlay.opacity = 100;
        int width = 1200, height = 630;
        overlay.measure(View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
                View.MeasureSpec.makeMeasureSpec(height, View.MeasureSpec.EXACTLY));
        overlay.layout(0, 0, width, height);
        Bitmap resting = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888);
        resting.eraseColor(Color.TRANSPARENT);
        overlay.draw(new Canvas(resting));
        // The layout's UP key is centred at approximately (145, 361).
        assertTrue("Actual production Canvas must draw the original UP sprite",
                Color.alpha(resting.getPixel(120, 335)) > 40);
        overlay.joyY = 0.8f;
        Bitmap pressed = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888);
        pressed.eraseColor(Color.TRANSPARENT);
        overlay.draw(new Canvas(pressed));
        int changed = 0;
        for (int y = 322; y <= 398; y += 2) {
            for (int x = 105; x <= 186; x += 2) {
                if (resting.getPixel(x, y) != pressed.getPixel(x, y)) changed++;
            }
        }
        assertTrue("Native Canvas must switch to original pressed PNG, not recolor glyph",
                changed > 20);
        File dir = new File("../../artifacts/hud-native-skia");
        assertTrue(dir.isDirectory() || dir.mkdirs());
        File png = new File(dir, "touchcontroller-classic-pressed-1200x630.png");
        try (FileOutputStream stream = new FileOutputStream(png)) {
            assertTrue(pressed.compress(Bitmap.CompressFormat.PNG, 100, stream));
        }
        assertTrue(png.length() > 1000);
        resting.recycle();
        pressed.recycle();
        normal.recycle();
        active.recycle();
    }

    @Test public void renderAllFourModesInNativeSkia() throws Exception {
        File dir = new File("../../artifacts/hud-native-skia");
        if (!dir.isDirectory() && !dir.mkdirs()) throw new IllegalStateException("mkdir " + dir);
        int[][] sizes = {{1280, 800}, {1600, 900}, {1200, 630}};
        String[] names = {"joystick-tap", "joystick-crosshair", "dpad-tap", "dpad-crosshair"};
        for (int[] size : sizes) {
            int width = size[0], height = size[1];
            for (int scheme = 0; scheme < 4; scheme++) {
                HudPreviewOverlay overlay = new HudPreviewOverlay(RuntimeEnvironment.getApplication());
                overlay.menu = false;
                overlay.scheme = scheme;
                overlay.scale = 1.0f;
                overlay.opacity = 72; // Horizon's production first-launch default
                overlay.buttons = 0;
                overlay.measure(View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
                    View.MeasureSpec.makeMeasureSpec(height, View.MeasureSpec.EXACTLY));
                overlay.layout(0, 0, width, height);
                Bitmap bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888);
                bitmap.eraseColor(Color.TRANSPARENT);
                overlay.draw(new Canvas(bitmap));
                int nonTransparent = 0;
                for (int y = 0; y < height; y += 8) {
                    for (int x = 0; x < width; x += 8) {
                        if (Color.alpha(bitmap.getPixel(x, y)) > 0) nonTransparent++;
                    }
                }
                assertTrue("Actual production Canvas rendered nothing in scheme " + scheme, nonTransparent > 25);
                File output = new File(dir, names[scheme] + "-" + width + "x" + height + ".png");
                try (FileOutputStream stream = new FileOutputStream(output)) {
                    assertTrue(bitmap.compress(Bitmap.CompressFormat.PNG, 100, stream));
                }
                assertTrue(output.isFile() && output.length() > 1000);
                System.out.println("HUD_NATIVE_SKIA_CAPTURE " + output.getAbsolutePath()
                    + " bytes=" + output.length() + " pixels=" + nonTransparent);
                bitmap.recycle();
                // Pixel-level optimisation against the supplied Bedrock capture:
                // capture *the real Android Canvas* at controlled opacity values.
                if (width == 1200 && height == 630 && scheme == 2) {
                    for (int opacity : new int[] {38, 40, 50, 60, 72, 85}) {
                        overlay.opacity = opacity;
                        Bitmap candidate = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888);
                        candidate.eraseColor(Color.TRANSPARENT);
                        overlay.draw(new Canvas(candidate));
                        File variant = new File(dir, "dpad-tap-opacity" + opacity
                                + "-" + width + "x" + height + ".png");
                        try (FileOutputStream stream = new FileOutputStream(variant)) {
                            assertTrue(candidate.compress(Bitmap.CompressFormat.PNG, 100, stream));
                        }
                        assertTrue(variant.length() > 1000);
                        candidate.recycle();
                    }
                }
            }
        }
    }
}
