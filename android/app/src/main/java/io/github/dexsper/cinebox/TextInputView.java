package io.github.dexsper.cinebox;

import android.content.Context;
import android.text.InputType;
import android.view.KeyEvent;
import android.view.View;
import android.view.inputmethod.BaseInputConnection;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;
import android.view.inputmethod.InputMethodManager;

/**
 * An invisible editor the soft keyboard can attach to. NativeActivity has no
 * text view of its own, so without this the keyboard would neither show nor
 * deliver text. The text itself lives in the Rust UI; this view only relays
 * what the keyboard does. UI thread only.
 */
final class TextInputView extends View {
    /** Codes sent by the Rust side (device.rs `purpose_code`, `action_code`). */
    static final int PURPOSE_TEXT = 0;
    static final int PURPOSE_URL = 1;
    static final int PURPOSE_SECRET = 2;
    static final int PURPOSE_VERBATIM = 3;
    static final int ACTION_DONE = 0;
    static final int ACTION_SEARCH = 1;

    private final InputMethodManager inputMethods;
    private int inputType = InputType.TYPE_CLASS_TEXT;
    private int imeAction = EditorInfo.IME_ACTION_DONE;
    private boolean editing;

    TextInputView(Context context) {
        super(context);
        inputMethods = context.getSystemService(InputMethodManager.class);
    }

    void start(int purpose, int action) {
        inputType = inputType(purpose);
        imeAction = action == ACTION_SEARCH ? EditorInfo.IME_ACTION_SEARCH : EditorInfo.IME_ACTION_DONE;
        editing = true;

        // Focusable only while typing: the system shows the keyboard on its own
        // for a focused editor when the window gains focus.
        setFocusable(true);
        setFocusableInTouchMode(true);
        requestFocus();
        inputMethods.restartInput(this);
        inputMethods.showSoftInput(this, 0);
    }

    void stop() {
        editing = false;
        inputMethods.hideSoftInputFromWindow(getWindowToken(), 0);
        clearFocus();
        setFocusable(false);
    }

    @Override
    public boolean onCheckIsTextEditor() {
        return editing;
    }

    @Override
    public InputConnection onCreateInputConnection(EditorInfo info) {
        info.inputType = inputType;
        // Fullscreen (extract) mode would show the keyboard's own empty copy of
        // the text: the real text is only in the Rust field.
        info.imeOptions = imeAction
                | EditorInfo.IME_FLAG_NO_FULLSCREEN
                | EditorInfo.IME_FLAG_NO_EXTRACT_UI;
        return new Relay(this);
    }

    /**
     * Back while typing would only close the keyboard, never reaching the app,
     * and leave the field waiting for a second Back. It ends typing here
     * instead, before the keyboard sees it.
     */
    @Override
    public boolean onKeyPreIme(int keyCode, KeyEvent event) {
        boolean back = keyCode == KeyEvent.KEYCODE_BACK;
        if (!back || !editing) {
            return super.onKeyPreIme(keyCode, event);
        }

        if (event.getAction() == KeyEvent.ACTION_UP) {
            stop();
            Natives.onKeyboardHidden();
        }

        return true;
    }

    private static int inputType(int purpose) {
        switch (purpose) {
            case PURPOSE_URL:
                return InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_URI;
            case PURPOSE_SECRET:
                return InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_PASSWORD;
            case PURPOSE_VERBATIM:
                // The variation keyboards honor to turn off suggestions and autocorrect.
                return InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD;
            default:
                return InputType.TYPE_CLASS_TEXT;
        }
    }

    /** Keys the keyboard sends as key events (Backspace, Enter) reach the app as usual input. */
    private static final class Relay extends BaseInputConnection {
        private String composing = "";

        Relay(View view) {
            super(view, false);
        }

        @Override
        public boolean setComposingText(CharSequence text, int newCursorPosition) {
            composing = text.toString();
            Natives.onComposingText(composing);
            return true;
        }

        @Override
        public boolean commitText(CharSequence text, int newCursorPosition) {
            composing = "";
            Natives.onCommitText(text.toString());
            return true;
        }

        @Override
        public boolean finishComposingText() {
            if (!composing.isEmpty()) {
                Natives.onCommitText(composing);
                composing = "";
            }
            return true;
        }

        @Override
        public boolean deleteSurroundingText(int beforeLength, int afterLength) {
            Natives.onDeleteSurrounding(beforeLength, afterLength);
            return true;
        }

        @Override
        public boolean performEditorAction(int actionCode) {
            finishComposingText();
            Natives.onEditorAction();
            return true;
        }
    }
}
