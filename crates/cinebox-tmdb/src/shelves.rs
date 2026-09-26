//! Shelf identities, section hub layouts, and the TMDB request behind each shelf.

use cinebox_core::{CatalogItem, HomeRowId, MediaKind, Section};
use cinebox_net::NetConfig;
use futures_util::future::join_all;
use serde::{Deserialize, Serialize};

use crate::discover::{Date, DiscoverQuery, DiscoverSort};
use crate::genres::{company, genre, keyword};
use crate::home::{CatalogPage, ListRequest, fetch_list};
use crate::Error;

/// A themed shelf on a section hub.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionRow {
    RecentlyWatched,
    NowPlaying,
    Popular,
    Upcoming,
    Recommended,
    ThisWeek,
    Airing,
    Series,
    Films,
    New,
    LastYear,
    WorthRewatch,
    HighRated,
    Family,
    Classics,
    Golden2000s,
    Comedy2000s,
    ModernComedy,
    Genre(u32),
    Keyword(u32),
    Studio(u32),
}

impl SectionRow {
    fn key(self) -> String {
        let fixed = match self {
            Self::Genre(id) => return format!("genre_{id}"),
            Self::Keyword(id) => return format!("keyword_{id}"),
            Self::Studio(id) => return format!("studio_{id}"),
            Self::RecentlyWatched => "recently_watched",
            Self::NowPlaying => "now_playing",
            Self::Popular => "popular",
            Self::Upcoming => "upcoming",
            Self::Recommended => "recommended",
            Self::ThisWeek => "this_week",
            Self::Airing => "airing",
            Self::Series => "series",
            Self::Films => "films",
            Self::New => "new",
            Self::LastYear => "last_year",
            Self::WorthRewatch => "worth_rewatch",
            Self::HighRated => "high_rated",
            Self::Family => "family",
            Self::Classics => "classics",
            Self::Golden2000s => "golden_2000s",
            Self::Comedy2000s => "comedy_2000s",
            Self::ModernComedy => "modern_comedy",
        };

        fixed.to_owned()
    }

    /// Kind of titles this row lists inside `section`.
    #[must_use]
    pub const fn kind(self, section: Section) -> MediaKind {
        match self {
            Self::NowPlaying | Self::Upcoming | Self::Films | Self::Studio(_) => MediaKind::Movie,
            Self::ThisWeek | Self::Series | Self::Airing => MediaKind::Tv,
            _ => section.primary_kind(),
        }
    }
}

const MOVIES: &[SectionRow] = &[
    SectionRow::RecentlyWatched,
    SectionRow::NowPlaying,
    SectionRow::Popular,
    SectionRow::Upcoming,
    SectionRow::LastYear,
    SectionRow::WorthRewatch,
    SectionRow::HighRated,
    SectionRow::Genre(genre::ROMANCE),
    SectionRow::Genre(genre::ACTION),
    SectionRow::Genre(genre::THRILLER),
    SectionRow::Genre(genre::ADVENTURE),
    SectionRow::Genre(genre::COMEDY),
    SectionRow::Genre(genre::HORROR),
    SectionRow::Genre(genre::SCI_FI),
    SectionRow::Keyword(keyword::SUPERHERO),
    SectionRow::Keyword(keyword::TIME_TRAVEL),
    SectionRow::Golden2000s,
];

const TV: &[SectionRow] = &[
    SectionRow::RecentlyWatched,
    SectionRow::Recommended,
    SectionRow::Popular,
    SectionRow::ThisWeek,
    SectionRow::LastYear,
    SectionRow::WorthRewatch,
    SectionRow::HighRated,
    SectionRow::Comedy2000s,
    SectionRow::ModernComedy,
    SectionRow::Genre(genre::ACTION_ADVENTURE),
    SectionRow::Genre(genre::CRIME),
    SectionRow::Genre(genre::SCI_FI_FANTASY),
    SectionRow::Genre(genre::MYSTERY),
    SectionRow::Keyword(keyword::VAMPIRE),
    SectionRow::Keyword(keyword::ROBOT),
];

const CARTOONS: &[SectionRow] = &[
    SectionRow::RecentlyWatched,
    SectionRow::Popular,
    SectionRow::Series,
    SectionRow::New,
    SectionRow::Family,
    SectionRow::HighRated,
    SectionRow::Studio(company::PIXAR),
    SectionRow::Studio(company::DREAMWORKS_ANIMATION),
    SectionRow::Studio(company::DISNEY_ANIMATION),
    SectionRow::Genre(genre::COMEDY),
    SectionRow::Genre(genre::ADVENTURE),
    SectionRow::Classics,
];

const ANIME: &[SectionRow] = &[
    SectionRow::RecentlyWatched,
    SectionRow::Popular,
    SectionRow::Airing,
    SectionRow::Films,
    SectionRow::HighRated,
    SectionRow::LastYear,
    SectionRow::Genre(genre::ACTION_ADVENTURE),
    SectionRow::Genre(genre::COMEDY),
    SectionRow::Genre(genre::SCI_FI_FANTASY),
    SectionRow::Genre(genre::MYSTERY),
    SectionRow::Genre(genre::DRAMA),
    SectionRow::Keyword(keyword::ISEKAI),
    SectionRow::Keyword(keyword::ROMANCE),
    SectionRow::Studio(company::GHIBLI),
];

/// Shelves of a section hub, top to bottom.
#[must_use]
pub const fn section_rows(section: Section) -> &'static [SectionRow] {
    match section {
        Section::Movies => MOVIES,
        Section::Tv => TV,
        Section::Cartoons => CARTOONS,
        Section::Anime => ANIME,
    }
}

/// Any shelf the app can open as a full grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShelfId {
    Home(HomeRowId),
    Section(Section, SectionRow),
}

impl ShelfId {
    /// Stable cache key. Home shelves keep their pre-section keys.
    #[must_use]
    pub fn as_key(self) -> String {
        match self {
            Self::Home(id) => id.as_key().to_owned(),
            Self::Section(section, row) => format!("{}:{}", section.as_key(), row.key()),
        }
    }

    /// Whether the shelf pages through TMDB (local rows are one read).
    #[must_use]
    pub const fn is_remote(self) -> bool {
        match self {
            Self::Home(id) => id.is_remote(),
            Self::Section(_, row) => !matches!(row, SectionRow::RecentlyWatched),
        }
    }

    /// Request that fills this shelf, relative to `today`.
    #[must_use]
    pub fn source(self, today: Date) -> ShelfSource {
        match self {
            Self::Home(id) => home_source(id),
            Self::Section(section, row) => section_source(section, row, today),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ShelfSource {
    Local,
    List {
        path: &'static str,
        kind: Option<MediaKind>,
    },
    Discover(Box<DiscoverQuery>),
}

fn home_source(id: HomeRowId) -> ShelfSource {
    let (path, kind) = match id {
        HomeRowId::RecentlyWatched | HomeRowId::Watching | HomeRowId::Planned => {
            return ShelfSource::Local;
        }
        HomeRowId::NowPlaying => ("movie/now_playing", Some(MediaKind::Movie)),
        HomeRowId::TrendingDay => ("trending/all/day", None),
        HomeRowId::TrendingWeek => ("trending/all/week", None),
        HomeRowId::PopularMovies => ("movie/popular", Some(MediaKind::Movie)),
        HomeRowId::PopularTv => ("tv/popular", Some(MediaKind::Tv)),
        HomeRowId::TopRatedMovies => ("movie/top_rated", Some(MediaKind::Movie)),
        HomeRowId::TopRatedTv => ("tv/top_rated", Some(MediaKind::Tv)),
    };

    ShelfSource::List { path, kind }
}

fn section_source(section: Section, row: SectionRow, today: Date) -> ShelfSource {
    let kind = row.kind(section);
    let list = |path| ShelfSource::List {
        path,
        kind: Some(kind),
    };

    match (section, row) {
        (_, SectionRow::RecentlyWatched) => return ShelfSource::Local,
        (_, SectionRow::NowPlaying) => return list("movie/now_playing"),
        (_, SectionRow::Upcoming) => return list("movie/upcoming"),
        (_, SectionRow::ThisWeek) => return list("trending/tv/week"),
        (Section::Movies, SectionRow::Popular) => return list("trending/movie/week"),
        (Section::Tv, SectionRow::Popular) => return list("tv/popular"),
        _ => {}
    }

    let year = today.year;
    let mut query = DiscoverQuery::for_section(section, kind);
    match row {
        SectionRow::Recommended => {
            query.released_from = Some(Date::jan1(year - 3));
            query.vote_min = Some(7.5);
            query.vote_count_min = Some(200);
        }
        SectionRow::Airing => {
            query.airing_from = Some(today.plus_days(-7));
            query.airing_to = Some(today.plus_days(7));
        }
        SectionRow::New => {
            query.released_from = Some(today.plus_days(-365));
            query.released_to = Some(today);
        }
        SectionRow::LastYear => {
            query.released_from = Some(Date::jan1(year - 1));
            query.released_to = Some(Date::dec31(year - 1));
        }
        SectionRow::WorthRewatch => {
            query.released_from = Some(Date::jan1(year - 7));
            query.released_to = Some(Date::dec31(year - 2));
        }
        SectionRow::HighRated => {
            query.sort = DiscoverSort::Rating;
            query.vote_count_min = Some(if kind == MediaKind::Movie { 1000 } else { 200 });
        }
        SectionRow::Family => query.genres.push(genre::FAMILY),
        SectionRow::Classics => {
            query.released_to = Some(Date::dec31(1999));
            query.vote_count_min = Some(500);
        }
        SectionRow::Golden2000s => {
            query.released_from = Some(Date::jan1(2000));
            query.released_to = Some(Date::dec31(2009));
            query.vote_count_min = Some(1000);
        }
        SectionRow::Comedy2000s => {
            query.genres.push(genre::COMEDY);
            query.released_from = Some(Date::jan1(2000));
            query.released_to = Some(Date::dec31(2009));
        }
        SectionRow::ModernComedy => {
            query.genres.push(genre::COMEDY);
            query.released_from = Some(Date::jan1(year - 5));
        }
        SectionRow::Genre(id) => query.genres.push(id),
        SectionRow::Keyword(id) => query.keywords.push(id),
        SectionRow::Studio(id) => query.companies.push(id),
        SectionRow::RecentlyWatched
        | SectionRow::NowPlaying
        | SectionRow::Upcoming
        | SectionRow::ThisWeek
        | SectionRow::Popular
        | SectionRow::Series
        | SectionRow::Films => {}
    }

    ShelfSource::Discover(Box::new(query))
}

/// One loaded hub shelf: items and/or a shelf-level error.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShelfRow {
    pub id: ShelfId,
    pub items: Vec<CatalogItem>,
    pub error: Option<String>,
}

impl ShelfRow {
    #[must_use]
    pub fn empty(id: ShelfId) -> Self {
        Self {
            id,
            items: Vec::new(),
            error: None,
        }
    }
}

pub(crate) async fn fetch_source_page(
    net: &NetConfig,
    api_key: &str,
    language: Option<&str>,
    source: &ShelfSource,
    page: u32,
    today: Date,
) -> Result<CatalogPage, Error> {
    match source {
        ShelfSource::Local => Ok(CatalogPage {
            items: Vec::new(),
            page: page.max(1),
            total_pages: 1,
        }),
        ShelfSource::List { path, kind } => {
            let request = ListRequest {
                path,
                params: &[],
                kind: *kind,
                exclude_language: None,
            };
            fetch_list(net, api_key, language, &request, page).await
        }
        ShelfSource::Discover(query) => {
            let params = query.params(today);
            let request = ListRequest {
                path: query.path(),
                params: &params,
                kind: Some(query.kind),
                exclude_language: query.exclude_original_language,
            };
            fetch_list(net, api_key, language, &request, page).await
        }
    }
}

pub(crate) async fn fetch_section(
    net: &NetConfig,
    api_key: &str,
    language: Option<&str>,
    section: Section,
) -> Vec<ShelfRow> {
    let today = Date::today();
    let futs = section_rows(section).iter().map(|row| async move {
        let id = ShelfId::Section(section, *row);
        let source = id.source(today);
        match fetch_source_page(net, api_key, language, &source, 1, today).await {
            Ok(page) => ShelfRow {
                id,
                items: page.items,
                error: None,
            },
            Err(error) => ShelfRow {
                id,
                items: Vec::new(),
                error: Some(error.to_string()),
            },
        }
    });

    join_all(futs).await
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    const TODAY: Date = Date::new(2026, 9, 26);

    #[test]
    fn every_shelf_key_is_unique() {
        let mut keys = HashSet::new();
        let home = HomeRowId::ALL.into_iter().map(ShelfId::Home);
        let sections = Section::ALL.into_iter().flat_map(|section| {
            section_rows(section)
                .iter()
                .map(move |row| ShelfId::Section(section, *row))
        });

        for id in home.chain(sections) {
            assert!(keys.insert(id.as_key()), "duplicate key {}", id.as_key());
        }
    }

    #[test]
    fn home_keys_match_legacy_cache_keys() {
        assert_eq!(ShelfId::Home(HomeRowId::NowPlaying).as_key(), "now_playing");
    }

    #[test]
    fn section_recently_watched_is_local() {
        let id = ShelfId::Section(Section::Anime, SectionRow::RecentlyWatched);

        assert_eq!(id.source(TODAY), ShelfSource::Local);
    }

    #[test]
    fn last_year_spans_previous_calendar_year() {
        let id = ShelfId::Section(Section::Movies, SectionRow::LastYear);
        let ShelfSource::Discover(query) = id.source(TODAY) else {
            panic!("expected discover source");
        };

        assert_eq!(
            (query.released_from, query.released_to),
            (Some(Date::jan1(2025)), Some(Date::dec31(2025)))
        );
    }

    #[test]
    fn studio_rows_list_films_in_series_sections() {
        let row = SectionRow::Studio(company::GHIBLI);

        assert_eq!(row.kind(Section::Anime), MediaKind::Movie);
    }

    #[test]
    fn anime_genre_rows_stay_japanese() {
        let id = ShelfId::Section(Section::Anime, SectionRow::Genre(genre::COMEDY));
        let ShelfSource::Discover(query) = id.source(TODAY) else {
            panic!("expected discover source");
        };

        assert_eq!(query.original_language, Some("ja"));
    }
}
