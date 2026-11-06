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
    @Test public void sourceDerivedCanvasUsesWholeGuiButtonAndPressedState() throws Exception {
        // These are synthetic interface fixtures, not Mojang-owned assets.
        // Verify the actual versioned cache lookup and that the *centre*
        // of a button is supplied by the GUI sprite rather than a block.
        File dir = new File(RuntimeEnvironment.getApplication().getFilesDir(),
                ".local/horizon-client/texture-fixture/assets/minecraft/textures/gui/sprites/widget");
        assertTrue(dir.isDirectory() || dir.mkdirs());
        File normalFile = new File(dir, "button.png");
        File pressedFile = new File(dir, "button_highlighted.png");
        for (int variant = 0; variant < 2; variant++) {
            Bitmap sprite = Bitmap.createBitmap(200, 20, Bitmap.Config.ARGB_8888);
            int centre = variant == 0 ? Color.rgb(184, 187, 188)
                    : Color.rgb(121, 198, 151);
            int border = variant == 0 ? Color.rgb(53, 58, 62)
                    : Color.rgb(36, 75, 48);
            for (int y = 0; y < 20; y++) {
                for (int x = 0; x < 200; x++) {
                    boolean edge = x < 3 || y < 3 || x >= 197 || y >= 17;
                    sprite.setPixel(x, y, edge ? border : centre);
                }
            }
            File target = variant == 0 ? normalFile : pressedFile;
            try (FileOutputStream output = new FileOutputStream(target)) {
                assertTrue(sprite.compress(Bitmap.CompressFormat.PNG, 100, output));
            }
            sprite.recycle();
        }

        HudPreviewOverlay overlay = new HudPreviewOverlay(RuntimeEnvironment.getApplication());
        overlay.menu = false;
        overlay.scheme = 2;
        overlay.scale = 1.0f;
        overlay.opacity = 100;
        assertTrue("Horizon must find its prepared GUI button resources",
                overlay.refreshButtonSprites());
        assertFalse("Button bitmaps must remain cached", overlay.refreshButtonSprites());
        java.lang.reflect.Field frameField = HudPreviewOverlay.class.getDeclaredField("buttonFrame");
        frameField.setAccessible(true);
        assertNotNull("Normal GUI button surface must load", frameField.get(overlay));
        java.lang.reflect.Field highlightField = HudPreviewOverlay.class.getDeclaredField("highlightedFrame");
        highlightField.setAccessible(true);
        assertNotNull("Highlighted GUI button surface must load", highlightField.get(overlay));

        int width = 1200, height = 630;
        overlay.measure(View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
                View.MeasureSpec.makeMeasureSpec(height, View.MeasureSpec.EXACTLY));
        overlay.layout(0, 0, width, height);
        Bitmap rendered = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888);
        rendered.eraseColor(Color.TRANSPARENT);
        overlay.draw(new Canvas(rendered));
        // The D-pad UP key's inner top-left area has no glyph or border.
        final int x = 118, y = 345;
        int resting = rendered.getPixel(x, y);
        assertEquals("Whole normal sprite must paint the button centre",
                Color.rgb(184, 187, 188), resting);
        File output = new File("../../artifacts/hud-native-skia/dpad-gui-widget-FIXTURE-1200x630.png");
        assertTrue(output.getParentFile().isDirectory() || output.getParentFile().mkdirs());
        try (FileOutputStream png = new FileOutputStream(output)) {
            assertTrue(rendered.compress(Bitmap.CompressFormat.PNG, 100, png));
        }
        rendered.recycle();
        assertTrue(output.length() > 1000);

        // D-pad UP is now held. The actual game state chooses the pressed
        // GUI sprite, changing the same untouched centre pixel.
        overlay.joyY = 0.8f;
        Bitmap pressed = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888);
        pressed.eraseColor(Color.TRANSPARENT);
        overlay.draw(new Canvas(pressed));
        assertEquals("Pressed state must use native highlighted button centre",
                Color.rgb(121, 198, 151), pressed.getPixel(x, y));
        pressed.recycle();
        assertTrue(normalFile.delete());
        assertTrue(pressedFile.delete());
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
