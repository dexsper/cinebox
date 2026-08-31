//! Settings categories and the field list the drawer renders.

use cinebox_core::i18n::Msg;
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
    pub title: Msg,
    pub subtitle: Msg,
    pub icon: MaterialIcon,
    pub fields: &'static [Field],
}

pub enum SelectId {
    Language,
    Scale,
    ParserKind,
    PosterSize,
}

pub enum MultiSelectId {
    Quality,
}

pub enum Field {
    Toggle {
        label: Msg,
        hint: Option<Msg>,
        get: fn(&Settings) -> bool,
        set: fn(&mut Settings, bool),
    },
    Text {
        label: Msg,
        hint: Option<Msg>,
        placeholder: &'static str,
        get: fn(&Settings) -> String,
        set: fn(&mut Settings, String),
    },
    Secret {
        label: Msg,
        hint: Option<Msg>,
        get: fn(&Settings) -> SecretString,
        set: fn(&mut Settings, SecretString),
    },
    Select {
        id: &'static str,
        label: Msg,
        hint: Option<Msg>,
        which: SelectId,
    },
    MultiSelect {
        id: &'static str,
        label: Msg,
        hint: Option<Msg>,
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
        title: Msg::SettingsGeneral,
        subtitle: Msg::SettingsGeneralHint,
        icon: ICON_TUNE,
        fields: GENERAL,
    },
    Category {
        id: CategoryId::Player,
        title: Msg::SettingsPlayer,
        subtitle: Msg::SettingsPlayerHint,
        icon: ICON_PLAY_CIRCLE,
        fields: PLAYER,
    },
    Category {
        id: CategoryId::Parser,
        title: Msg::SettingsParser,
        subtitle: Msg::SettingsParserHint,
        icon: ICON_SEARCH,
        fields: PARSER,
    },
    Category {
        id: CategoryId::TorrServer,
        title: Msg::SettingsTorrServer,
        subtitle: Msg::SettingsTorrServerHint,
        icon: ICON_CLOUD,
        fields: TORRSERVER,
    },
    Category {
        id: CategoryId::Tmdb,
        title: Msg::SettingsTmdb,
        subtitle: Msg::SettingsTmdbHint,
        icon: ICON_MOVIE,
        fields: TMDB,
    },
];

const GENERAL: &[Field] = &[
    Field::Select {
        id: "language",
        label: Msg::FilterLanguage,
        hint: None,
        which: SelectId::Language,
    },
    Field::Toggle {
        label: Msg::UseSystemProxy,
        hint: Some(Msg::UseSystemProxyHint),
        get: |s| s.general.use_system_proxy,
        set: |s, v| s.general.use_system_proxy = v,
    },
];

const PLAYER: &[Field] = &[
    Field::Toggle {
        label: Msg::Loudnorm,
        hint: Some(Msg::LoudnormHint),
        get: |s| s.player.loudnorm,
        set: |s, v| s.player.loudnorm = v,
    },
    Field::Toggle {
        label: Msg::PlayNextAutomatically,
        hint: None,
        get: |s| s.player.auto_next,
        set: |s, v| s.player.auto_next = v,
    },
    Field::Toggle {
        label: Msg::SaveTimecode,
        hint: Some(Msg::SaveTimecodeHint),
        get: |s| s.player.save_timecode,
        set: |s, v| s.player.save_timecode = v,
    },
    Field::Select {
        id: "scale",
        label: Msg::Scale,
        hint: None,
        which: SelectId::Scale,
    },
];

const PARSER: &[Field] = &[
    Field::Select {
        id: "parser-kind",
        label: Msg::ParserType,
        hint: None,
        which: SelectId::ParserKind,
    },
    Field::Text {
        label: Msg::Url,
        hint: None,
        placeholder: "http://127.0.0.1:9117",
        get: |s| s.parser.url.clone(),
        set: |s, v| s.parser.url = v,
    },
    Field::Secret {
        label: Msg::ApiKey,
        hint: None,
        get: |s| s.parser.api_key.clone(),
        set: |s, v| s.parser.api_key = v,
    },
    Field::MultiSelect {
        id: "parser-quality",
        label: Msg::DefaultQuality,
        hint: None,
        which: MultiSelectId::Quality,
    },
    Field::ProbeParser,
];

const TORRSERVER: &[Field] = &[
    Field::Text {
        label: Msg::Url,
        hint: None,
        placeholder: "http://127.0.0.1:8090",
        get: |s| s.torrserver.url.clone(),
        set: |s, v| s.torrserver.url = v,
    },
    Field::Toggle {
        label: Msg::SaveTorrentsToDb,
        hint: None,
        get: |s| s.torrserver.save_to_db,
        set: |s, v| s.torrserver.save_to_db = v,
    },
    Field::Toggle {
        label: Msg::WaitForPreload,
        hint: None,
        get: |s| s.torrserver.wait_preload,
        set: |s, v| s.torrserver.wait_preload = v,
    },
    Field::Toggle {
        label: Msg::TrackTimecodeOnServer,
        hint: None,
        get: |s| s.torrserver.track_timecode,
        set: |s, v| s.torrserver.track_timecode = v,
    },
    Field::Text {
        label: Msg::Username,
        hint: None,
        placeholder: "",
        get: |s| s.torrserver.username.clone(),
        set: |s, v| s.torrserver.username = v,
    },
    Field::Secret {
        label: Msg::Password,
        hint: None,
        get: |s| s.torrserver.password.clone(),
        set: |s, v| s.torrserver.password = v,
    },
    Field::ProbeTorr,
    Field::SpeedTest,
];

const TMDB: &[Field] = &[
    Field::Secret {
        label: Msg::ApiKey,
        hint: Some(Msg::TmdbApiKeyHint),
        get: |s| s.tmdb.api_key.clone(),
        set: |s, v| s.tmdb.api_key = v,
    },
    Field::Select {
        id: "poster-size",
        label: Msg::PosterSize,
        hint: None,
        which: SelectId::PosterSize,
    },
    Field::ProbeTmdb,
    Field::ClearCache,
];
