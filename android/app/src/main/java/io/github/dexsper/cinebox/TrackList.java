package io.github.dexsper.cinebox;

import androidx.annotation.Nullable;
import androidx.media3.common.C;
import androidx.media3.common.Format;
import androidx.media3.common.MimeTypes;
import androidx.media3.common.Tracks;

/**
 * A file's tracks as the Rust side numbers them (player.rs `parse_tracks`):
 * ids count from 1 per kind in file order, over every track of that kind,
 * so an id still finds its track when the unplayable ones are left out of
 * the list.
 */
final class TrackList {
    private TrackList() {}

    /** One line per playable track: kind, id, selected (0 or 1), language, label; tab-separated. */
    static String describe(Tracks tracks) {
        StringBuilder lines = new StringBuilder();
        int audio = 0;
        int text = 0;
        int video = 0;

        for (Tracks.Group group : tracks.getGroups()) {
            switch (group.getType()) {
                case C.TRACK_TYPE_AUDIO:
                    audio++;
                    appendLine(lines, "audio", audio, group);
                    break;
                case C.TRACK_TYPE_TEXT:
                    text++;
                    appendLine(lines, "sub", text, group);
                    break;
                case C.TRACK_TYPE_VIDEO:
                    video++;
                    appendLine(lines, "video", video, group);
                    break;
                default:
                    break;
            }
        }

        return lines.toString();
    }

    /** The {@code id}-th track group of {@code type}; null when the file has fewer. */
    @Nullable
    static Tracks.Group find(Tracks tracks, int type, int id) {
        int seen = 0;
        for (Tracks.Group group : tracks.getGroups()) {
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

    private static void appendLine(StringBuilder lines, String kind, int id, Tracks.Group group) {
        if (!isPlayable(group)) {
            return;
        }

        Format format = group.getTrackFormat(0);
        lines.append(kind).append('\t')
                .append(id).append('\t')
                .append(group.isSelected() ? 1 : 0).append('\t')
                .append(field(format.language)).append('\t')
                .append(field(format.label)).append('\n');
    }

    /** Subtitles drawn as pictures count as unplayable: only text is passed to the app. */
    private static boolean isPlayable(Tracks.Group group) {
        if (!group.isSupported()) {
            return false;
        }

        return !isPictureSubtitle(group.getTrackFormat(0));
    }

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

    private static String field(@Nullable String value) {
        if (value == null) {
            return "";
        }

        return value.replace('\t', ' ').replace('\n', ' ');
    }
}
