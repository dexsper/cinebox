package io.github.dexsper.cinebox;

import android.Manifest;
import android.app.Activity;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.os.Bundle;
import android.speech.RecognitionListener;
import android.speech.RecognizerIntent;
import android.speech.SpeechRecognizer;

import androidx.annotation.Nullable;

import java.util.List;

/**
 * Speech recognition inside the app, so voice search keeps the app's own look
 * instead of the system's overlay. The recognizer service checks that the
 * calling app holds the microphone permission, which is asked for on first use.
 * UI thread only.
 */
final class SpeechInput {
    static final int PERMISSION_REQUEST = 1;

    private final Activity activity;
    private SpeechRecognizer recognizer;
    /** Language to listen in once the microphone permission is granted. */
    private String waitingLanguage;

    SpeechInput(Activity activity) {
        this.activity = activity;
    }

    static boolean isAvailable(Context context) {
        return SpeechRecognizer.isRecognitionAvailable(context);
    }

    void start(String language) {
        int permission = activity.checkSelfPermission(Manifest.permission.RECORD_AUDIO);
        if (permission == PackageManager.PERMISSION_GRANTED) {
            listen(language);
            return;
        }

        waitingLanguage = language;
        String[] wanted = {Manifest.permission.RECORD_AUDIO};
        activity.requestPermissions(wanted, PERMISSION_REQUEST);
    }

    void onPermissionResult(boolean granted) {
        String language = waitingLanguage;
        waitingLanguage = null;

        // No language: stopped while the permission dialog was open.
        if (!granted || language == null) {
            Natives.onSpeechEnded();
            return;
        }

        listen(language);
    }

    void stop() {
        waitingLanguage = null;
        if (recognizer != null) {
            recognizer.cancel();
        }
    }

    void release() {
        if (recognizer == null) {
            return;
        }

        recognizer.destroy();
        recognizer = null;
    }

    private void listen(String language) {
        if (recognizer == null) {
            recognizer = SpeechRecognizer.createSpeechRecognizer(activity);
            recognizer.setRecognitionListener(new Listener());
        }

        String model = RecognizerIntent.LANGUAGE_MODEL_FREE_FORM;
        Intent intent = new Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH)
                .putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, model)
                .putExtra(RecognizerIntent.EXTRA_LANGUAGE, language)
                .putExtra(RecognizerIntent.EXTRA_PARTIAL_RESULTS, true)
                .putExtra(RecognizerIntent.EXTRA_MAX_RESULTS, 1);
        recognizer.startListening(intent);
    }

    @Nullable
    private static String firstResult(Bundle results) {
        List<String> heard = results.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION);
        if (heard == null || heard.isEmpty()) {
            return null;
        }

        String text = heard.get(0);
        if (text.isEmpty()) {
            return null;
        }

        return text;
    }

    private static final class Listener implements RecognitionListener {
        @Override
        public void onReadyForSpeech(Bundle params) {
            Natives.onSpeechListening();
        }

        @Override
        public void onPartialResults(Bundle partialResults) {
            String heard = firstResult(partialResults);
            if (heard != null) {
                Natives.onSpeechPartial(heard);
            }
        }

        @Override
        public void onResults(Bundle results) {
            String heard = firstResult(results);
            if (heard == null) {
                Natives.onSpeechEnded();
                return;
            }

            Natives.onSpeechResult(heard);
        }

        @Override
        public void onError(int error) {
            Natives.onSpeechEnded();
        }

        @Override
        public void onBeginningOfSpeech() {}

        @Override
        public void onRmsChanged(float rmsdB) {}

        @Override
        public void onBufferReceived(byte[] buffer) {}

        @Override
        public void onEndOfSpeech() {}

        @Override
        public void onEvent(int eventType, Bundle params) {}
    }
}
