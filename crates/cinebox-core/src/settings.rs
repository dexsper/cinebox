//! JSON settings stored next to the executable.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::paths;

/// Redacted string for API keys and passwords.
///
/// Serializes as the real value (needed for `settings.json`). `Debug` never
/// prints the secret.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SecretString(String);

impl SecretString {
    /// Borrow the secret. Callers must not log this.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Whether the secret is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<String> for SecretString {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for SecretString {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

/// Failures loading or saving settings.
#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("could not determine executable directory")]
    NoExeDir(#[source] io::Error),
    #[error("failed to create config directory {}", .path.display())]
    CreateDir {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to read settings from {}", .path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to write settings to {}", .path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse settings from {}", .path.display())]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to serialize settings")]
    Serialize {
        #[source]
        source: serde_json::Error,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiLanguage {
    #[default]
    English,
    Russian,
}

impl UiLanguage {
    pub const ALL: &[Self] = &[Self::English, Self::Russian];

    /// TMDB `language` query token.
    #[must_use]
    pub const fn tmdb_code(self) -> &'static str {
        match self {
            Self::English => "en-US",
            Self::Russian => "ru-RU",
        }
    }
}

impl fmt::Display for UiLanguage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::English => "English",
            Self::Russian => "Russian",
        })
    }
}

/// Jackett vs Prowlarr. One URL in settings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParserKind {
    #[default]
    Jackett,
    Prowlarr,
}

impl ParserKind {
    pub const ALL: &[Self] = &[Self::Jackett, Self::Prowlarr];
}

impl fmt::Display for ParserKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Jackett => "Jackett",
            Self::Prowlarr => "Prowlarr",
        })
    }
}

/// Video fit inside the window, mpv `keepaspect` / `panscan` / `video-zoom`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoScale {
    /// Letterbox: `keepaspect=yes, panscan=0`.
    #[default]
    Default,
    /// Crop-to-fill: `keepaspect=yes, panscan=1`.
    Expand,
    /// Stretch, ignores aspect: `keepaspect=no`.
    Fill,
    /// `keepaspect=yes` + `video-zoom = log2(1.15)`.
    Zoom115,
    /// `keepaspect=yes` + `video-zoom = log2(1.30)`.
    Zoom130,
}

impl VideoScale {
    pub const ALL: &[Self] = &[
        Self::Default,
        Self::Expand,
        Self::Fill,
        Self::Zoom115,
        Self::Zoom130,
    ];
}

impl fmt::Display for VideoScale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Default => "Default",
            Self::Expand => "Expand",
            Self::Fill => "Fill",
            Self::Zoom115 => "Zoom 115%",
            Self::Zoom130 => "Zoom 130%",
        })
    }
}

/// Resolution band used by torrent filters and default-quality settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QualityBand {
    #[serde(rename = "4k")]
    Uhd,
    #[serde(rename = "1080p")]
    Fhd,
    #[serde(rename = "720p")]
    Hd,
    #[serde(rename = "480p")]
    Sd,
}

impl QualityBand {
    pub const ALL: &[Self] = &[Self::Uhd, Self::Fhd, Self::Hd, Self::Sd];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Uhd => "4K",
            Self::Fhd => "1080p",
            Self::Hd => "720p",
            Self::Sd => "480p",
        }
    }
}

impl fmt::Display for QualityBand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// TMDB poster size path segment.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PosterSize {
    #[serde(rename = "w342")]
    W342,
    #[default]
    #[serde(rename = "w500")]
    W500,
    #[serde(rename = "w780")]
    W780,
}

impl PosterSize {
    pub const ALL: &[Self] = &[Self::W342, Self::W500, Self::W780];

    /// TMDB image size token, e.g. `w500`.
    #[must_use]
    pub const fn tmdb_path(self) -> &'static str {
        match self {
            Self::W342 => "w342",
            Self::W500 => "w500",
            Self::W780 => "w780",
        }
    }
}

impl fmt::Display for PosterSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.tmdb_path())
    }
}

fn default_system_proxy() -> bool {
    true
}

/// General category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralSettings {
    pub language: UiLanguage,
    /// WinINet / env HTTP(S) proxy for TMDB and parser. TorrServer always direct.
    #[serde(default = "default_system_proxy")]
    pub use_system_proxy: bool,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            language: UiLanguage::default(),
            use_system_proxy: true,
        }
    }
}

/// Player category.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayerSettings {
    pub loudnorm: bool,
    pub auto_next: bool,
    pub volume: f64,
}

impl Default for PlayerSettings {
    fn default() -> Self {
        Self {
            loudnorm: false,
            auto_next: false,
            volume: 90.0,
        }
    }
}

/// Parser (Jackett / Prowlarr) category.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ParserSettings {
    pub kind: ParserKind,
    pub url: String,
    pub api_key: SecretString,
    pub default_quality: Vec<QualityBand>,
}

/// External TorrServer category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TorrServerSettings {
    pub url: String,
    pub save_to_db: bool,
    pub wait_preload: bool,
    pub track_timecode: bool,
    pub username: String,
    pub password: SecretString,
}

impl Default for TorrServerSettings {
    fn default() -> Self {
        Self {
            url: String::from("http://127.0.0.1:8090"),
            save_to_db: false,
            wait_preload: false,
            track_timecode: false,
            username: String::new(),
            password: SecretString::default(),
        }
    }
}

/// TMDB category. API key is user-supplied.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TmdbSettings {
    pub api_key: SecretString,
    pub poster_size: PosterSize,
}

/// All settings categories from the ТЗ.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub general: GeneralSettings,
    pub player: PlayerSettings,
    pub parser: ParserSettings,
    pub torrserver: TorrServerSettings,
    pub tmdb: TmdbSettings,
}

/// On-disk JSON store. Path is injectable for tests.
#[derive(Debug, Clone)]
pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    /// Next to the executable: `settings.json`.
    ///
    /// # Errors
    ///
    /// Returns [`SettingsError::NoExeDir`] if the executable path cannot be resolved.
    pub fn system() -> Result<Self, SettingsError> {
        let dir = paths::exe_dir().map_err(SettingsError::NoExeDir)?;
        Ok(Self {
            path: dir.join("settings.json"),
        })
    }

    /// Use an explicit file path (tests, overrides).
    #[must_use]
    pub fn from_path(path: PathBuf) -> Self {
        Self { path }
    }

    /// Settings file path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Load JSON, or defaults if the file does not exist.
    ///
    /// # Errors
    ///
    /// Returns IO or parse errors. Missing file is not an error.
    pub fn load(&self) -> Result<Settings, SettingsError> {
        if !self.path.exists() {
            return Ok(Settings::default());
        }

        let json = fs::read_to_string(&self.path).map_err(|source| SettingsError::Read {
            path: self.path.clone(),
            source,
        })?;

        serde_json::from_str(&json).map_err(|source| SettingsError::Parse {
            path: self.path.clone(),
            source,
        })
    }

    /// Create parent dirs and write pretty JSON.
    ///
    /// # Errors
    ///
    /// Returns IO or serialize errors.
    pub fn save(&self, settings: &Settings) -> Result<(), SettingsError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|source| SettingsError::CreateDir {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let json = serde_json::to_string_pretty(settings)
            .map_err(|source| SettingsError::Serialize { source })?;

        fs::write(&self.path, json).map_err(|source| SettingsError::Write {
            path: self.path.clone(),
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_settings_path() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);

        std::env::temp_dir().join(format!("cinebox-settings-{nanos}.json"))
    }

    #[test]
    fn settings_roundtrip_json() -> Result<(), serde_json::Error> {
        let settings = Settings::default();
        assert!(settings.general.use_system_proxy);

        let json = serde_json::to_string_pretty(&settings)?;
        let back: Settings = serde_json::from_str(&json)?;
        assert_eq!(settings, back);

        Ok(())
    }

    #[test]
    fn secret_debug_is_redacted() {
        let secret = SecretString::from("hunter2");
        let rendered = format!("{secret:?}");

        assert_eq!(rendered, "***");
        assert!(!rendered.contains("hunter2"));
    }

    #[test]
    fn missing_file_loads_defaults() -> Result<(), SettingsError> {
        let path = temp_settings_path();
        let store = SettingsStore::from_path(path);
        let loaded = store.load()?;

        assert_eq!(loaded, Settings::default());
        Ok(())
    }

    #[test]
    fn save_then_load() -> Result<(), SettingsError> {
        let path = temp_settings_path();
        let store = SettingsStore::from_path(path.clone());
        let mut settings = Settings::default();

        settings.general.language = UiLanguage::Russian;
        settings.parser.kind = ParserKind::Prowlarr;
        settings.tmdb.api_key = SecretString::from("not-a-real-key");
        store.save(&settings)?;

        let loaded = store.load()?;
        let _ = fs::remove_file(&path);

        assert_eq!(loaded.general.language, UiLanguage::Russian);
        assert_eq!(loaded.parser.kind, ParserKind::Prowlarr);
        assert_eq!(loaded.tmdb.api_key.expose(), "not-a-real-key");

        Ok(())
    }

    #[test]
    fn partial_json_fills_defaults() -> Result<(), serde_json::Error> {
        let parsed: Settings = serde_json::from_str(r#"{"general":{"language":"russian"}}"#)?;

        assert_eq!(parsed.general.language, UiLanguage::Russian);
        assert!(parsed.general.use_system_proxy);
        assert_eq!(parsed.torrserver.url, "http://127.0.0.1:8090");
        assert!((parsed.player.volume - 90.0).abs() < f64::EPSILON);

        Ok(())
    }

    #[test]
    fn video_scale_serde_is_snake_case() -> Result<(), serde_json::Error> {
        let json = serde_json::to_string(&VideoScale::Zoom115)?;
        assert_eq!(json, "\"zoom115\"");

        let back: VideoScale = serde_json::from_str("\"expand\"")?;
        assert_eq!(back, VideoScale::Expand);

        assert_eq!(VideoScale::ALL.len(), 5);
        assert_eq!(VideoScale::default(), VideoScale::Default);

        Ok(())
    }

    #[test]
    fn legacy_scale_field_is_ignored() -> Result<(), serde_json::Error> {
        let parsed: Settings =
            serde_json::from_str(r#"{"player":{"scale":"keep_aspect","loudnorm":true}}"#)?;

        assert!(parsed.player.loudnorm);
        assert!((parsed.player.volume - 90.0).abs() < f64::EPSILON);

        Ok(())
    }
}
