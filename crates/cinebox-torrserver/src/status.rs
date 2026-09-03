//! `TorrentStatus` / file stats from `POST /torrents`.

/// TorrServer `stat` field (`iota` in `state.go`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TorrentStat {
    Added,
    GettingInfo,
    Preload,
    Working,
    Closed,
    InDb,
    Unknown(i32),
}

impl From<i32> for TorrentStat {
    fn from(value: i32) -> Self {
        match value {
            0 => Self::Added,
            1 => Self::GettingInfo,
            2 => Self::Preload,
            3 => Self::Working,
            4 => Self::Closed,
            5 => Self::InDb,
            other => Self::Unknown(other),
        }
    }
}

/// One file inside a torrent. `id` is 1-based.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct FileStat {
    #[serde(default)]
    pub id: i32,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub length: i64,
}

/// Body of add/get (and stream `?stat`).
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct TorrentStatus {
    #[serde(default)]
    pub hash: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub stat: i32,
    #[serde(default)]
    pub preloaded_bytes: i64,
    #[serde(default)]
    pub preload_size: i64,
    #[serde(default)]
    pub download_speed: f64,
    #[serde(default)]
    pub active_peers: i32,
    #[serde(default)]
    pub total_peers: i32,
    #[serde(default)]
    pub file_stats: Vec<FileStat>,
}

impl TorrentStatus {
    #[must_use]
    pub fn stat_kind(&self) -> TorrentStat {
        TorrentStat::from(self.stat)
    }

    #[must_use]
    pub fn preload_ready(&self) -> bool {
        self.preload_percent() >= 95.0
    }

    /// Preload progress in percent (`100.0` when nothing needs preloading).
    #[must_use]
    pub fn preload_percent(&self) -> f64 {
        if self.preload_size <= 0 {
            return 100.0;
        }
        let progress = (self.preloaded_bytes as f64) * 100.0 / (self.preload_size as f64);

        progress.clamp(0.0, 100.0)
    }
}

/// Playable video extensions.
const VIDEO_EXT: &[&str] = &[
    "asf", "wmv", "divx", "avi", "mp4", "m4v", "mov", "3gp", "3g2", "mkv", "trp", "tp", "mts",
    "mpg", "mpeg", "dat", "vob", "rm", "rmvb", "m2ts", "ts",
];

/// True when the path looks like a video file.
#[must_use]
pub fn is_playable_path(path: &str) -> bool {
    let Some((_, ext)) = path.rsplit_once('.') else {
        return false;
    };
    let ext = ext.to_ascii_lowercase();
    VIDEO_EXT.iter().any(|ok| *ok == ext)
}

/// Video files, or every non-empty file when the torrent has no known video ext.
#[must_use]
pub fn files_for_list(stats: &[FileStat]) -> Vec<FileStat> {
    let playable: Vec<FileStat> = stats
        .iter()
        .filter(|file| file.id > 0 && is_playable_path(&file.path))
        .cloned()
        .collect();
    if !playable.is_empty() {
        return playable;
    }
    stats
        .iter()
        .filter(|file| file.id > 0 && file.length > 0)
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mkv_is_playable() {
        assert!(is_playable_path(r"Season 1/S01E01.mkv"));
        assert!(!is_playable_path("readme.txt"));
        assert!(!is_playable_path("noext"));
    }

    #[test]
    fn falls_back_when_no_video_ext() {
        let stats = vec![FileStat {
            id: 1,
            path: String::from("disc.iso"),
            length: 1000,
        }];
        assert_eq!(files_for_list(&stats).len(), 1);
    }

    fn status_with_preload(preloaded_bytes: i64, preload_size: i64) -> TorrentStatus {
        TorrentStatus {
            hash: String::new(),
            title: String::new(),
            stat: 0,
            preloaded_bytes,
            preload_size,
            download_speed: 0.0,
            active_peers: 0,
            total_peers: 0,
            file_stats: Vec::new(),
        }
    }

    #[test]
    fn preload_ready_when_size_zero() {
        let status = status_with_preload(0, 0);
        assert!(status.preload_ready());
        assert!((status.preload_percent() - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn preload_percent_tracks_bytes() {
        let half = status_with_preload(50, 100);
        assert!((half.preload_percent() - 50.0).abs() < f64::EPSILON);
        assert!(!half.preload_ready());

        let over = status_with_preload(200, 100);
        assert!((over.preload_percent() - 100.0).abs() < f64::EPSILON);
        assert!(over.preload_ready());
    }

    #[test]
    fn parses_add_response_fixture() {
        // Trimmed real `POST /torrents action=add` body; unknown keys ignored.
        let fixture = r#"{
            "title": "Show S01 1080p",
            "poster": "http://img/poster.jpg",
            "hash": "abcdef0123456789abcdef0123456789abcdef01",
            "stat": 3,
            "stat_string": "Torrent working",
            "torrent_size": 4200000000,
            "preloaded_bytes": 5000000,
            "preload_size": 20000000,
            "download_speed": 1250000.5,
            "active_peers": 12,
            "total_peers": 60,
            "file_stats": [
                { "id": 1, "path": "Show/S01E01.mkv", "length": 2100000000 },
                { "id": 2, "path": "Show/S01E02.mkv", "length": 2100000000 }
            ]
        }"#;

        let status: TorrentStatus = serde_json::from_str(fixture)
            .unwrap_or_else(|error| panic!("fixture: {error}"));

        assert_eq!(status.hash, "abcdef0123456789abcdef0123456789abcdef01");
        assert_eq!(status.stat_kind(), TorrentStat::Working);
        assert_eq!(status.file_stats.len(), 2);
        assert_eq!(status.file_stats[0].path, "Show/S01E01.mkv");
        assert!((status.preload_percent() - 25.0).abs() < f64::EPSILON);
    }

    #[test]
    fn missing_fields_default_and_unknown_stat_maps() {
        let status: TorrentStatus = serde_json::from_str("{}")
            .unwrap_or_else(|error| panic!("fixture: {error}"));

        assert_eq!(status.hash, "");
        assert!(status.file_stats.is_empty());
        assert_eq!(status.stat_kind(), TorrentStat::Added);

        assert_eq!(TorrentStat::from(42), TorrentStat::Unknown(42));
        assert_eq!(TorrentStat::from(5), TorrentStat::InDb);
    }
}
