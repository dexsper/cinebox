//! Settings categories and the field list the drawer renders.

use cinebox_core::{SecretString, Settings};
use cinebox_player::Features;

use crate::app::nav::SettingsPage;
use crate::services::settings_input::{InputKind, KeyHint, doh_url_ok, tmdb_key_hint};
use egui_material_icons::MaterialIcon;
use egui_material_icons::icons::{ICON_CLOUD, ICON_MOVIE, ICON_PLAY_CIRCLE, ICON_SEARCH, ICON_TUNE};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CategoryId {
    General,
    Player,
    Parser,
    TorrServer,
    Tmdb,
}

impl CategoryId {
    #[must_use]
    pub(super) fn as_key(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Player => "player",
            Self::Parser => "parser",
            Self::TorrServer => "torrserver",
            Self::Tmdb => "tmdb",
        }
    }
}

impl From<SettingsPage> for CategoryId {
    fn from(page: SettingsPage) -> Self {
        match page {
            SettingsPage::General => Self::General,
            SettingsPage::Parser => Self::Parser,
            SettingsPage::TorrServer => Self::TorrServer,
            SettingsPage::Tmdb => Self::Tmdb,
        }
    }
}

pub(super) struct Category {
    pub id: CategoryId,
    pub title: &'static str,
    pub subtitle: &'static str,
    pub icon: MaterialIcon,
    pub fields: &'static [Field],
}

pub(super) enum SelectId {
    Language,
    ParserKind,
    PosterSize,
}

pub(super) enum MultiSelectId {
    Quality,
    HomeRows,
}

pub(super) enum Field {
    Toggle {
        label: &'static str,
        hint: Option<&'static str>,
        get: fn(&Settings) -> bool,
        set: fn(&mut Settings, bool),
        /// While this returns `true` the toggle shows on and takes no clicks; the stored value is kept.
        forced: Option<fn(&Settings) -> bool>,
    },
    Text {
        label: &'static str,
        hint: Option<&'static str>,
        placeholder: &'static str,
        kind: InputKind,
        check: Option<Check>,
        get: fn(&Settings) -> String,
        set: fn(&mut Settings, String),
    },
    Secret {
        label: &'static str,
        hint: Option<&'static str>,
        kind: InputKind,
        check: Option<Check>,
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
    /// `field`, while the player has what it sets.
    PlayerOnly {
        needs: fn(&Features) -> bool,
        field: &'static Self,
    },
    ProbeParser,
    ProbeTorr,
    ProbeTmdb,
    SpeedTest,
    ClearCache,
}

/// Returns the i18n key of a warning shown under the committed value.
pub(super) type Check = fn(&str) -> Option<&'static str>;

fn check_tmdb_key(key: &str) -> Option<&'static str> {
    tmdb_key_hint(key).map(|hint| match hint {
        KeyHint::AccessToken => "settings.tmdb_key_token",
        KeyHint::BadFormat => "settings.tmdb_key_format",
    })
}

fn check_doh_url(url: &str) -> Option<&'static str> {
    (!doh_url_ok(url)).then_some("settings.doh_needs_https")
}

pub(super) fn catalog() -> &'static [Category] {
    CATALOG
}

pub(super) fn category(id: CategoryId) -> &'static Category {
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
    Field::Select { id: "language", label: "filter.language", hint: None, which: SelectId::Language },
    Field::MultiSelect { id: "home-rows", label: "settings.home_shelves", hint: None, which: MultiSelectId::HomeRows },
    Field::Toggle {
        label: "settings.use_system_proxy",
        hint: Some("settings.use_system_proxy_hint"),
        get: |s| s.general.use_system_proxy,
        set: |s, v| s.general.use_system_proxy = v,
        forced: None,
    },
    Field::Toggle {
        label: "settings.dns_bypass",
        hint: Some("settings.dns_bypass_hint"),
        get: |s| s.general.dns_bypass,
        set: |s, v| s.general.dns_bypass = v,
        forced: None,
    },
    Field::Text {
        label: "settings.custom_doh_url",
        hint: Some("settings.custom_doh_url_hint"),
        placeholder: "https://dns.example.com/dns-query",
        kind: InputKind::Url,
        check: Some(check_doh_url),
        get: |s| s.general.custom_doh_url.clone(),
        set: |s, v| s.general.custom_doh_url = v,
    },
];

const PLAYER: &[Field] = &[
    Field::PlayerOnly {
        needs: |features| features.decoder_choice,
        field: &Field::Toggle {
            label: "settings.hardware_decoding",
            hint: Some("settings.hardware_decoding_hint"),
            get: |s| s.player.hardware_decoding,
            set: |s, v| s.player.hardware_decoding = v,
            forced: None,
        },
    },
    Field::PlayerOnly {
        needs: |features| features.loudnorm,
        field: &Field::Toggle {
            label: "settings.loudnorm",
            hint: Some("settings.loudnorm_hint"),
            get: |s| s.player.loudnorm,
            set: |s, v| s.player.loudnorm = v,
            forced: None,
        },
    },
    Field::Toggle {
        label: "settings.play_next",
        hint: None,
        get: |s| s.player.auto_next,
        set: |s, v| s.player.auto_next = v,
        forced: None,
    },
    Field::Toggle {
        label: "settings.skip_intro",
        hint: None,
        get: |s| s.player.skip_segments.intro,
        set: |s, v| s.player.skip_segments.intro = v,
        forced: None,
    },
    Field::Toggle {
        label: "settings.skip_recap",
        hint: None,
        get: |s| s.player.skip_segments.recap,
        set: |s, v| s.player.skip_segments.recap = v,
        forced: None,
    },
    Field::Toggle {
        label: "settings.skip_credits",
        hint: None,
        get: |s| s.player.skip_segments.credits,
        set: |s, v| s.player.skip_segments.credits = v,
        forced: None,
    },
    Field::Toggle {
        label: "settings.skip_preview",
        hint: None,
        get: |s| s.player.skip_segments.preview,
        set: |s, v| s.player.skip_segments.preview = v,
        forced: None,
    },
];

const PARSER: &[Field] = &[
    Field::Select { id: "parser-kind", label: "settings.parser_type", hint: None, which: SelectId::ParserKind },
    Field::Text {
        label: "settings.url",
        hint: None,
        placeholder: "http://127.0.0.1:9117",
        kind: InputKind::Url,
        check: None,
        get: |s| s.parser.url.clone(),
        set: |s, v| s.parser.url = v,
    },
    Field::Secret {
        label: "settings.api_key",
        hint: None,
        kind: InputKind::Key,
        check: None,
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
        kind: InputKind::Url,
        check: None,
        get: |s| s.torrserver.url.clone(),
        set: |s, v| s.torrserver.url = v,
    },
    Field::Text {
        label: "settings.username",
        hint: None,
        placeholder: "",
        kind: InputKind::Plain,
        check: None,
        get: |s| s.torrserver.username.clone(),
        set: |s, v| s.torrserver.username = v,
    },
    Field::Secret {
        label: "settings.password",
        hint: None,
        kind: InputKind::Raw,
        check: None,
        get: |s| s.torrserver.password.clone(),
        set: |s, v| s.torrserver.password = v,
    },
    Field::Toggle {
        label: "settings.save_torrents",
        hint: Some("settings.save_torrents_hint"),
        get: |s| s.torrserver.save_to_db,
        set: |s, v| s.torrserver.save_to_db = v,
        forced: None,
    },
    Field::Toggle {
        label: "settings.wait_preload",
        hint: Some("settings.wait_preload_hint"),
        get: |s| s.torrserver.wait_preload,
        set: |s, v| s.torrserver.wait_preload = v,
        forced: None,
    },
    Field::Toggle {
        label: "settings.save_position",
        hint: Some("settings.save_position_hint"),
        get: |s| s.torrserver.track_timecode,
        set: |s, v| s.torrserver.track_timecode = v,
        // Continuing elsewhere reads and writes positions on the server either way.
        forced: Some(|s| s.torrserver.sync_history),
    },
    Field::Toggle {
        label: "settings.continue_elsewhere",
        hint: Some("settings.continue_elsewhere_hint"),
        get: |s| s.torrserver.sync_history,
        set: |s, v| s.torrserver.sync_history = v,
        forced: None,
    },
    Field::ProbeTorr,
    Field::SpeedTest,
];

const TMDB: &[Field] = &[
    Field::Secret {
        label: "settings.api_key",
        hint: Some("settings.tmdb_api_key_hint"),
        kind: InputKind::Key,
        check: Some(check_tmdb_key),
        get: |s| s.tmdb.api_key.clone(),
        set: |s, v| s.tmdb.api_key = v,
    },
    Field::Select { id: "poster-size", label: "settings.poster_size", hint: None, which: SelectId::PosterSize },
    Field::ProbeTmdb,
    Field::ClearCache,
];
