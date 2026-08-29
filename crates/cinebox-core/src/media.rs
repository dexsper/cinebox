//! Full media card and person page payloads.

use crate::catalog::CatalogItem;
use crate::ids::{MediaKind, TmdbId};

/// YouTube (or other) trailer/teaser from TMDB `videos`.
#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreditPerson {
    pub id: TmdbId,
    pub name: String,
    pub role: String,
    pub profile_path: Option<String>,
}

/// Full movie/TV card (Phase 4).
#[derive(Debug, Clone, PartialEq)]
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

    /// Runtime and genres under the rating row.
    #[must_use]
    pub fn detail_bits(&self) -> Vec<String> {
        let mut bits = Vec::new();
        if let Some(mins) = self.runtime_minutes.filter(|m| *m > 0) {
            bits.push(format_runtime(mins));
        }
        bits.extend(self.genres.iter().take(5).cloned());
        bits
    }

    /// Jackett/Prowlarr anime category (TMDB animation + ja/zh).
    #[must_use]
    pub fn is_anime(&self) -> bool {
        const ANIMATION: u32 = 16;
        matches!(self.original_language.as_deref(), Some("ja" | "zh"))
            && self.genre_ids.contains(&ANIMATION)
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
}

/// Person page: bio + combined credits grid.
#[derive(Debug, Clone, PartialEq)]
pub struct PersonDetails {
    pub id: TmdbId,
    pub name: String,
    pub biography: Option<String>,
    pub birthday: Option<String>,
    pub place_of_birth: Option<String>,
    pub profile_path: Option<String>,
    pub credits: Vec<CatalogItem>,
}

/// Format `125` → `2h 5m`.
#[must_use]
pub fn format_runtime(minutes: u32) -> String {
    let hours = minutes / 60;
    let mins = minutes % 60;
    if hours == 0 {
        format!("{mins}m")
    } else if mins == 0 {
        format!("{hours}h")
    } else {
        format!("{hours}h {mins}m")
    }
}

/// Format a TMDB budget/revenue if it is present and positive.
#[must_use]
pub fn format_money(amount: u64) -> String {
    let mut s = amount.to_string();
    let mut out = String::new();
    while !s.is_empty() {
        let take = if s.len().is_multiple_of(3) {
            3
        } else {
            s.len() % 3
        };
        if !out.is_empty() {
            out.push(',');
        }
        out.push_str(&s[..take]);
        s = s[take..].to_owned();
    }
    format!("${out}")
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
        assert_eq!(
            details.trailers[0].watch_url(),
            "https://www.youtube.com/watch?v=abc"
        );
    }
}
