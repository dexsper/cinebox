pub(crate) mod category;
pub(crate) mod discover;
pub(crate) mod gate;
pub(crate) mod home;
pub(crate) mod library;
pub(crate) mod media;
pub(crate) mod onboarding;
mod paged;
pub(crate) mod person;
pub(crate) mod play;
pub(crate) mod player;
pub(crate) mod search;
pub(crate) mod section;
pub(crate) mod settings;
pub(crate) mod shelf;
mod swr;
pub(crate) mod torrents;
mod trailers;

pub(crate) use category::CategoryScreen;
pub(crate) use discover::DiscoverScreen;
pub(crate) use home::HomeScreen;
pub(crate) use library::LibraryScreen;
pub(crate) use media::MediaScreen;
pub(crate) use onboarding::OnboardingScreen;
pub(crate) use person::PersonScreen;
pub(crate) use player::PlayerScreen;
pub(crate) use search::SearchScreen;
pub(crate) use section::SectionScreen;
pub(crate) use settings::SettingsScreen;
pub(crate) use torrents::TorrentsScreen;

/// A screen that shows TMDB data fetched for the current key, language, and network.
pub(crate) trait LiveTmdb {
    /// Drop the fetched data so the next frame loads it again with current settings.
    fn forget_live(&mut self);
}
