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

impl CategoryId {
    #[must_use]
    pub fn as_key(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Player => "player",
            Self::Parser => "parser",
            Self::TorrServer => "torrserver",
            Self::Tmdb => "tmdb",
        }
    }
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
    ParserKind,
    PosterSize,
}

pub enum MultiSelectId {
    Quality,
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
    MultiSelect {
        id: &'static str,
        label: &'static str,
        hint: Option<&'static str>,
        which: MultiSelectId,
    },
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
        title: "settings.general",
        subtitle: "settings.general_hint",
        icon: ICON_TUNE,
        fields: GENERAL,
    },
    Category {
        id: CategoryId::Player,
        title: "settings.player",
        subtitle: "settings.player_hint",
        icon: ICON_PLAY_CIRCLE,
        fields: PLAYER,
    },
    Category {
        id: CategoryId::Parser,
        title: "settings.parser",
        subtitle: "settings.parser_hint",
        icon: ICON_SEARCH,
        fields: PARSER,
    },
    Category {
        id: CategoryId::TorrServer,
        title: "settings.torrserver",
        subtitle: "settings.torrserver_hint",
        icon: ICON_CLOUD,
        fields: TORRSERVER,
    },
    Category {
        id: CategoryId::Tmdb,
        title: "settings.tmdb",
        subtitle: "settings.tmdb_hint",
        icon: ICON_MOVIE,
        fields: TMDB,
    },
];

const GENERAL: &[Field] = &[
    Field::Select {
        id: "language",
        label: "filter.language",
        hint: None,
        which: SelectId::Language,
    },
    Field::Toggle {
        label: "settings.use_system_proxy",
        hint: Some("settings.use_system_proxy_hint"),
        get: |s| s.general.use_system_proxy,
        set: |s, v| s.general.use_system_proxy = v,
    },
    Field::Toggle {
        label: "settings.dns_bypass",
        hint: Some("settings.dns_bypass_hint"),
        get: |s| s.general.dns_bypass,
        set: |s, v| s.general.dns_bypass = v,
    },
    Field::Text {
        label: "settings.custom_doh_url",
        hint: Some("settings.custom_doh_url_hint"),
        placeholder: "https://dns.example.com/dns-query",
        get: |s| s.general.custom_doh_url.clone(),
        set: |s, v| s.general.custom_doh_url = v,
    },
];

const PLAYER: &[Field] = &[
    Field::Toggle {
        label: "settings.loudnorm",
        hint: Some("settings.loudnorm_hint"),
        get: |s| s.player.loudnorm,
        set: |s, v| s.player.loudnorm = v,
    },
    Field::Toggle {
        label: "settings.play_next",
        hint: None,
        get: |s| s.player.auto_next,
        set: |s, v| s.player.auto_next = v,
    },
];

const PARSER: &[Field] = &[
    Field::Select {
        id: "parser-kind",
        label: "settings.parser_type",
        hint: None,
        which: SelectId::ParserKind,
    },
    Field::Text {
        label: "settings.url",
        hint: None,
        placeholder: "http://127.0.0.1:9117",
        get: |s| s.parser.url.clone(),
        set: |s, v| s.parser.url = v,
    },
    Field::Secret {
        label: "settings.api_key",
        hint: None,
        get: |s| s.parser.api_key.clone(),
        set: |s, v| s.parser.api_key = v,
    },
    Field::MultiSelect {
        id: "parser-quality",
        label: "settings.default_quality",
        hint: None,
        which: MultiSelectId::Quality,
    },
    Field::ProbeParser,
];

const TORRSERVER: &[Field] = &[
    Field::Text {
        label: "settings.url",
        hint: None,
        placeholder: "http://127.0.0.1:8090",
        get: |s| s.torrserver.url.clone(),
        set: |s, v| s.torrserver.url = v,
    },
    Field::Toggle {
        label: "settings.save_torrents",
        hint: None,
        get: |s| s.torrserver.save_to_db,
        set: |s, v| s.torrserver.save_to_db = v,
    },
    Field::Toggle {
        label: "settings.wait_preload",
        hint: None,
        get: |s| s.torrserver.wait_preload,
        set: |s, v| s.torrserver.wait_preload = v,
    },
    Field::Toggle {
        label: "settings.track_timecode",
        hint: None,
        get: |s| s.torrserver.track_timecode,
        set: |s, v| s.torrserver.track_timecode = v,
    },
    Field::Text {
        label: "settings.username",
        hint: None,
        placeholder: "",
        get: |s| s.torrserver.username.clone(),
        set: |s, v| s.torrserver.username = v,
    },
    Field::Secret {
        label: "settings.password",
        hint: None,
        get: |s| s.torrserver.password.clone(),
        set: |s, v| s.torrserver.password = v,
    },
    Field::ProbeTorr,
    Field::SpeedTest,
];

const TMDB: &[Field] = &[
    Field::Secret {
        label: "settings.api_key",
        hint: Some("settings.tmdb_api_key_hint"),
        get: |s| s.tmdb.api_key.clone(),
        set: |s, v| s.tmdb.api_key = v,
    },
    Field::Select {
        id: "poster-size",
        label: "settings.poster_size",
        hint: None,
        which: SelectId::PosterSize,
    },
    Field::ProbeTmdb,
    Field::ClearCache,
];
