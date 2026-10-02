package io.github.dexsper.cinebox;

import android.app.NativeActivity;
import android.content.pm.PackageManager;
import android.graphics.Bitmap;
import android.graphics.BitmapFactory;
import android.graphics.PixelFormat;
import android.os.Bundle;
import android.view.ViewGroup;
import android.view.WindowManager;

/**
 * The app's only activity. The Rust library draws and runs the app; the
 * public methods here are what it calls for things only Java can reach, from
 * its own thread (crates/cinebox-android/src/device.rs).
 */
public final class CineboxActivity extends NativeActivity {
    // NativeActivity opens the library by path, outside the class loader, so
    // the JVM would not find the native methods of Natives without this.
    static {
        System.loadLibrary("cinebox_android");
    }

    private MediaSessionController mediaSession;
    private TextInputView textInput;
    private SpeechInput speech;
    private VideoPlayer videoPlayer;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        // Ready before NativeActivity starts the Rust side, which may call in at once.
        mediaSession = new MediaSessionController(this);
        textInput = new TextInputView(this);
        speech = new SpeechInput(this);
        videoPlayer = new VideoPlayer(this);

        super.onCreate(savedInstanceState);
        addContentView(textInput, new ViewGroup.LayoutParams(1, 1));

        // The video plays in a layer below the window and shows through where
        // the app leaves the window clear.
        getWindow().setFormat(PixelFormat.TRANSLUCENT);
        videoPlayer.attachView();
    }

    @Override
    protected void onDestroy() {
        videoPlayer.release();
        mediaSession.release();
        speech.release();
        super.onDestroy();
    }

    public VideoPlayer videoPlayer() {
        return videoPlayer;
    }

    public void keepScreenOn(boolean on) {
        int flag = WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON;
        runOnUiThread(() -> {
            if (on) {
                getWindow().addFlags(flag);
                return;
            }

            getWindow().clearFlags(flag);
        });
    }

    public void playUiSound(int sound, boolean repeat) {
        runOnUiThread(() -> UiSounds.play(getWindow().getDecorView(), sound, repeat));
    }

    public void setMediaMetadata(String title, String subtitle, long durationMs, byte[] artwork) {
        Bitmap art = decode(artwork);
        runOnUiThread(() -> mediaSession.setMetadata(title, subtitle, durationMs, art));
    }

    public void setMediaPlayback(boolean playing, long positionMs, boolean canNext, boolean canPrevious) {
        runOnUiThread(() -> mediaSession.setPlayback(playing, positionMs, canNext, canPrevious));
    }

    public void clearMediaSession() {
        runOnUiThread(() -> mediaSession.clear());
    }

    public boolean isSpeechInputAvailable() {
        return SpeechInput.isAvailable(this);
    }

    public void startSpeechInput(String language) {
        runOnUiThread(() -> speech.start(language));
    }

    public void stopSpeechInput() {
        runOnUiThread(() -> speech.stop());
    }

    @Override
    public void onRequestPermissionsResult(int requestCode, String[] permissions, int[] results) {
        super.onRequestPermissionsResult(requestCode, permissions, results);
        if (requestCode != SpeechInput.PERMISSION_REQUEST) {
            return;
        }

        boolean granted = results.length > 0 && results[0] == PackageManager.PERMISSION_GRANTED;
        speech.onPermissionResult(granted);
    }

    public void startTextInput(int purpose, int action, String text, int start, int end) {
        runOnUiThread(() -> textInput.start(purpose, action, text, start, end));
    }

    public void updateTextInput(String text, int start, int end) {
        runOnUiThread(() -> textInput.update(text, start, end));
    }

    public void stopTextInput() {
        runOnUiThread(() -> textInput.stop());
    }

    private static Bitmap decode(byte[] image) {
        if (image == null) {
            return null;
        }

        return BitmapFactory.decodeByteArray(image, 0, image.length);
    }
}
