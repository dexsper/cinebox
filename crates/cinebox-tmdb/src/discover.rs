//! `discover/{movie,tv}` queries: section scoping, filters, and query parameters.

use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use cinebox_core::{MediaKind, Section};

use crate::genres::{genre, keyword};

/// Calendar date (UTC) for TMDB date filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

impl Date {
    #[must_use]
    pub const fn new(year: i32, month: u8, day: u8) -> Self {
        Self { year, month, day }
    }

    #[must_use]
    pub const fn jan1(year: i32) -> Self {
        Self::new(year, 1, 1)
    }

    #[must_use]
    pub const fn dec31(year: i32) -> Self {
        Self::new(year, 12, 31)
    }

    /// Today in UTC. Falls back to the Unix epoch if the system clock is before it.
    #[must_use]
    pub fn today() -> Self {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs());

        let days = i64::try_from(secs / 86_400).unwrap_or(0);
        Self::from_days(days)
    }

    #[must_use]
    pub fn plus_days(self, days: i64) -> Self {
        Self::from_days(self.to_days() + days)
    }

    // Howard Hinnant's civil-from-days: https://howardhinnant.github.io/date_algorithms.html
    fn from_days(days: i64) -> Self {
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = yoe + era * 400 + i64::from(month <= 2);

        Self::new(
            i32::try_from(year).unwrap_or(i32::MAX),
            u8::try_from(month).unwrap_or(1),
            u8::try_from(day).unwrap_or(1),
        )
    }

    fn to_days(self) -> i64 {
        let month = i64::from(self.month);
        let year = i64::from(self.year) - i64::from(month <= 2);
        let era = year.div_euclid(400);
        let yoe = year.rem_euclid(400);
        let mp = if month > 2 { month - 3 } else { month + 9 };
        let doy = (153 * mp + 2) / 5 + i64::from(self.day) - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;

        era * 146_097 + doe - 719_468
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

/// `sort_by` choices exposed to the user.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum DiscoverSort {
    #[default]
    Popular,
    Rating,
    Newest,
    Revenue,
}

impl DiscoverSort {
    pub const ALL: [Self; 4] = [Self::Popular, Self::Rating, Self::Newest, Self::Revenue];

    /// Sorts that make sense for `kind`; TMDB has no revenue for TV.
    #[must_use]
    pub fn available(kind: MediaKind) -> &'static [Self] {
        match kind {
            MediaKind::Tv => &Self::ALL[..3],
            MediaKind::Movie | MediaKind::Person => &Self::ALL,
        }
    }
}

/// Minimum votes applied to rating sorts so single-vote titles do not top the list.
const RATING_SORT_MIN_VOTES: u32 = 300;

/// One `discover/{kind}` request, minus paging and language.
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoverQuery {
    pub kind: MediaKind,
    /// All must match.
    pub genres: Vec<u32>,
    pub without_genres: Vec<u32>,
    /// Any may match.
    pub keywords: Vec<u32>,
    pub without_keywords: Vec<u32>,
    /// Any may match.
    pub companies: Vec<u32>,
    pub original_language: Option<&'static str>,
    /// Dropped client-side: TMDB discover has no `without_original_language`.
    pub exclude_original_language: Option<&'static str>,
    pub released_from: Option<Date>,
    pub released_to: Option<Date>,
    pub airing_from: Option<Date>,
    pub airing_to: Option<Date>,
    pub vote_min: Option<f32>,
    pub vote_count_min: Option<u32>,
    pub sort: DiscoverSort,
}

impl DiscoverQuery {
    /// Unfiltered query for `kind`, most popular first.
    #[must_use]
    pub fn new(kind: MediaKind) -> Self {
        Self {
            kind,
            genres: Vec::new(),
            without_genres: Vec::new(),
            keywords: Vec::new(),
            without_keywords: Vec::new(),
            companies: Vec::new(),
            original_language: None,
            exclude_original_language: None,
            released_from: None,
            released_to: None,
            airing_from: None,
            airing_to: None,
            vote_min: None,
            vote_count_min: None,
            sort: DiscoverSort::Popular,
        }
    }

    /// Query limited to what `section` shows for `kind`.
    #[must_use]
    pub fn for_section(section: Section, kind: MediaKind) -> Self {
        let mut query = Self::new(kind);
        match section {
            Section::Movies | Section::Tv => query.without_genres.push(genre::ANIMATION),
            Section::Cartoons => {
                query.genres.push(genre::ANIMATION);
                query.without_keywords.push(keyword::ANIME);
                query.exclude_original_language = Some("ja");
            }
            Section::Anime => {
                query.genres.push(genre::ANIMATION);
                query.original_language = Some("ja");
            }
        }

        query
    }

    pub(crate) fn path(&self) -> &'static str {
        match self.kind {
            MediaKind::Tv => "discover/tv",
            MediaKind::Movie | MediaKind::Person => "discover/movie",
        }
    }

    pub(crate) fn params(&self, today: Date) -> Vec<(&'static str, String)> {
        let tv = self.kind == MediaKind::Tv;
        let mut out = vec![
            ("include_adult", String::from("false")),
            ("sort_by", String::from(self.sort_key())),
        ];

        push_ids(&mut out, "with_genres", &self.genres, ",");
        push_ids(&mut out, "without_genres", &self.without_genres, ",");
        push_ids(&mut out, "with_keywords", &self.keywords, "|");
        push_ids(&mut out, "without_keywords", &self.without_keywords, ",");
        push_ids(&mut out, "with_companies", &self.companies, "|");

        if let Some(language) = self.original_language {
            out.push(("with_original_language", language.to_owned()));
        }

        let released_to = match self.sort {
            DiscoverSort::Newest => self.released_to.or(Some(today)),
            _ => self.released_to,
        };

        let (from_key, to_key) = if tv {
            ("first_air_date.gte", "first_air_date.lte")
        } else {
            ("primary_release_date.gte", "primary_release_date.lte")
        };

        push_date(&mut out, from_key, self.released_from);
        push_date(&mut out, to_key, released_to);
        push_date(&mut out, "air_date.gte", self.airing_from);
        push_date(&mut out, "air_date.lte", self.airing_to);

        if let Some(vote) = self.vote_min {
            out.push(("vote_average.gte", format!("{vote:.1}")));
        }

        let vote_count = match self.sort {
            DiscoverSort::Rating => self.vote_count_min.or(Some(RATING_SORT_MIN_VOTES)),
            _ => self.vote_count_min,
        };

        if let Some(count) = vote_count {
            out.push(("vote_count.gte", count.to_string()));
        }

        out
    }

    fn sort_key(&self) -> &'static str {
        let tv = self.kind == MediaKind::Tv;
        match self.sort {
            DiscoverSort::Popular => "popularity.desc",
            DiscoverSort::Rating => "vote_average.desc",
            DiscoverSort::Newest if tv => "first_air_date.desc",
            DiscoverSort::Newest => "primary_release_date.desc",
            DiscoverSort::Revenue if tv => "popularity.desc",
            DiscoverSort::Revenue => "revenue.desc",
        }
    }
}

fn push_ids(out: &mut Vec<(&'static str, String)>, key: &'static str, ids: &[u32], sep: &str) {
    if ids.is_empty() {
        return;
    }

    let joined = ids.iter().map(u32::to_string).collect::<Vec<_>>().join(sep);

    out.push((key, joined));
}

fn push_date(out: &mut Vec<(&'static str, String)>, key: &'static str, date: Option<Date>) {
    if let Some(date) = date {
        out.push((key, date.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TODAY: Date = Date::new(2026, 9, 26);

    fn param<'a>(params: &'a [(&'static str, String)], key: &str) -> Option<&'a str> {
        params
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value.as_str())
    }

    mod date {
        use super::*;

        #[test]
        fn formats_as_iso() {
            assert_eq!(Date::new(2021, 3, 7).to_string(), "2021-03-07");
        }

        #[test]
        fn plus_days_crosses_year_boundary() {
            assert_eq!(Date::dec31(2025).plus_days(1), Date::jan1(2026));
        }

        #[test]
        fn minus_days_handles_leap_february() {
            assert_eq!(Date::new(2024, 3, 1).plus_days(-1), Date::new(2024, 2, 29));
        }
    }

    mod for_section {
        use super::*;

        #[test]
        fn anime_requires_japanese_animation() {
            let params = DiscoverQuery::for_section(Section::Anime, MediaKind::Tv).params(TODAY);

            assert_eq!(param(&params, "with_original_language"), Some("ja"));
        }

        #[test]
        fn cartoons_exclude_anime_keyword() {
            let params =
                DiscoverQuery::for_section(Section::Cartoons, MediaKind::Movie).params(TODAY);

            assert_eq!(param(&params, "without_keywords"), Some("210024"));
        }

        #[test]
        fn movies_exclude_animation() {
            let params =
                DiscoverQuery::for_section(Section::Movies, MediaKind::Movie).params(TODAY);

            assert_eq!(param(&params, "without_genres"), Some("16"));
        }
    }

    mod params {
        use super::*;

        #[test]
        fn tv_dates_use_first_air_date() {
            let mut query = DiscoverQuery::new(MediaKind::Tv);
            query.released_from = Some(Date::jan1(2020));

            assert_eq!(
                param(&query.params(TODAY), "first_air_date.gte"),
                Some("2020-01-01")
            );
        }

        #[test]
        fn genres_are_joined_as_all_of() {
            let mut query = DiscoverQuery::new(MediaKind::Movie);
            query.genres = vec![genre::ANIMATION, genre::FAMILY];

            assert_eq!(param(&query.params(TODAY), "with_genres"), Some("16,10751"));
        }

        #[test]
        fn keywords_are_joined_as_any_of() {
            let mut query = DiscoverQuery::new(MediaKind::Tv);
            query.keywords = vec![keyword::VAMPIRE, keyword::ROBOT];

            assert_eq!(
                param(&query.params(TODAY), "with_keywords"),
                Some("3133|14544")
            );
        }

        #[test]
        fn rating_sort_adds_minimum_votes() {
            let mut query = DiscoverQuery::new(MediaKind::Movie);
            query.sort = DiscoverSort::Rating;

            assert_eq!(param(&query.params(TODAY), "vote_count.gte"), Some("300"));
        }

        #[test]
        fn newest_sort_hides_unreleased_titles() {
            let mut query = DiscoverQuery::new(MediaKind::Movie);
            query.sort = DiscoverSort::Newest;

            assert_eq!(
                param(&query.params(TODAY), "primary_release_date.lte"),
                Some("2026-09-26")
            );
        }

        #[test]
        fn revenue_sort_falls_back_to_popularity_for_tv() {
            let mut query = DiscoverQuery::new(MediaKind::Tv);
            query.sort = DiscoverSort::Revenue;

            assert_eq!(
                param(&query.params(TODAY), "sort_by"),
                Some("popularity.desc")
            );
        }
    }
}
