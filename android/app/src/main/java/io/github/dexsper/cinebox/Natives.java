package io.github.dexsper.cinebox;

/** Calls into the Rust app (crates/cinebox-android/src/natives.rs). Safe from any thread. */
final class Natives {
    private Natives() {}

    /** {@code command} is one of {@code MediaSessionController.COMMAND_*}. */
    static native void onMediaCommand(int command, double seconds);

    static native void onSpeechListening();

    static native void onSpeechPartial(String text);

    static native void onSpeechResult(String text);

    static native void onSpeechEnded();

    /** The whole field after the keyboard's edit; {@code start} and {@code end} count code points. */
    static native void onTextEdited(String text, int start, int end);

    static native void onEditorAction();

    static native void onKeyboardHidden();

    /** The video player has something new to show. */
    static native void onPlayerChanged();
}
