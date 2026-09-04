//! UI copy via rust-i18n. Active locale is [`apply`].

use std::borrow::Cow;

use cinebox_core::{HomeRowId, MediaDetails, MediaKind, UiLanguage};
use rust_i18n::t;

/// Sync rust-i18n with the settings language.
pub fn apply(lang: UiLanguage) {
    rust_i18n::set_locale(locale_code(lang));
}

fn locale_code(lang: UiLanguage) -> &'static str {
    match lang {
        UiLanguage::English => "en",
        UiLanguage::Russian => "ru",
        UiLanguage::Ukrainian => "uk",
    }
}

/// Look up a key stored as data (settings catalog, etc.).
#[must_use]
pub fn tr(key: &str) -> Cow<'_, str> {
    t!(key)
}

#[must_use]
pub fn home_row_title(id: HomeRowId) -> Cow<'static, str> {
    match id {
        HomeRowId::RecentlyWatched => t!("home.recently_watched"),
        HomeRowId::NowPlaying => t!("home.now_playing"),
        HomeRowId::TrendingDay => t!("home.trending_day"),
        HomeRowId::TrendingWeek => t!("home.trending_week"),
        HomeRowId::PopularMovies => t!("home.popular_movies"),
        HomeRowId::PopularTv => t!("home.popular_tv"),
        HomeRowId::TopRatedMovies => t!("home.top_rated_movies"),
        HomeRowId::TopRatedTv => t!("home.top_rated_tv"),
    }
}

/// Format `125` → `2h 5m` / `2 ч 5 мин`.
#[must_use]
pub fn format_runtime(minutes: u32) -> String {
    let hours = minutes / 60;
    let mins = minutes % 60;

    if hours == 0 {
        return t!("media.runtime.m", mins = mins).into_owned();
    }

    if mins == 0 {
        return t!("media.runtime.h", hours = hours).into_owned();
    }

    t!("media.runtime.hm", hours = hours, mins = mins).into_owned()
}

fn format_seasons(count: u32) -> String {
    if uses_slavic_plural() {
        return match slavic_plural(count) {
            SlavicPlural::One => t!("media.seasons.one", count = count).into_owned(),
            SlavicPlural::Few => t!("media.seasons.few", count = count).into_owned(),
            SlavicPlural::Many => t!("media.seasons.many", count = count).into_owned(),
        };
    }

    if count == 1 {
        return t!("media.seasons.one", count = count).into_owned();
    }

    t!("media.seasons.other", count = count).into_owned()
}

fn format_episodes(count: u32) -> String {
    if uses_slavic_plural() {
        return match slavic_plural(count) {
            SlavicPlural::One => t!("media.episodes.one", count = count).into_owned(),
            SlavicPlural::Few => t!("media.episodes.few", count = count).into_owned(),
            SlavicPlural::Many => t!("media.episodes.many", count = count).into_owned(),
        };
    }

    if count == 1 {
        return t!("media.episodes.one", count = count).into_owned();
    }

    t!("media.episodes.other", count = count).into_owned()
}

fn uses_slavic_plural() -> bool {
    matches!(&*rust_i18n::locale(), "ru" | "uk")
}

enum SlavicPlural {
    One,
    Few,
    Many,
}

fn slavic_plural(n: u32) -> SlavicPlural {
    let n10 = n % 10;
    let n100 = n % 100;

    if n10 == 1 && n100 != 11 {
        return SlavicPlural::One;
    }

    let few_range = (2..=4).contains(&n10);
    let teen = (12..=14).contains(&n100);
    if few_range && !teen {
        return SlavicPlural::Few;
    }

    SlavicPlural::Many
}

/// Runtime (movies) or season/episode counts (TV), then genres.
#[must_use]
pub fn detail_bits(details: &MediaDetails) -> Vec<String> {
    let mut bits = Vec::new();
    match details.kind {
        MediaKind::Tv => {
            if let Some(seasons) = details.number_of_seasons.filter(|n| *n > 0) {
                bits.push(format_seasons(seasons));
            }

            if let Some(episodes) = details.number_of_episodes.filter(|n| *n > 0) {
                bits.push(format_episodes(episodes));
            }
        }
        MediaKind::Movie | MediaKind::Person => {
            if let Some(mins) = details.runtime_minutes.filter(|m| *m > 0) {
                bits.push(format_runtime(mins));
            }
        }
    }

    bits.extend(details.genres.iter().take(5).cloned());
    bits
}

/// Abbreviated month name for `1..=12` in the active UI language.
#[must_use]
pub fn month_abbr(month: usize) -> Option<Cow<'static, str>> {
    match month {
        1 => Some(t!("month.jan")),
        2 => Some(t!("month.feb")),
        3 => Some(t!("month.mar")),
        4 => Some(t!("month.apr")),
        5 => Some(t!("month.may")),
        6 => Some(t!("month.jun")),
        7 => Some(t!("month.jul")),
        8 => Some(t!("month.aug")),
        9 => Some(t!("month.sep")),
        10 => Some(t!("month.oct")),
        11 => Some(t!("month.nov")),
        12 => Some(t!("month.dec")),
        _ => None,
    }
}

/// Format `YYYY-MM-DD` as `22 Oct 2021` / `22 окт 2021`. Other strings pass through.
#[must_use]
pub fn format_release_date(iso: &str) -> String {
    let mut parts = iso.split('-');
    let Some(year) = parts.next() else {
        return iso.to_owned();
    };

    let Some(month) = parts.next().and_then(|m| m.parse::<usize>().ok()) else {
        return iso.to_owned();
    };

    let Some(day) = parts.next() else {
        return iso.to_owned();
    };

    let Some(month_name) = month_abbr(month) else {
        return iso.to_owned();
    };

    let day = day.trim_start_matches('0');
    format!("{day} {month_name} {year}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use cinebox_core::{MediaKind, TmdbId};
    use std::sync::{Mutex, MutexGuard, PoisonError};

    static LOCALE_LOCK: Mutex<()> = Mutex::new(());

    fn lock_locale() -> MutexGuard<'static, ()> {
        let locked = LOCALE_LOCK.lock();
        locked.unwrap_or_else(PoisonError::into_inner)
    }

    fn restore_en() {
        rust_i18n::set_locale("en");
    }

    #[test]
    fn russian_differs_on_chrome() {
        let _guard = lock_locale();
        rust_i18n::set_locale("en");
        assert_eq!(t!("nav.settings").as_ref(), "Settings");

        rust_i18n::set_locale("ru");
        assert_eq!(t!("nav.settings").as_ref(), "Настройки");
        assert_eq!(t!("media.watch").as_ref(), "Смотреть");
        assert_eq!(t!("search.placeholder").as_ref(), "Поиск");
        assert_eq!(t!("search.actors").as_ref(), "Актёры");
        assert_eq!(month_abbr(10).as_deref(), Some("окт"));

        rust_i18n::set_locale("en");
        assert_eq!(month_abbr(10).as_deref(), Some("Oct"));
        assert_eq!(month_abbr(0).as_deref(), None);
        restore_en();
    }

    #[test]
    fn runtime_and_dates_english() {
        let _guard = lock_locale();
        rust_i18n::set_locale("en");

        assert_eq!(format_runtime(125), "2h 5m");
        assert_eq!(format_runtime(60), "1h");
        assert_eq!(format_runtime(9), "9m");
        assert_eq!(format_release_date("2021-10-22"), "22 Oct 2021");
        assert_eq!(format_release_date("2021"), "2021");
        restore_en();
    }

    #[test]
    fn russian_runtime_and_dates() {
        let _guard = lock_locale();
        rust_i18n::set_locale("ru");

        assert_eq!(format_runtime(125), "2 ч 5 мин");
        assert_eq!(format_runtime(60), "1 ч");
        assert_eq!(format_runtime(9), "9 мин");
        assert_eq!(format_release_date("2021-10-22"), "22 окт 2021");
        assert_eq!(format_seasons(1), "1 сезон");
        assert_eq!(format_seasons(2), "2 сезона");
        assert_eq!(format_seasons(5), "5 сезонов");
        assert_eq!(format_episodes(21), "21 серия");
        assert_eq!(format_episodes(22), "22 серии");
        assert_eq!(format_episodes(11), "11 серий");

        restore_en();
    }

    #[test]
    fn ukrainian_runtime_and_dates() {
        let _guard = lock_locale();
        rust_i18n::set_locale("uk");

        assert_eq!(t!("nav.settings").as_ref(), "Налаштування");
        assert_eq!(format_runtime(125), "2 год 5 хв");
        assert_eq!(format_runtime(60), "1 год");
        assert_eq!(format_runtime(9), "9 хв");
        assert_eq!(format_release_date("2021-10-22"), "22 жов 2021");
        assert_eq!(format_seasons(1), "1 сезон");
        assert_eq!(format_seasons(2), "2 сезони");
        assert_eq!(format_seasons(5), "5 сезонів");
        assert_eq!(format_episodes(21), "21 серія");
        assert_eq!(format_episodes(22), "22 серії");
        assert_eq!(format_episodes(11), "11 серій");

        restore_en();
    }

    #[test]
    fn detail_bits_english() {
        let _guard = lock_locale();
        rust_i18n::set_locale("en");

        let details = MediaDetails {
            id: TmdbId::new(1),
            kind: MediaKind::Movie,
            title: String::from("Dune"),
            original_title: None,
            original_language: None,
            tagline: None,
            overview: None,
            year: Some(2021),
            released: Some(String::from("2021-10-22")),
            runtime_minutes: Some(155),
            number_of_seasons: None,
            number_of_episodes: None,
            certification: None,
            vote: Some(8.1),
            budget: None,
            genre_ids: Vec::new(),
            genres: vec![String::from("Sci-Fi"), String::from("Adventure")],
            countries: vec![String::from("United States")],
            poster_path: None,
            backdrop_path: None,
            directors: Vec::new(),
            cast: Vec::new(),
            collection: Vec::new(),
            recommendations: Vec::new(),
            similar: Vec::new(),
            trailers: Vec::new(),
        };

        assert_eq!(detail_bits(&details), vec!["2h 35m", "Sci-Fi", "Adventure"]);

        let tv = MediaDetails {
            kind: MediaKind::Tv,
            runtime_minutes: Some(47),
            number_of_seasons: Some(5),
            number_of_episodes: Some(62),
            ..details
        };

        assert_eq!(
            detail_bits(&tv),
            vec!["5 seasons", "62 episodes", "Sci-Fi", "Adventure"]
        );

        restore_en();
    }
}
