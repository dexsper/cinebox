package io.github.dexsper.cinebox;

import android.os.Build;
import android.view.SoundEffectConstants;
import android.view.View;

/** The system's interface sounds, played the way framework views play them. */
final class UiSounds {
    /** Codes sent by the Rust side (device.rs `sound_code`). */
    static final int ACTIVATE = 0;
    static final int UP = 1;
    static final int DOWN = 2;
    static final int LEFT = 3;
    static final int RIGHT = 4;

    private UiSounds() {}

    /** On the UI thread; the system setting for interface sounds is honored downstream. */
    static void play(View view, int sound, boolean repeat) {
        if (sound == ACTIVATE) {
            view.playSoundEffect(SoundEffectConstants.CLICK);
            return;
        }

        int direction = focusDirection(sound);
        if (direction == 0) {
            return;
        }

        view.playSoundEffect(navigation(direction, repeat));
    }

    @SuppressWarnings("deprecation")
    private static int navigation(int direction, boolean repeat) {
        // API 33 varies the sound while a key is held.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            return SoundEffectConstants.getConstantForFocusDirection(direction, repeat);
        }

        return SoundEffectConstants.getContantForFocusDirection(direction);
    }

    private static int focusDirection(int sound) {
        switch (sound) {
            case UP:
                return View.FOCUS_UP;
            case DOWN:
                return View.FOCUS_DOWN;
            case LEFT:
                return View.FOCUS_LEFT;
            case RIGHT:
                return View.FOCUS_RIGHT;
            default:
                return 0;
        }
    }
}
