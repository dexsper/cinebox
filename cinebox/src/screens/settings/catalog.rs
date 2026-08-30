//! Settings categories and the field list the drawer renders.

use cinebox_core::{SecretString, Settings};
use egui_material_icons::MaterialIcon;
use egui_material_icons::icons::{
    ICON_CLOUD, ICON_MOVIE, ICON_PLAY_CIRCLE, ICON_SEARCH, ICON_TUNE,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CategoryId {
    General,
    Player,
    Parser,
    TorrServer,
    Tmdb,
}

pub struct Category {
    pub id: CategoryId,
    pub title: &'static str,
    pub subtitle: &'static str,
    pub icon: MaterialIcon,
    pub fields: &'static [Field],
}

pub enum SelectId {
    Language,
    Scale,
    Quality,
    ParserKind,
    PosterSize,
}

pub enum Field {
    Toggle {
        label: &'static str,
        hint: Option<&'static str>,
        get: fn(&Settings) -> bool,
        set: fn(&mut Settings, bool),
    },
    Text {
        label: &'static str,
        hint: Option<&'static str>,
        placeholder: &'static str,
        get: fn(&Settings) -> String,
        set: fn(&mut Settings, String),
    },
    Secret {
        label: &'static str,
        hint: Option<&'static str>,
        get: fn(&Settings) -> SecretString,
        set: fn(&mut Settings, SecretString),
    },
    Select {
        id: &'static str,
        label: &'static str,
        hint: Option<&'static str>,
        which: SelectId,
    },
    DataLanguage,
    ProbeParser,
    ProbeTorr,
    ProbeTmdb,
    SpeedTest,
    ClearCache,
}

pub fn catalog() -> &'static [Category] {
    CATALOG
}

pub fn category(id: CategoryId) -> &'static Category {
    for cat in catalog() {
        if cat.id == id {
            return cat;
        }
    }

    &CATALOG[0]
}

const CATALOG: &[Category] = &[
    Category {
        id: CategoryId::General,
        title: "General",
        subtitle: "Language and proxy",
        icon: ICON_TUNE,
        fields: GENERAL,
    },
    Category {
        id: CategoryId::Player,
        title: "Player",
        subtitle: "Playback and quality",
        icon: ICON_PLAY_CIRCLE,
        fields: PLAYER,
    },
    Category {
        id: CategoryId::Parser,
        title: "Parser",
        subtitle: "Jackett or Prowlarr",
        icon: ICON_SEARCH,
        fields: PARSER,
    },
    Category {
        id: CategoryId::TorrServer,
        title: "TorrServer",
        subtitle: "Streaming backend",
        icon: ICON_CLOUD,
        fields: TORRSERVER,
    },
    Category {
        id: CategoryId::Tmdb,
        title: "TMDB",
        subtitle: "Catalog and posters",
        icon: ICON_MOVIE,
        fields: TMDB,
    },
];

const GENERAL: &[Field] = &[
    Field::Select {
        id: "language",
        label: "Language",
        hint: None,
        which: SelectId::Language,
    },
    Field::Toggle {
        label: "Use system proxy",
        hint: Some("TMDB and parser. TorrServer always connects directly."),
        get: |s| s.interface.use_system_proxy,
        set: |s, v| s.interface.use_system_proxy = v,
    },
];

const PLAYER: &[Field] = &[
    Field::Toggle {
        label: "Loudnorm",
        hint: Some("Normalize volume across files."),
        get: |s| s.player.loudnorm,
        set: |s, v| s.player.loudnorm = v,
    },
    Field::Toggle {
        label: "Play next file automatically",
        hint: None,
        get: |s| s.player.auto_next,
        set: |s, v| s.player.auto_next = v,
    },
    Field::Toggle {
        label: "Save timecode",
        hint: Some("Resume where you left off."),
        get: |s| s.player.save_timecode,
        set: |s, v| s.player.save_timecode = v,
    },
    Field::Select {
        id: "scale",
        label: "Scale",
        hint: None,
        which: SelectId::Scale,
    },
    Field::Select {
        id: "quality",
        label: "Default quality",
        hint: None,
        which: SelectId::Quality,
    },
];

const PARSER: &[Field] = &[
    Field::Select {
        id: "parser-kind",
        label: "Type",
        hint: None,
        which: SelectId::ParserKind,
    },
    Field::Text {
        label: "URL",
        hint: None,
        placeholder: "http://127.0.0.1:9117",
        get: |s| s.parser.url.clone(),
        set: |s, v| s.parser.url = v,
    },
    Field::Secret {
        label: "API key",
        hint: None,
        get: |s| s.parser.api_key.clone(),
        set: |s, v| s.parser.api_key = v,
    },
    Field::ProbeParser,
];

const TORRSERVER: &[Field] = &[
    Field::Text {
        label: "URL",
        hint: None,
        placeholder: "http://127.0.0.1:8090",
        get: |s| s.torrserver.url.clone(),
        set: |s, v| s.torrserver.url = v,
    },
    Field::Toggle {
        label: "Save torrents to server DB",
        hint: None,
        get: |s| s.torrserver.save_to_db,
        set: |s, v| s.torrserver.save_to_db = v,
    },
    Field::Toggle {
        label: "Wait for preload",
        hint: None,
        get: |s| s.torrserver.wait_preload,
        set: |s, v| s.torrserver.wait_preload = v,
    },
    Field::Toggle {
        label: "Track timecode on server",
        hint: None,
        get: |s| s.torrserver.track_timecode,
        set: |s, v| s.torrserver.track_timecode = v,
    },
    Field::Text {
        label: "Username",
        hint: None,
        placeholder: "",
        get: |s| s.torrserver.username.clone(),
        set: |s, v| s.torrserver.username = v,
    },
    Field::Secret {
        label: "Password",
        hint: None,
        get: |s| s.torrserver.password.clone(),
        set: |s, v| s.torrserver.password = v,
    },
    Field::ProbeTorr,
    Field::SpeedTest,
];

const TMDB: &[Field] = &[
    Field::Secret {
        label: "API key",
        hint: Some("Short API key from themoviedb.org (32 hex). Not the JWT access token."),
        get: |s| s.tmdb.api_key.clone(),
        set: |s, v| s.tmdb.api_key = v,
    },
    Field::DataLanguage,
    Field::Select {
        id: "poster-size",
        label: "Poster size",
        hint: None,
        which: SelectId::PosterSize,
    },
    Field::ProbeTmdb,
    Field::ClearCache,
];
