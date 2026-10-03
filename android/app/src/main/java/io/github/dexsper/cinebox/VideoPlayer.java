package io.github.dexsper.cinebox;

import android.app.Activity;
import android.os.Handler;
import android.os.Looper;
import android.util.Log;
import android.view.SurfaceView;
import android.view.View;
import android.widget.FrameLayout;

import androidx.annotation.Nullable;
import androidx.annotation.OptIn;
import androidx.media3.common.AudioAttributes;
import androidx.media3.common.C;
import androidx.media3.common.ColorInfo;
import androidx.media3.common.Format;
import androidx.media3.common.MediaItem;
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
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.function.Consumer;

/**
 * The app's video player: Media3 ExoPlayer showing the picture in a SurfaceView
 * below the window, where the Rust side leaves the window clear. Commands come
 * from the Rust thread and run on the main thread; what they change is read
 * back from any thread (crates/cinebox-android/src/player.rs).
 */
@OptIn(markerClass = UnstableApi.class)
final class VideoPlayer {
    /** Bits of {@link #flags()} and codes of {@link #failure()}, mirrored in player.rs. */
    static final int FLAG_PAUSED = 1;
    static final int FLAG_ENDED = 2;
    static final int FAILURE_NONE = 0;
    static final int FAILURE_UNSUPPORTED = 1;
    static final int FAILURE_NETWORK = 2;
    static final int FAILURE_OTHER = 3;

    private static final String TAG = "CineboxPlayer";
    /** How often the position is refreshed while a stream is loaded. */
    private static final long TICK_MS = 250;
    /** TorrServer can hold a response while it fetches the pieces a read needs. */
    private static final int CONNECT_TIMEOUT_MS = 30_000;
    private static final int READ_TIMEOUT_MS = 60_000;
    private static final int NO_CHOICE = -1;
    private static final int SUBTITLES_OFF = 0;

    private final Activity activity;
    private final SurfaceView view;
    private final Handler main = new Handler(Looper.getMainLooper());
    private final Runnable tick = this::tick;
    private ExoPlayer player;
    private boolean released;
    private int scalingMode = C.VIDEO_SCALING_MODE_SCALE_TO_FIT;
    /** Track ids chosen before the file's tracks were known; applied once they are. */
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

    /** {@code headers} holds {@code Name: value} lines. */
    void load(String url, String headers, @Nullable String audioUrl, long startMs) {
        post(() -> {
            ExoPlayer exo = getOrCreatePlayer();
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
        post(() -> {
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
        withPlayer(exo -> exo.setPlayWhenReady(!paused));
    }

    void seekTo(long positionMs) {
        withPlayer(exo -> exo.seekTo(Math.max(0, positionMs)));
    }

    void seekBy(long deltaMs) {
        withPlayer(exo -> exo.seekTo(Math.max(0, exo.getCurrentPosition() + deltaMs)));
    }

    /** {@code id} counts from 1 among the file's audio tracks. */
    void selectAudio(int id) {
        post(() -> {
            pendingAudio = id;
            applyChoices();
        });
    }

    /** {@code id} counts from 1 among the file's subtitle tracks; 0 turns them off. */
    void selectSubtitle(int id) {
        post(() -> {
            pendingSubtitle = id;
            applyChoices();
        });
    }

    /**
     * Where the picture goes, in window pixels; {@code crop} fills that rect
     * by cropping the picture.
     */
    void place(int x, int y, int width, int height, boolean crop) {
        post(() -> {
            FrameLayout.LayoutParams params = new FrameLayout.LayoutParams(width, height);
            params.leftMargin = x;
            params.topMargin = y;
            view.setLayoutParams(params);

            scalingMode = crop
                    ? C.VIDEO_SCALING_MODE_SCALE_TO_FIT_WITH_CROPPING
                    : C.VIDEO_SCALING_MODE_SCALE_TO_FIT;
            if (player != null) {
                player.setVideoScalingMode(scalingMode);
            }
        });
    }

    void release() {
        released = true;
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

    /** The text on screen now, lines joined with newlines. */
    @Nullable
    String subtitle() {
        return subtitle;
    }

    /** See {@link TrackList#describe}. */
    String tracks() {
        return tracks;
    }

    @Nullable
    String decoderName() {
        return decoderName;
    }

    /** Codecs string and colour description of the video, tab-separated. */
    @Nullable
    String decoderFormat() {
        return decoderFormat;
    }

    int failure() {
        return failure;
    }

    @Nullable
    String failureMessage() {
        return failureMessage;
    }

    /** Commands still queued when the activity goes away are dropped. */
    private void post(Runnable command) {
        main.post(() -> {
            if (released) {
                return;
            }

            command.run();
        });
    }

    /** A command that needs the player, dropped before the first load creates one. */
    private void withPlayer(Consumer<ExoPlayer> command) {
        post(() -> {
            if (player == null) {
                return;
            }

            command.accept(player);
            publish(player);
        });
    }

    private ExoPlayer getOrCreatePlayer() {
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
        player.setVideoScalingMode(scalingMode);
        player.addListener(new StateListener());
        player.addAnalyticsListener(new DecoderListener());

        return player;
    }

    private static MediaSource source(String url, String headers, @Nullable String audioUrl) {
        DefaultExtractorsFactory extractors = new DefaultExtractorsFactory()
                .setConstantBitrateSeekingEnabled(true);

        DefaultHttpDataSource.Factory http = httpDataSource(headers);
        DefaultMediaSourceFactory sources = new DefaultMediaSourceFactory(http, extractors);
        MediaSource video = sources.createMediaSource(MediaItem.fromUri(url));
        if (audioUrl == null) {
            return video;
        }

        MediaSource audio = sources.createMediaSource(MediaItem.fromUri(audioUrl));
        return new MergingMediaSource(video, audio);
    }

    private static DefaultHttpDataSource.Factory httpDataSource(String headers) {
        return new DefaultHttpDataSource.Factory()
                .setConnectTimeoutMs(CONNECT_TIMEOUT_MS)
                .setReadTimeoutMs(READ_TIMEOUT_MS)
                .setAllowCrossProtocolRedirects(true)
                .setDefaultRequestProperties(requestProperties(headers));
    }

    private static Map<String, String> requestProperties(String headers) {
        Map<String, String> properties = new HashMap<>();
        for (String line : headers.split("\n")) {
            int colon = line.indexOf(':');
            if (colon <= 0) {
                continue;
            }

            String name = line.substring(0, colon).trim();
            String value = line.substring(colon + 1).trim();
            properties.put(name, value);
        }

        return properties;
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

        publish(player);
        main.postDelayed(tick, TICK_MS);
    }

    private void publish(Player exo) {
        long duration = exo.getDuration();
        VideoSize size = exo.getVideoSize();

        positionMs = exo.getCurrentPosition();
        durationMs = duration == C.TIME_UNSET ? 0 : duration;
        flags = flagsOf(exo);
        videoWidth = Math.round(size.width * size.pixelWidthHeightRatio);
        videoHeight = size.height;
    }

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
            override(params, TrackList.find(all, C.TRACK_TYPE_AUDIO, pendingAudio));
            pendingAudio = NO_CHOICE;
        }

        if (pendingSubtitle == SUBTITLES_OFF) {
            params.setTrackTypeDisabled(C.TRACK_TYPE_TEXT, true);
        } else if (pendingSubtitle != NO_CHOICE) {
            params.setTrackTypeDisabled(C.TRACK_TYPE_TEXT, false);
            override(params, TrackList.find(all, C.TRACK_TYPE_TEXT, pendingSubtitle));
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

    private static void override(
            TrackSelectionParameters.Builder params,
            @Nullable Tracks.Group group
    ) {
        if (group == null) {
            return;
        }

        params.setOverrideForType(new TrackSelectionOverride(group.getMediaTrackGroup(), 0));
    }

    private static int flagsOf(Player exo) {
        int flags = 0;
        if (!exo.getPlayWhenReady()) {
            flags |= FLAG_PAUSED;
        }

        if (exo.getPlaybackState() == Player.STATE_ENDED) {
            flags |= FLAG_ENDED;
        }

        return flags;
    }

    @Nullable
    private static String joinCues(List<Cue> cues) {
        StringBuilder text = new StringBuilder();
        for (Cue cue : cues) {
            if (cue.text == null) {
                continue;
            }

            if (text.length() > 0) {
                text.append('\n');
            }

            text.append(cue.text);
        }

        if (text.length() == 0) {
            return null;
        }

        return text.toString();
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

    private final class StateListener implements Player.Listener {
        @Override
        public void onEvents(Player changed, Player.Events events) {
            publish(changed);
            Natives.onPlayerChanged();
        }

        @Override
        public void onTracksChanged(Tracks all) {
            tracks = TrackList.describe(all);
            applyChoices();
        }

        @Override
        public void onCues(CueGroup group) {
            subtitle = joinCues(group.cues);
        }

        @Override
        public void onPlayerError(PlaybackException error) {
            Log.w(TAG, "playback failed: " + error.getErrorCodeName(), error);
            failureMessage = error.getErrorCodeName() + ": " + error.getMessage();
            failure = failureCode(error.errorCode);
        }
    }

    private final class DecoderListener implements AnalyticsListener {
        @Override
        public void onVideoDecoderInitialized(
                EventTime eventTime,
                String name,
                long initializedTimestampMs,
                long initializationDurationMs
        ) {
            decoderName = name;
        }

        @Override
        public void onVideoInputFormatChanged(
                EventTime eventTime,
                Format format,
                DecoderReuseEvaluation reuse
        ) {
            String codecs = format.codecs != null ? format.codecs : format.sampleMimeType;
            ColorInfo color = format.colorInfo;
            String colorText = color != null ? color.toLogString() : "";
            decoderFormat = Objects.toString(codecs, "") + '\t' + colorText;
        }
    }
}
