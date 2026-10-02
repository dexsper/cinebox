package io.github.dexsper.cinebox;

import android.app.Activity;
import android.os.Handler;
import android.os.Looper;
import android.util.Log;
import android.view.SurfaceView;
import android.view.View;
import android.widget.FrameLayout;

import androidx.annotation.OptIn;
import androidx.media3.common.AudioAttributes;
import androidx.media3.common.C;
import androidx.media3.common.ColorInfo;
import androidx.media3.common.Format;
import androidx.media3.common.MediaItem;
import androidx.media3.common.MimeTypes;
import androidx.media3.common.PlaybackException;
import androidx.media3.common.Player;
import androidx.media3.common.TrackSelectionOverride;
import androidx.media3.common.TrackSelectionParameters;
import androidx.media3.common.Tracks;
import androidx.media3.common.VideoSize;
import androidx.media3.common.text.Cue;
import androidx.media3.common.text.CueGroup;
import androidx.media3.common.util.UnstableApi;
import androidx.media3.datasource.DefaultHttpDataSource;
import androidx.media3.exoplayer.DecoderReuseEvaluation;
import androidx.media3.exoplayer.DefaultRenderersFactory;
import androidx.media3.exoplayer.ExoPlayer;
import androidx.media3.exoplayer.analytics.AnalyticsListener;
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory;
import androidx.media3.exoplayer.source.MediaSource;
import androidx.media3.exoplayer.source.MergingMediaSource;
import androidx.media3.extractor.DefaultExtractorsFactory;

import java.util.HashMap;
import java.util.Map;

/**
 * The app's video player: Media3 ExoPlayer showing the picture in a SurfaceView
 * below the window, where the Rust side leaves the window clear. Commands come
 * from the Rust thread and run on the main thread; what they change is read
 * back from any thread (crates/cinebox-android/src/player.rs).
 */
@OptIn(markerClass = UnstableApi.class)
final class VideoPlayer {
    private static final String TAG = "CineboxPlayer";
    /** How often the position is refreshed while a stream is loaded. */
    private static final long TICK_MS = 250;

    /** Bits of {@link #flags()} and codes of {@link #failure()}, mirrored in player.rs. */
    static final int FLAG_PAUSED = 1;
    static final int FLAG_ENDED = 2;
    static final int FAILURE_NONE = 0;
    static final int FAILURE_UNSUPPORTED = 1;
    static final int FAILURE_NETWORK = 2;
    static final int FAILURE_OTHER = 3;

    /** A track id to apply once the file's tracks are known. */
    private static final int NO_CHOICE = -1;
    private static final int SUBTITLES_OFF = 0;

    private final Activity activity;
    private final SurfaceView view;
    private final Handler main = new Handler(Looper.getMainLooper());
    private final Runnable tick = this::tick;
    private ExoPlayer player;
    private int pendingAudio = NO_CHOICE;
    private int pendingSubtitle = NO_CHOICE;

    private volatile long positionMs;
    private volatile long durationMs;
    private volatile int flags = FLAG_PAUSED;
    private volatile int videoWidth;
    private volatile int videoHeight;
    private volatile String subtitle;
    private volatile String tracks = "";
    private volatile String decoderName;
    private volatile String decoderFormat;
    private volatile int failure = FAILURE_NONE;
    private volatile String failureMessage;

    VideoPlayer(Activity activity) {
        this.activity = activity;
        view = new SurfaceView(activity);
        view.setVisibility(View.GONE);
    }

    /** Once the activity has its content view: setting that one drops views added before. */
    void attachView() {
        int match = FrameLayout.LayoutParams.MATCH_PARENT;
        activity.addContentView(view, new FrameLayout.LayoutParams(match, match));
    }

    /** {@code headers} holds {@code Name: value} lines; {@code audioUrl} may be null. */
    void load(String url, String headers, String audioUrl, long startMs) {
        main.post(() -> {
            ExoPlayer exo = player();
            clearState();
            view.setVisibility(View.VISIBLE);

            TrackSelectionParameters fresh = exo.getTrackSelectionParameters()
                    .buildUpon()
                    .clearOverrides()
                    .setTrackTypeDisabled(C.TRACK_TYPE_TEXT, false)
                    .build();

            exo.setTrackSelectionParameters(fresh);
            exo.setMediaSource(source(url, headers, audioUrl), startMs);
            exo.prepare();
            exo.setPlayWhenReady(true);

            main.removeCallbacks(tick);
            main.post(tick);
        });
    }

    void stop() {
        main.post(() -> {
            main.removeCallbacks(tick);
            if (player != null) {
                player.stop();
                player.clearMediaItems();
            }

            clearState();
            view.setVisibility(View.GONE);
            Natives.onPlayerChanged();
        });
    }

    void setPaused(boolean paused) {
        main.post(() -> {
            if (player == null) {
                return;
            }

            player.setPlayWhenReady(!paused);
            publish();
        });
    }

    void seekTo(long positionMs) {
        main.post(() -> {
            if (player == null) {
                return;
            }

            player.seekTo(Math.max(0, positionMs));
            publish();
        });
    }

    void seekBy(long deltaMs) {
        main.post(() -> {
            if (player == null) {
                return;
            }

            player.seekTo(Math.max(0, player.getCurrentPosition() + deltaMs));
            publish();
        });
    }

    /** {@code id} counts from 1 among the file's audio tracks. */
    void selectAudio(int id) {
        main.post(() -> {
            pendingAudio = id;
            applyChoices();
        });
    }

    /** {@code id} counts from 1 among the file's subtitle tracks; 0 turns them off. */
    void selectSubtitle(int id) {
        main.post(() -> {
            pendingSubtitle = id;
            applyChoices();
        });
    }

    /** Where the picture goes, in window pixels; it may reach past the window. */
    void place(int x, int y, int width, int height) {
        main.post(() -> {
            FrameLayout.LayoutParams params = new FrameLayout.LayoutParams(width, height);
            params.leftMargin = x;
            params.topMargin = y;
            view.setLayoutParams(params);
        });
    }

    void release() {
        main.removeCallbacks(tick);
        if (player == null) {
            return;
        }

        player.release();
        player = null;
    }

    long positionMs() {
        return positionMs;
    }

    long durationMs() {
        return durationMs;
    }

    int flags() {
        return flags;
    }

    /** Display width: the pixel aspect ratio is applied. */
    int videoWidth() {
        return videoWidth;
    }

    int videoHeight() {
        return videoHeight;
    }

    /** The text on screen now, lines joined with newlines; null when there is none. */
    String subtitle() {
        return subtitle;
    }

    /** One line per playable track: kind, id, selected (0 or 1), language, label; tab-separated. */
    String tracks() {
        return tracks;
    }

    String decoderName() {
        return decoderName;
    }

    /** Codecs string and colour description of the video, tab-separated. */
    String decoderFormat() {
        return decoderFormat;
    }

    int failure() {
        return failure;
    }

    String failureMessage() {
        return failureMessage;
    }

    private ExoPlayer player() {
        if (player != null) {
            return player;
        }

        DefaultRenderersFactory renderers = new DefaultRenderersFactory(activity)
                .setEnableDecoderFallback(true);

        AudioAttributes movie = new AudioAttributes.Builder()
                .setUsage(C.USAGE_MEDIA)
                .setContentType(C.AUDIO_CONTENT_TYPE_MOVIE)
                .build();

        // MediaSessionController holds audio focus for the player; a second
        // request from the same app would take it away from the first.
        player = new ExoPlayer.Builder(activity, renderers)
                .setAudioAttributes(movie, false)
                .build();

        player.setVideoSurfaceView(view);
        player.addListener(new Listener());
        player.addAnalyticsListener(new Decoders());

        return player;
    }

    private MediaSource source(String url, String headers, String audioUrl) {
        Map<String, String> properties = new HashMap<>();
        DefaultHttpDataSource.Factory http = new DefaultHttpDataSource.Factory()
                .setAllowCrossProtocolRedirects(true);

        for (String line : headers.split("\n")) {
            int colon = line.indexOf(':');
            if (colon <= 0) {
                continue;
            }

            String name = line.substring(0, colon).trim();
            String value = line.substring(colon + 1).trim();
            // The factory sends its own User-Agent over a request property of that name.
            if (name.equalsIgnoreCase("User-Agent")) {
                http.setUserAgent(value);
                continue;
            }

            properties.put(name, value);
        }

        http.setDefaultRequestProperties(properties);
        DefaultExtractorsFactory extractors = new DefaultExtractorsFactory()
                .setConstantBitrateSeekingEnabled(true);
                
        DefaultMediaSourceFactory sources = new DefaultMediaSourceFactory(http, extractors);
        MediaSource video = sources.createMediaSource(MediaItem.fromUri(url));
        if (audioUrl == null) {
            return video;
        }

        MediaSource audio = sources.createMediaSource(MediaItem.fromUri(audioUrl));
        return new MergingMediaSource(video, audio);
    }

    private void clearState() {
        pendingAudio = NO_CHOICE;
        pendingSubtitle = NO_CHOICE;
        positionMs = 0;
        durationMs = 0;
        flags = FLAG_PAUSED;
        videoWidth = 0;
        videoHeight = 0;
        subtitle = null;
        tracks = "";
        decoderName = null;
        decoderFormat = null;
        failure = FAILURE_NONE;
        failureMessage = null;
    }

    private void tick() {
        if (player == null) {
            return;
        }

        publish();
        main.postDelayed(tick, TICK_MS);
    }

    private void publish() {
        positionMs = player.getCurrentPosition();
        long duration = player.getDuration();
        durationMs = duration == C.TIME_UNSET ? 0 : duration;

        int next = 0;
        if (!player.getPlayWhenReady()) {
            next |= FLAG_PAUSED;
        }
        if (player.getPlaybackState() == Player.STATE_ENDED) {
            next |= FLAG_ENDED;
        }

        VideoSize size = player.getVideoSize();
        videoWidth = Math.round(size.width * size.pixelWidthHeightRatio);

        flags = next;
        videoHeight = size.height;
    }

    /** Track choices made before the tracks were known wait here until they are. */
    private void applyChoices() {
        if (player == null) {
            return;
        }
        if (!choicePending()) {
            return;
        }

        Tracks all = player.getCurrentTracks();
        if (all.isEmpty()) {
            return;
        }

        TrackSelectionParameters.Builder params = player.getTrackSelectionParameters().buildUpon();
        if (pendingAudio != NO_CHOICE) {
            override(params, nth(all, C.TRACK_TYPE_AUDIO, pendingAudio));
            pendingAudio = NO_CHOICE;
        }

        if (pendingSubtitle == SUBTITLES_OFF) {
            params.setTrackTypeDisabled(C.TRACK_TYPE_TEXT, true);
        } else if (pendingSubtitle != NO_CHOICE) {
            params.setTrackTypeDisabled(C.TRACK_TYPE_TEXT, false);
            override(params, nth(all, C.TRACK_TYPE_TEXT, pendingSubtitle));
        }
        
        pendingSubtitle = NO_CHOICE;
        player.setTrackSelectionParameters(params.build());
    }

    private boolean choicePending() {
        if (pendingAudio != NO_CHOICE) {
            return true;
        }

        return pendingSubtitle != NO_CHOICE;
    }

    private static void override(TrackSelectionParameters.Builder params, Tracks.Group group) {
        if (group == null) {
            return;
        }

        params.setOverrideForType(new TrackSelectionOverride(group.getMediaTrackGroup(), 0));
    }

    /** The {@code id}-th group of {@code type}, counting from 1 in file order. */
    private static Tracks.Group nth(Tracks all, int type, int id) {
        int seen = 0;
        for (Tracks.Group group : all.getGroups()) {
            if (group.getType() != type) {
                continue;
            }

            seen++;
            if (seen == id) {
                return group;
            }
        }

        return null;
    }

    private void publishTracks(Tracks all) {
        StringBuilder lines = new StringBuilder();
        int audio = 0;
        int text = 0;
        int video = 0;

        for (Tracks.Group group : all.getGroups()) {
            int type = group.getType();
            String kind;
            int id;
            if (type == C.TRACK_TYPE_AUDIO) {
                kind = "audio";
                id = ++audio;
            } else if (type == C.TRACK_TYPE_TEXT) {
                kind = "sub";
                id = ++text;
            } else if (type == C.TRACK_TYPE_VIDEO) {
                kind = "video";
                id = ++video;
            } else {
                continue;
            }

            // Ids keep counting over the tracks left out, so they still match nth().
            if (!group.isSupported()) {
                continue;
            }

            Format format = group.getTrackFormat(0);
            if (isPictureSubtitle(format)) {
                continue;
            }

            lines.append(kind).append('\t')
                    .append(id).append('\t')
                    .append(group.isSelected() ? 1 : 0).append('\t')
                    .append(field(format.language)).append('\t')
                    .append(field(format.label)).append('\n');
        }

        tracks = lines.toString();
    }

    /** Subtitles drawn as pictures; only text is passed to the app. */
    private static boolean isPictureSubtitle(Format format) {
        // Parsed during extraction, the original format moves to `codecs`.
        String original = format.codecs != null ? format.codecs : format.sampleMimeType;
        if (original == null) {
            return false;
        }

        switch (original) {
            case MimeTypes.APPLICATION_PGS:
            case MimeTypes.APPLICATION_VOBSUB:
            case MimeTypes.APPLICATION_DVBSUBS:
                return true;
            default:
                return false;
        }
    }

    private static String field(String value) {
        if (value == null) {
            return "";
        }

        return value.replace('\t', ' ').replace('\n', ' ');
    }

    private static int failureCode(int errorCode) {
        // ERROR_CODE_IO_* are the 2xxx range.
        if (errorCode / 1000 == 2) {
            return FAILURE_NETWORK;
        }

        switch (errorCode) {
            case PlaybackException.ERROR_CODE_PARSING_CONTAINER_UNSUPPORTED:
            case PlaybackException.ERROR_CODE_DECODING_FORMAT_UNSUPPORTED:
            case PlaybackException.ERROR_CODE_DECODING_FORMAT_EXCEEDS_CAPABILITIES:
                return FAILURE_UNSUPPORTED;
            default:
                return FAILURE_OTHER;
        }
    }

    private final class Listener implements Player.Listener {
        @Override
        public void onEvents(Player changed, Player.Events events) {
            publish();
            Natives.onPlayerChanged();
        }

        @Override
        public void onTracksChanged(Tracks all) {
            publishTracks(all);
            applyChoices();
        }

        @Override
        public void onCues(CueGroup group) {
            StringBuilder text = new StringBuilder();
            for (Cue cue : group.cues) {
                if (cue.text == null) {
                    continue;
                }
                if (text.length() > 0) {
                    text.append('\n');
                }
                text.append(cue.text);
            }

            subtitle = text.length() == 0 ? null : text.toString();
        }

        @Override
        public void onPlayerError(PlaybackException error) {
            Log.w(TAG, "playback failed: " + error.getErrorCodeName(), error);
            failureMessage = error.getErrorCodeName() + ": " + error.getMessage();
            failure = failureCode(error.errorCode);
        }
    }

    private final class Decoders implements AnalyticsListener {
        @Override
        public void onVideoDecoderInitialized(
                EventTime eventTime, String name, long initializedTimestampMs, long initializationDurationMs) {
            decoderName = name;
        }

        @Override
        public void onVideoInputFormatChanged(
                EventTime eventTime, Format format, DecoderReuseEvaluation reuse) {
            String codecs = format.codecs != null ? format.codecs : format.sampleMimeType;
            ColorInfo color = format.colorInfo;
            String colorText = color != null ? color.toLogString() : "";
            decoderFormat = field(codecs) + '\t' + field(colorText);
        }
    }
}
