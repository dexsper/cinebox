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

    static native void onComposingText(String text);

    static native void onCommitText(String text);

    static native void onDeleteSurrounding(int before, int after);

    static native void onEditorAction();

    static native void onKeyboardHidden();
}
