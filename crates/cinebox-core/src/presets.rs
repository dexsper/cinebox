//! Suggested service addresses for first-run discovery, kept as data.
//!
//! The list lives in `presets/services.json` at the repository root. The app
//! fetches the latest copy from [`presets_url`], falls back to the copy built
//! into the binary, and appends entries from a `presets.json` the user may put
//! next to `settings.json`. Presets only suggest addresses: every candidate is
//! probed and nothing is applied until the viewer picks it.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Deserializer, Serialize};

use crate::settings::ParserKind;

/// Highest file format this build understands.
pub const PRESETS_VERSION: u32 = 1;

const PRESETS_HOST: &str = "https://raw.githubusercontent.com";
const PRESETS_PATH: &str = "dexsper/cinebox/master/presets/services.json";

/// File name of the user's own additions, next to `settings.json`.
pub const USER_PRESETS_FILE: &str = "presets.json";

const BUNDLED: &str = include_str!("../../../presets/services.json");

const DEFAULT_MDNS_SERVICE: &str = "_torrserver._tcp.local.";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServicePresets {
    pub version: u32,
    #[serde(default, deserialize_with = "skip_bad_entries")]
    pub parsers: Vec<ParserPreset>,
    #[serde(default)]
    pub torrserver: TorrPresets,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParserPreset {
    pub kind: ParserKind,
    pub url: String,
    /// A shared server that works without an API key.
    #[serde(default)]
    pub public: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TorrPresets {
    /// DNS-SD service type TorrServer announces itself under.
    pub mdns_service: String,
    /// Addresses tried when nothing answers over mDNS.
    #[serde(default)]
    pub fallback: Vec<String>,
}

impl Default for TorrPresets {
    fn default() -> Self {
        Self {
            mdns_service: DEFAULT_MDNS_SERVICE.to_owned(),
            fallback: Vec::new(),
        }
    }
}

/// Where the latest list is published.
#[must_use]
pub fn presets_url() -> String {
    format!("{PRESETS_HOST}/{PRESETS_PATH}")
}

impl ServicePresets {
    /// The copy built into the binary.
    #[must_use]
    pub fn bundled() -> Self {
        Self::parse(BUNDLED).unwrap_or_default()
    }

    /// `None` for invalid JSON or a format newer than [`PRESETS_VERSION`].
    #[must_use]
    pub fn parse(json: &str) -> Option<Self> {
        let mut presets: Self = serde_json::from_str(json).ok()?;
        if presets.version > PRESETS_VERSION {
            return None;
        }

        presets.parsers.retain(ParserPreset::is_usable);
        Some(presets)
    }

    /// The user's `presets.json` in `config_dir`, if present and valid.
    #[must_use]
    pub fn load_user(config_dir: &Path) -> Option<Self> {
        let json = fs::read_to_string(config_dir.join(USER_PRESETS_FILE)).ok()?;
        Self::parse(&json)
    }

    /// Append `extra` entries whose address is not listed yet.
    #[must_use]
    pub fn merged_with(mut self, extra: Self) -> Self {
        for parser in extra.parsers {
            let known = self.parsers.iter().any(|known| known.url == parser.url);
            if !known {
                self.parsers.push(parser);
            }
        }

        for url in extra.torrserver.fallback {
            if !self.torrserver.fallback.contains(&url) {
                self.torrserver.fallback.push(url);
            }
        }

        self
    }
}

impl ParserPreset {
    /// Public servers are reached over the internet, so they must use TLS.
    fn is_usable(&self) -> bool {
        if self.url.is_empty() {
            return false;
        }

        !self.public || self.url.starts_with("https://")
    }
}

/// A malformed entry drops only itself, not the whole list.
fn skip_bad_entries<'de, D>(deserializer: D) -> Result<Vec<ParserPreset>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = Vec::<serde_json::Value>::deserialize(deserializer)?;
    let parsed = raw.into_iter().filter_map(parse_entry);

    Ok(parsed.collect())
}

fn parse_entry(value: serde_json::Value) -> Option<ParserPreset> {
    serde_json::from_value(value).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_file_is_valid() {
        let Some(presets) = ServicePresets::parse(BUNDLED) else {
            panic!("bundled presets must parse");
        };

        assert!(!presets.parsers.is_empty());
        assert_eq!(presets.torrserver.mdns_service, DEFAULT_MDNS_SERVICE);
    }

    #[test]
    fn newer_format_is_rejected() {
        let json = r#"{ "version": 99, "parsers": [] }"#;

        assert_eq!(ServicePresets::parse(json), None);
    }

    #[test]
    fn public_server_needs_https() {
        let json = r#"{
            "version": 1,
            "parsers": [
                { "kind": "jackett", "url": "http://open.example", "public": true },
                { "kind": "jackett", "url": "http://127.0.0.1:9117" }
            ]
        }"#;

        let Some(presets) = ServicePresets::parse(json) else {
            panic!("valid json");
        };
        let urls: Vec<&str> = presets.parsers.iter().map(|p| p.url.as_str()).collect();
        assert_eq!(urls, vec!["http://127.0.0.1:9117"]);
    }

    #[test]
    fn bad_entry_drops_only_itself() {
        let json = r#"{
            "version": 1,
            "parsers": [
                { "kind": "sonarr", "url": "https://x.example" },
                { "kind": "prowlarr", "url": "http://127.0.0.1:9696" }
            ]
        }"#;

        let Some(presets) = ServicePresets::parse(json) else {
            panic!("valid json");
        };
        assert_eq!(presets.parsers.len(), 1);
    }

    #[test]
    fn merge_appends_new_addresses_only() {
        let base = ServicePresets::bundled();
        let before = base.parsers.len();
        let mine = ParserPreset {
            kind: ParserKind::Jackett,
            url: String::from("http://nas.local:9117"),
            public: false,
        };
        let extra = ServicePresets {
            version: 1,
            parsers: vec![base.parsers[0].clone(), mine],
            torrserver: TorrPresets::default(),
        };

        let merged = base.merged_with(extra);
        assert_eq!(merged.parsers.len(), before + 1);
    }
}
