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
    @Test public void sourceDerivedCanvasLoadsPreparedStoneWithoutAnotherDownloader() throws Exception {
        // Synthetic fixture checks the real texture-loading path without
        // placing copyrighted Mojang PNGs into the repository or APK.
        File dir = new File(RuntimeEnvironment.getApplication().getFilesDir(),
                ".local/horizon-client/texture-fixture/assets/minecraft/textures/block");
        assertTrue(dir.isDirectory() || dir.mkdirs());
        File file = new File(dir, "stone.png");
        Bitmap fixture = Bitmap.createBitmap(16, 16, Bitmap.Config.ARGB_8888);
        for (int y = 0; y < 16; y++) {
            for (int x = 0; x < 16; x++) {
                fixture.setPixel(x, y, ((x / 2 + y / 2) & 1) == 0
                        ? Color.rgb(52, 52, 52) : Color.rgb(206, 206, 206));
            }
        }
        try (FileOutputStream output = new FileOutputStream(file)) {
            assertTrue(fixture.compress(Bitmap.CompressFormat.PNG, 100, output));
        }
        fixture.recycle();

        HudPreviewOverlay overlay = new HudPreviewOverlay(RuntimeEnvironment.getApplication());
        overlay.menu = false;
        overlay.scheme = 2;
        overlay.scale = 1.0f;
        overlay.opacity = 100;
        assertTrue("Horizon must locate real prepared-resource paths", overlay.refreshStoneTexture());
        assertFalse("Texture must remain cached", overlay.refreshStoneTexture());

        int width = 1200, height = 630;
        overlay.measure(View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
                View.MeasureSpec.makeMeasureSpec(height, View.MeasureSpec.EXACTLY));
        overlay.layout(0, 0, width, height);
        Bitmap rendered = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888);
        overlay.draw(new Canvas(rendered));
        java.util.HashSet<Integer> visibleColors = new java.util.HashSet<>();
        // The upper-left interior of the D-pad's UP button contains no glyph.
        for (int y = 342; y <= 355; y += 2) {
            for (int x = 118; x <= 135; x += 2) {
                int color = rendered.getPixel(x, y);
                if (Color.alpha(color) > 0) visibleColors.add(color);
            }
        }
        assertTrue("Stone must be textured, not a flat coloured rectangle",
                visibleColors.size() >= 2);
        File output = new File("../../artifacts/hud-native-skia/dpad-stone-FIXTURE-1200x630.png");
        assertTrue(output.getParentFile().isDirectory() || output.getParentFile().mkdirs());
        try (FileOutputStream png = new FileOutputStream(output)) {
            assertTrue(rendered.compress(Bitmap.CompressFormat.PNG, 100, png));
        }
        rendered.recycle();
        assertTrue(output.length() > 1000);
        // Avoid a synthetic fixture being accidentally picked up by gameplay.
        assertTrue(file.delete());
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
