pub mod category;
pub mod discover;
pub(crate) mod gate;
pub mod home;
pub mod library;
pub mod media;
pub mod onboarding;
mod paged;
pub mod person;
pub mod play;
pub mod player;
pub mod search;
pub mod section;
pub mod settings;
pub(crate) mod shelf;
mod swr;
mod trailers;
pub mod torrents;

pub use category::CategoryScreen;
pub use discover::DiscoverScreen;
pub use home::HomeScreen;
pub use library::LibraryScreen;
pub use media::MediaScreen;
pub use onboarding::OnboardingScreen;
pub use person::PersonScreen;
pub use player::PlayerScreen;
pub use search::SearchScreen;
pub use section::SectionScreen;
pub use settings::SettingsScreen;
pub use torrents::TorrentsScreen;

/// A screen that shows TMDB data fetched for the current key, language, and network.
pub trait LiveTmdb {
    /// Drop the fetched data so the next frame loads it again with current settings.
    fn forget_live(&mut self);
}
