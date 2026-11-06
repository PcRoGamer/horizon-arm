package dev.horizon.hudpreview;

import android.graphics.Bitmap;
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
    @Test public void chromeButtonsHaveRoundedCornersAndPressedFeedback() throws Exception {
        HudPreviewOverlay overlay = new HudPreviewOverlay(RuntimeEnvironment.getApplication());
        overlay.menu = false;
        overlay.scheme = 2; // Four-key D-pad, the new default.
        overlay.scale = 1.0f;
        overlay.opacity = 100;
        int width = 1200, height = 630;
        overlay.measure(View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
                View.MeasureSpec.makeMeasureSpec(height, View.MeasureSpec.EXACTLY));
        overlay.layout(0, 0, width, height);
        Bitmap resting = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888);
        resting.eraseColor(Color.TRANSPARENT);
        overlay.draw(new Canvas(resting));
        assertEquals("The D-pad UP key must have rounded, transparent corners",
                0, Color.alpha(resting.getPixel(104, 319)));
        assertTrue("Its chrome face must be visible",
                Color.alpha(resting.getPixel(118, 345)) > 50);
        java.util.HashSet<Integer> shades = new java.util.HashSet<>();
        for (int y = 338; y < 354; y += 2) {
            for (int x = 116; x < 130; x += 2) {
                if (Color.alpha(resting.getPixel(x, y)) > 30) {
                    shades.add(resting.getPixel(x, y));
                }
            }
        }
        assertTrue("Button surface must have shading and depth",
                shades.size() >= 3);
        File dir = new File("../../artifacts/hud-native-skia");
        assertTrue(dir.isDirectory() || dir.mkdirs());
        File output = new File(dir, "dpad-chrome-FIXTURE-1200x630.png");
        try (FileOutputStream png = new FileOutputStream(output)) {
            assertTrue(resting.compress(Bitmap.CompressFormat.PNG, 100, png));
        }
        assertTrue(output.length() > 1000);
        overlay.joyY = 0.8f;
        Bitmap pressed = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888);
        pressed.eraseColor(Color.TRANSPARENT);
        overlay.draw(new Canvas(pressed));
        assertNotEquals("Pressed touch control must have responsive shading",
                resting.getPixel(118, 345), pressed.getPixel(118, 345));
        resting.recycle();
        pressed.recycle();
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
