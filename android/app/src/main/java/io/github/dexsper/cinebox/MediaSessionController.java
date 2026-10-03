package io.github.dexsper.cinebox;

import android.app.Activity;
import android.app.PendingIntent;
import android.content.Intent;
import android.graphics.Bitmap;
import android.media.AudioAttributes;
import android.media.AudioFocusRequest;
import android.media.AudioManager;
import android.media.MediaMetadata;
import android.media.session.MediaSession;
import android.media.session.PlaybackState;

import androidx.annotation.Nullable;

/**
 * The system media session for the player: remote and assistant transport
 * controls, the "now playing" card, and audio focus. UI thread only.
 */
final class MediaSessionController {
    /** Codes the Rust side maps back to commands (natives.rs `media_command`). */
    static final int COMMAND_PLAY = 0;
    static final int COMMAND_PAUSE = 1;
    static final int COMMAND_PLAY_PAUSE = 2;
    static final int COMMAND_STOP = 3;
    static final int COMMAND_SEEK_TO = 4;
    static final int COMMAND_FAST_FORWARD = 5;
    static final int COMMAND_REWIND = 6;
    static final int COMMAND_NEXT = 7;
    static final int COMMAND_PREVIOUS = 8;

    private static final long ALWAYS_ALLOWED = PlaybackState.ACTION_PLAY
            | PlaybackState.ACTION_PAUSE
            | PlaybackState.ACTION_PLAY_PAUSE
            | PlaybackState.ACTION_STOP
            | PlaybackState.ACTION_SEEK_TO
            | PlaybackState.ACTION_FAST_FORWARD
            | PlaybackState.ACTION_REWIND;

    private final MediaSession session;
    private final AudioManager audio;
    private final AudioFocusRequest focusRequest;
    private boolean playing;
    private boolean hasFocus;
    /** Playback was paused by a short focus loss and resumes when the focus comes back. */
    private boolean resumeOnFocus;

    MediaSessionController(Activity activity) {
        session = new MediaSession(activity, "Cinebox");
        session.setCallback(new Callback());
        session.setSessionActivity(openApp(activity));

        audio = activity.getSystemService(AudioManager.class);
        focusRequest = new AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
                .setAudioAttributes(movieAudio())
                .setOnAudioFocusChangeListener(this::onFocusChange)
                .build();
    }

    void setMetadata(
            String title,
            @Nullable String subtitle,
            long durationMs,
            @Nullable Bitmap artwork
    ) {
        MediaMetadata.Builder metadata = new MediaMetadata.Builder()
                .putString(MediaMetadata.METADATA_KEY_TITLE, title)
                .putString(MediaMetadata.METADATA_KEY_DISPLAY_TITLE, title)
                .putLong(MediaMetadata.METADATA_KEY_DURATION, durationMs);

        if (subtitle != null) {
            metadata.putString(MediaMetadata.METADATA_KEY_DISPLAY_SUBTITLE, subtitle);
        }

        if (artwork != null) {
            metadata.putBitmap(MediaMetadata.METADATA_KEY_ART, artwork);
        }

        session.setMetadata(metadata.build());
        session.setActive(true);
    }

    void setPlayback(boolean playing, long positionMs, boolean canNext, boolean canPrevious) {
        this.playing = playing;
        int state = playing ? PlaybackState.STATE_PLAYING : PlaybackState.STATE_PAUSED;
        float speed = playing ? 1f : 0f;
        PlaybackState playback = new PlaybackState.Builder()
                .setActions(actions(canNext, canPrevious))
                .setState(state, positionMs, speed)
                .build();

        session.setPlaybackState(playback);
        session.setActive(true);
        if (playing) {
            requestFocus();
        }
    }

    /** Nothing is playing any more. */
    void clear() {
        playing = false;
        resumeOnFocus = false;
        session.setPlaybackState(new PlaybackState.Builder()
                .setState(PlaybackState.STATE_NONE, 0, 0f)
                .build());

        session.setActive(false);
        abandonFocus();
    }

    void release() {
        clear();
        session.release();
    }

    private void requestFocus() {
        if (hasFocus) {
            return;
        }

        hasFocus = audio.requestAudioFocus(focusRequest) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED;
        if (!hasFocus) {
            send(COMMAND_PAUSE);
        }
    }

    private void abandonFocus() {
        if (!hasFocus) {
            return;
        }

        audio.abandonAudioFocusRequest(focusRequest);
        hasFocus = false;
    }

    private void onFocusChange(int change) {
        switch (change) {
            case AudioManager.AUDIOFOCUS_GAIN:
                if (resumeOnFocus) {
                    resumeOnFocus = false;
                    send(COMMAND_PLAY);
                }
                break;
            case AudioManager.AUDIOFOCUS_LOSS_TRANSIENT:
                resumeOnFocus = playing;
                send(COMMAND_PAUSE);
                break;
            case AudioManager.AUDIOFOCUS_LOSS:
                resumeOnFocus = false;
                abandonFocus();
                send(COMMAND_PAUSE);
                break;
            default:
                // Ducking for a transient loss is done by the system.
                break;
        }
    }

    private static long actions(boolean canNext, boolean canPrevious) {
        long actions = ALWAYS_ALLOWED;
        if (canNext) {
            actions |= PlaybackState.ACTION_SKIP_TO_NEXT;
        }

        if (canPrevious) {
            actions |= PlaybackState.ACTION_SKIP_TO_PREVIOUS;
        }

        return actions;
    }

    /** What the "now playing" card opens. */
    private static PendingIntent openApp(Activity activity) {
        Intent open = new Intent(activity, activity.getClass())
                .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP);
        int flags = PendingIntent.FLAG_IMMUTABLE;
        return PendingIntent.getActivity(activity, 0, open, flags);
    }

    private static AudioAttributes movieAudio() {
        return new AudioAttributes.Builder()
                .setUsage(AudioAttributes.USAGE_MEDIA)
                .setContentType(AudioAttributes.CONTENT_TYPE_MOVIE)
                .build();
    }

    private static void send(int command) {
        Natives.onMediaCommand(command, 0);
    }

    private static final class Callback extends MediaSession.Callback {
        @Override
        public void onPlay() {
            send(COMMAND_PLAY);
        }

        @Override
        public void onPause() {
            send(COMMAND_PAUSE);
        }

        @Override
        public void onStop() {
            send(COMMAND_STOP);
        }

        @Override
        public void onSeekTo(long positionMs) {
            Natives.onMediaCommand(COMMAND_SEEK_TO, positionMs / 1000.0);
        }

        @Override
        public void onFastForward() {
            send(COMMAND_FAST_FORWARD);
        }

        @Override
        public void onRewind() {
            send(COMMAND_REWIND);
        }

        @Override
        public void onSkipToNext() {
            send(COMMAND_NEXT);
        }

        @Override
        public void onSkipToPrevious() {
            send(COMMAND_PREVIOUS);
        }
    }
}
