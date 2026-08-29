//! English UI strings for Phase 1. Locale switching is Phase 8.

/// Message keys used by the Iced shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Msg {
    AppTitle,
    NavSettings,
    NavBack,
    HomeTitle,
    SettingsTitle,
    SettingsPath,
    SettingsLoadError,
    SettingsApiKeySet,
    SettingsApiKeyMissing,
    EmptyRow,
    NeedTmdbKey,
    LoadingHome,
}

impl Msg {
    /// English copy. Other locales land in Phase 8.
    #[must_use]
    pub const fn en(self) -> &'static str {
        match self {
            Self::AppTitle => "Cinebox",
            Self::NavSettings => "Settings",
            Self::NavBack => "Back",
            Self::HomeTitle => "Home",
            Self::SettingsTitle => "Settings",
            Self::SettingsPath => "Config file",
            Self::SettingsLoadError => "Could not load settings; using defaults.",
            Self::SettingsApiKeySet => "set",
            Self::SettingsApiKeyMissing => "not set",
            Self::EmptyRow => "Nothing here yet",
            Self::NeedTmdbKey => "Set a TMDB API key in Settings to load the catalog.",
            Self::LoadingHome => "Loading catalog…",
        }
    }
}

/// Placeholder home rows matching the ТЗ catalog sections.
#[must_use]
pub fn home_row_titles() -> &'static [&'static str] {
    use crate::catalog::HomeRowId;
    const TITLES: [&str; 8] = [
        HomeRowId::RecentlyWatched.title(),
        HomeRowId::NowPlaying.title(),
        HomeRowId::TrendingDay.title(),
        HomeRowId::TrendingWeek.title(),
        HomeRowId::PopularMovies.title(),
        HomeRowId::PopularTv.title(),
        HomeRowId::TopRatedMovies.title(),
        HomeRowId::TopRatedTv.title(),
    ];
    &TITLES
}
