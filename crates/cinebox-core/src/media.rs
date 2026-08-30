//! Full media card and person page payloads.

use crate::catalog::{CatalogItem, normalize_tmdb_path};
use crate::ids::{MediaKind, TmdbId};
use serde::{Deserialize, Serialize};

/// YouTube (or other) trailer/teaser from TMDB `videos`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trailer {
    pub name: String,
    pub youtube_key: String,
}

impl Trailer {
    /// Watch URL. Only YouTube keys are stored.
    #[must_use]
    pub fn watch_url(&self) -> String {
        format!("https://www.youtube.com/watch?v={}", self.youtube_key)
    }
}

/// Cast or crew member shown on a media card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreditPerson {
    pub id: TmdbId,
    pub name: String,
    pub role: String,
    pub profile_path: Option<String>,
}

/// Full movie/TV card (Phase 4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaDetails {
    pub id: TmdbId,
    pub kind: MediaKind,
    pub title: String,
    pub original_title: Option<String>,
    pub original_language: Option<String>,
    pub tagline: Option<String>,
    pub overview: Option<String>,
    pub year: Option<u16>,
    pub released: Option<String>,
    pub runtime_minutes: Option<u32>,
    pub number_of_seasons: Option<u32>,
    pub number_of_episodes: Option<u32>,
    pub certification: Option<String>,
    pub vote: Option<f32>,
    pub budget: Option<u64>,
    pub genre_ids: Vec<u32>,
    pub genres: Vec<String>,
    pub countries: Vec<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub directors: Vec<CreditPerson>,
    pub cast: Vec<CreditPerson>,
    pub collection: Vec<CatalogItem>,
    pub recommendations: Vec<CatalogItem>,
    pub similar: Vec<CatalogItem>,
    pub trailers: Vec<Trailer>,
}

impl MediaDetails {
    /// Year and production countries shown above the title.
    #[must_use]
    pub fn head_line(&self) -> String {
        let mut parts = Vec::new();
        if let Some(year) = self.year {
            parts.push(year.to_string());
        }

        if !self.countries.is_empty() {
            parts.push(
                self.countries
                    .iter()
                    .take(5)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" | "),
            );
        }

        parts.join(", ")
    }

    /// Runtime (movies) or season/episode counts (TV), then genres.
    #[must_use]
    pub fn detail_bits(&self) -> Vec<String> {
        let mut bits = Vec::new();
        match self.kind {
            MediaKind::Tv => {
                if let Some(seasons) = self.number_of_seasons.filter(|n| *n > 0) {
                    bits.push(format_seasons(seasons));
                }
                if let Some(episodes) = self.number_of_episodes.filter(|n| *n > 0) {
                    bits.push(format_episodes(episodes));
                }
            }
            MediaKind::Movie | MediaKind::Person => {
                if let Some(mins) = self.runtime_minutes.filter(|m| *m > 0) {
                    bits.push(format_runtime(mins));
                }
            }
        }
        bits.extend(self.genres.iter().take(5).cloned());
        bits
    }

    /// Jackett/Prowlarr anime category (TMDB animation + ja/zh).
    #[must_use]
    pub fn is_anime(&self) -> bool {
        const ANIMATION: u32 = 16;

        if !matches!(self.original_language.as_deref(), Some("ja" | "zh")) {
            return false;
        }

        self.genre_ids.contains(&ANIMATION)
    }

    /// Default indexer query: original title (year is a separate Jackett param).
    #[must_use]
    pub fn torrent_query(&self) -> String {
        self.original_title
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(&self.title)
            .to_owned()
    }

    /// Poster / backdrop / credit / related paths for the image cache.
    #[must_use]
    pub fn image_paths(&self) -> Vec<String> {
        let mut paths = Vec::new();
        push_path(&mut paths, self.poster_path.as_deref());
        push_path(&mut paths, self.backdrop_path.as_deref());

        let people = self.directors.iter().chain(self.cast.iter());
        for person in people {
            push_path(&mut paths, person.profile_path.as_deref());
        }

        let items = self
            .collection
            .iter()
            .chain(&self.recommendations)
            .chain(&self.similar);

        for item in items {
            push_path(&mut paths, item.poster_path.as_deref());
        }

        paths
    }
}

/// Person page: bio + combined credits grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersonDetails {
    pub id: TmdbId,
    pub name: String,
    pub biography: Option<String>,
    pub birthday: Option<String>,
    pub place_of_birth: Option<String>,
    pub profile_path: Option<String>,
    pub credits: Vec<CatalogItem>,
}

impl PersonDetails {
    /// Profile and credit poster paths for the image cache.
    #[must_use]
    pub fn image_paths(&self) -> Vec<String> {
        let mut paths = Vec::new();
        push_path(&mut paths, self.profile_path.as_deref());

        for item in &self.credits {
            push_path(&mut paths, item.poster_path.as_deref());
        }

        paths
    }
}

fn push_path(paths: &mut Vec<String>, raw: Option<&str>) {
    let Some(path) = normalize_tmdb_path(raw) else {
        return;
    };

    if paths.contains(&path) {
        return;
    }

    paths.push(path);
}

/// Format `125` → `2h 5m`.
#[must_use]
pub fn format_runtime(minutes: u32) -> String {
    let hours = minutes / 60;
    let mins = minutes % 60;

    if hours == 0 {
        return format!("{mins}m");
    }

    if mins == 0 {
        return format!("{hours}h");
    }

    format!("{hours}h {mins}m")
}

#[must_use]
fn format_seasons(count: u32) -> String {
    if count == 1 {
        return String::from("1 season");
    }

    format!("{count} seasons")
}

#[must_use]
fn format_episodes(count: u32) -> String {
    if count == 1 {
        return String::from("1 episode");
    }

    format!("{count} episodes")
}

/// Map MPAA / TV parental codes to an age mark (`13+`). Other labels pass through.
#[must_use]
pub fn decode_certification(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mapped = match trimmed.to_ascii_uppercase().as_str() {
        "G" | "TV-G" => "3+",
        "PG" | "TV-PG" => "6+",
        "PG-13" => "13+",
        "TV-14" => "14+",
        "R" | "TV-MA" => "17+",
        "NC-17" => "18+",
        "TV-Y" => "0+",
        "TV-Y7" | "TV-Y7-FV" => "7+",
        _ => trimmed,
    };

    Some(mapped.to_owned())
}

/// Format a TMDB budget/revenue if it is present and positive.
#[must_use]
pub fn format_money(amount: u64) -> String {
    let digits = amount.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    out.push('$');

    let head = digits.len() % 3;
    let mut i = 0;
    if head != 0 {
        out.push_str(&digits[..head]);
        i = head;
    }

    while i < digits.len() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&digits[i..i + 3]);
        i += 3;
    }

    out
}

/// Format `YYYY-MM-DD` as `22 Oct 2021`. Other strings pass through.
#[must_use]
pub fn format_release_date(iso: &str) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];

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

    if !(1..=12).contains(&month) {
        return iso.to_owned();
    }

    let day = day.trim_start_matches('0');
    format!("{day} {} {year}", MONTHS[month - 1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_and_money() {
        assert_eq!(format_runtime(125), "2h 5m");
        assert_eq!(format_runtime(60), "1h");
        assert_eq!(format_runtime(9), "9m");
        assert_eq!(format_money(12_000_000), "$12,000,000");
        assert_eq!(format_money(100), "$100");
        assert_eq!(format_release_date("2021-10-22"), "22 Oct 2021");
        assert_eq!(format_release_date("2021"), "2021");
        assert_eq!(decode_certification("PG-13").as_deref(), Some("13+"));
        assert_eq!(decode_certification("TV-MA").as_deref(), Some("17+"));
        assert_eq!(decode_certification("16+").as_deref(), Some("16+"));
        assert_eq!(decode_certification("  ").as_deref(), None);
    }

    #[test]
    fn head_and_detail_bits() {
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
            trailers: vec![Trailer {
                name: String::from("Official"),
                youtube_key: String::from("abc"),
            }],
        };

        assert_eq!(details.head_line(), "2021, United States");
        assert_eq!(details.detail_bits(), vec!["2h 35m", "Sci-Fi", "Adventure"]);
        assert_eq!(details.torrent_query(), "Dune");
        assert!(!details.is_anime());
        let tv = MediaDetails {
            kind: MediaKind::Tv,
            runtime_minutes: Some(47),
            number_of_seasons: Some(5),
            number_of_episodes: Some(62),
            ..details.clone()
        };

        assert_eq!(
            tv.detail_bits(),
            vec!["5 seasons", "62 episodes", "Sci-Fi", "Adventure"]
        );

        assert_eq!(
            details.trailers[0].watch_url(),
            "https://www.youtube.com/watch?v=abc"
        );
    }
}
