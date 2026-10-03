package io.github.dexsper.cinebox;

import android.content.Context;
import android.os.Build;
import android.text.Editable;
import android.text.InputType;
import android.text.Selection;
import android.text.SpannableStringBuilder;
import android.view.KeyEvent;
import android.view.View;
import android.view.inputmethod.BaseInputConnection;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.ExtractedText;
import android.view.inputmethod.ExtractedTextRequest;
import android.view.inputmethod.InputConnection;
import android.view.inputmethod.InputMethodManager;

/**
 * An invisible editor the soft keyboard can attach to. NativeActivity has no
 * text view of its own, so without this the keyboard would neither show nor
 * deliver text.
 *
 * <p>It holds a copy of the Rust field being typed. Keyboards read the field
 * back, and typing from a phone (the Google TV remote) sends nothing to a
 * field that will not say what it holds. Every edit goes back to Rust as the
 * whole field; Rust sends its own changes with {@link #update}. UI thread only.
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
    private final Editable text = new SpannableStringBuilder();
    private int inputType = InputType.TYPE_CLASS_TEXT;
    private int imeAction = EditorInfo.IME_ACTION_DONE;
    private boolean editing;
    /** Set while the keyboard asked to be told of every change to the text. */
    private ExtractedTextRequest monitor;
    /** The field as Rust last knew it, in code points like Rust's characters. */
    private String reported = "";
    private int reportedStart;
    private int reportedEnd;

    TextInputView(Context context) {
        super(context);
        inputMethods = context.getSystemService(InputMethodManager.class);
    }

    /** {@code start} and {@code end} count code points. */
    void start(int purpose, int action, String value, int start, int end) {
        inputType = inputType(purpose);
        imeAction = imeAction(action);
        editing = true;
        monitor = null;
        replace(value, start, end);

        // Focusable only while typing: the system shows the keyboard on its own
        // for a focused editor when the window gains focus.
        setFocusable(true);
        setFocusableInTouchMode(true);
        requestFocus();
        inputMethods.restartInput(this);
        inputMethods.showSoftInput(this, 0);
    }

    /** The field changed on the Rust side, not through the keyboard. */
    void update(String value, int start, int end) {
        if (!editing) {
            return;
        }

        if (isReported(value, start, end)) {
            return;
        }

        replace(value, start, end);
        tellKeyboard();
    }

    void stop() {
        editing = false;
        monitor = null;
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
        // Fullscreen (extract) mode would cover the app with the keyboard's
        // own copy of the field.
        info.imeOptions = imeAction
                | EditorInfo.IME_FLAG_NO_FULLSCREEN
                | EditorInfo.IME_FLAG_NO_EXTRACT_UI;
        info.initialSelStart = Selection.getSelectionStart(text);
        info.initialSelEnd = Selection.getSelectionEnd(text);
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            info.setInitialSurroundingText(text);
        }

        return new Relay();
    }

    /**
     * Back while typing would only close the keyboard, never reaching the app,
     * and leave the field waiting for a second Back. It ends typing here
     * instead, before the keyboard sees it.
     */
    @Override
    public boolean onKeyPreIme(int keyCode, KeyEvent event) {
        boolean endsTyping = editing && keyCode == KeyEvent.KEYCODE_BACK;
        if (!endsTyping) {
            return super.onKeyPreIme(keyCode, event);
        }

        if (event.getAction() == KeyEvent.ACTION_UP) {
            stop();
            Natives.onKeyboardHidden();
        }

        return true;
    }

    private void replace(String value, int start, int end) {
        BaseInputConnection.removeComposingSpans(text);
        text.replace(0, text.length(), value);
        Selection.setSelection(text, charIndex(value, start), charIndex(value, end));
        remember(value, start, end);
    }

    private boolean isReported(String value, int start, int end) {
        return value.equals(reported) && start == reportedStart && end == reportedEnd;
    }

    private void remember(String value, int start, int end) {
        reported = value;
        reportedStart = start;
        reportedEnd = end;
    }

    /** After the keyboard's edit settled: what the field holds now, to both sides. */
    private void edited() {
        tellKeyboard();

        String value = text.toString();
        int start = codePointIndex(value, Selection.getSelectionStart(text));
        int end = codePointIndex(value, Selection.getSelectionEnd(text));
        if (isReported(value, start, end)) {
            return;
        }

        remember(value, start, end);
        Natives.onTextEdited(value, start, end);
    }

    private void tellKeyboard() {
        int start = Selection.getSelectionStart(text);
        int end = Selection.getSelectionEnd(text);
        int composingStart = BaseInputConnection.getComposingSpanStart(text);
        int composingEnd = BaseInputConnection.getComposingSpanEnd(text);
        inputMethods.updateSelection(this, start, end, composingStart, composingEnd);

        if (monitor != null) {
            inputMethods.updateExtractedText(this, monitor.token, extractedText());
        }
    }

    private ExtractedText extractedText() {
        ExtractedText out = new ExtractedText();
        out.text = text.toString();
        out.startOffset = 0;
        out.partialStartOffset = -1;
        out.partialEndOffset = -1;
        out.selectionStart = Selection.getSelectionStart(text);
        out.selectionEnd = Selection.getSelectionEnd(text);
        out.flags = ExtractedText.FLAG_SINGLE_LINE;
        return out;
    }

    /**
     * Backspace and Delete sent by the keyboard are applied to the copy: in
     * the app they would land after the keyboard's next edit of the copy.
     */
    private boolean deleteKey(KeyEvent event) {
        int code = event.getKeyCode();
        boolean backward = code == KeyEvent.KEYCODE_DEL;
        boolean forward = code == KeyEvent.KEYCODE_FORWARD_DEL;
        if (!backward && !forward) {
            return false;
        }

        if (event.getAction() != KeyEvent.ACTION_DOWN) {
            return true;
        }

        // A selection made backwards has its start after its end.
        int anchor = Selection.getSelectionStart(text);
        int cursor = Selection.getSelectionEnd(text);
        int start = Math.min(anchor, cursor);
        int end = Math.max(anchor, cursor);
        if (start == end) {
            if (backward) {
                start = previousCodePoint(start);
            } else {
                end = nextCodePoint(end);
            }
        }

        BaseInputConnection.removeComposingSpans(text);
        text.delete(start, end);
        edited();
        return true;
    }

    private int previousCodePoint(int index) {
        if (index <= 0) {
            return 0;
        }

        return Character.offsetByCodePoints(text, index, -1);
    }

    private int nextCodePoint(int index) {
        if (index >= text.length()) {
            return text.length();
        }

        return Character.offsetByCodePoints(text, index, 1);
    }

    private static int codePointIndex(String value, int charIndex) {
        int clamped = Math.max(0, Math.min(charIndex, value.length()));
        return value.codePointCount(0, clamped);
    }

    private static int charIndex(String value, int codePoints) {
        int total = value.codePointCount(0, value.length());
        int clamped = Math.max(0, Math.min(codePoints, total));
        return value.offsetByCodePoints(0, clamped);
    }

    private static int inputType(int purpose) {
        switch (purpose) {
            case PURPOSE_URL:
                return InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_URI;
            case PURPOSE_SECRET:
                return InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_PASSWORD;
            case PURPOSE_VERBATIM:
                // Keyboards honor this variation by turning off suggestions and autocorrect.
                return InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD;
            default:
                return InputType.TYPE_CLASS_TEXT;
        }
    }

    private static int imeAction(int action) {
        if (action == ACTION_SEARCH) {
            return EditorInfo.IME_ACTION_SEARCH;
        }

        return EditorInfo.IME_ACTION_DONE;
    }

    /**
     * BaseInputConnection edits {@link #text} itself and wraps each edit in a
     * batch, so the end of the outermost batch is when an edit has settled.
     * Other keys, such as Enter, reach the app as usual input.
     */
    private final class Relay extends BaseInputConnection {
        private int batches;

        Relay() {
            super(TextInputView.this, true);
        }

        @Override
        public Editable getEditable() {
            return text;
        }

        @Override
        public boolean beginBatchEdit() {
            batches++;
            return true;
        }

        @Override
        public boolean endBatchEdit() {
            if (batches > 0) {
                batches--;
            }

            if (batches == 0) {
                edited();
            }

            return batches > 0;
        }

        @Override
        public boolean setSelection(int start, int end) {
            beginBatchEdit();
            boolean done = super.setSelection(start, end);
            endBatchEdit();
            return done;
        }

        @Override
        public ExtractedText getExtractedText(ExtractedTextRequest request, int flags) {
            if ((flags & GET_EXTRACTED_TEXT_MONITOR) != 0) {
                monitor = request;
            }

            return extractedText();
        }

        @Override
        public boolean sendKeyEvent(KeyEvent event) {
            if (deleteKey(event)) {
                return true;
            }

            return super.sendKeyEvent(event);
        }

        @Override
        public boolean performEditorAction(int actionCode) {
            finishComposingText();
            Natives.onEditorAction();
            return true;
        }
    }
}
