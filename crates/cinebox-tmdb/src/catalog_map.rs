//! Shared TMDB list-item → [`CatalogItem`] mapping (home rows, recs, credits).

use cinebox_core::{CatalogItem, MediaKind, TmdbId, year_from_date};
use serde::Deserialize;

/// One row from a TMDB list endpoint (`results`, collection `parts`, combined credits).
#[derive(Debug, Default, Deserialize)]
pub(crate) struct CatalogListItem {
    pub(crate) id: Option<u32>,
    pub(crate) title: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) poster_path: Option<String>,
    pub(crate) profile_path: Option<String>,
    pub(crate) vote_average: Option<f32>,
    pub(crate) release_date: Option<String>,
    pub(crate) first_air_date: Option<String>,
    pub(crate) media_type: Option<String>,
}

pub(crate) fn catalog_items_from(
    items: impl IntoIterator<Item = CatalogListItem>,
    fallback: Option<MediaKind>,
    cap: usize,
) -> Vec<CatalogItem> {
    let mut out = Vec::new();
    let mut seen = Vec::new();
    for raw in items {
        if out.len() >= cap {
            break;
        }

        let Some(id) = raw.id.filter(|id| *id > 0) else {
            continue;
        };

        if seen.contains(&id) {
            continue;
        }

        if raw.media_type.as_deref() == Some("person") {
            continue;
        }

        let kind = raw
            .media_type
            .as_deref()
            .and_then(kind_from_media_type)
            .or(fallback);

        let Some(kind @ (MediaKind::Movie | MediaKind::Tv)) = kind else {
            continue;
        };

        let title = match kind {
            MediaKind::Movie => raw.title.or(raw.name),
            MediaKind::Tv => raw.name.or(raw.title),
            MediaKind::Person => None,
        };

        let Some(title) = title.filter(|t| !t.is_empty()) else {
            continue;
        };

        let date = match kind {
            MediaKind::Movie => raw.release_date.as_deref(),
            MediaKind::Tv => raw.first_air_date.as_deref(),
            MediaKind::Person => None,
        };

        seen.push(id);
        out.push(CatalogItem {
            id: TmdbId::new(id),
            kind,
            title,
            year: date.and_then(year_from_date),
            vote: raw.vote_average.filter(|v| *v > 0.0),
            poster_path: raw.poster_path.filter(|s| !s.trim().is_empty()),
        });
    }
    out
}

pub(crate) fn person_items_from(
    items: impl IntoIterator<Item = CatalogListItem>,
    cap: usize,
) -> Vec<CatalogItem> {
    let mut out = Vec::new();
    let mut seen = Vec::new();

    for raw in items {
        if out.len() >= cap {
            break;
        }

        let is_non_person = raw.media_type.as_deref().is_some_and(|kind| kind != "person");
        if is_non_person {
            continue;
        }

        let Some(id) = raw.id.filter(|id| *id > 0) else {
            continue;
        };

        if seen.contains(&id) {
            continue;
        }

        let Some(title) = raw.name.or(raw.title).filter(|t| !t.is_empty()) else {
            continue;
        };

        let poster_path = raw
            .profile_path
            .or(raw.poster_path)
            .filter(|path| !path.trim().is_empty());

        seen.push(id);
        out.push(CatalogItem {
            id: TmdbId::new(id),
            kind: MediaKind::Person,
            title,
            year: None,
            vote: raw.vote_average.filter(|v| *v > 0.0),
            poster_path,
        });
    }

    out
}

fn kind_from_media_type(value: &str) -> Option<MediaKind> {
    match value {
        "movie" => Some(MediaKind::Movie),
        "tv" => Some(MediaKind::Tv),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mixed_trending_and_skips_people() {
        let items = vec![
            CatalogListItem {
                id: Some(10),
                title: Some(String::from("Film")),
                poster_path: Some(String::from("/p.jpg")),
                vote_average: Some(7.5),
                release_date: Some(String::from("2023-01-02")),
                media_type: Some(String::from("movie")),
                ..Default::default()
            },
            CatalogListItem {
                id: Some(11),
                name: Some(String::from("Show")),
                vote_average: Some(8.0),
                first_air_date: Some(String::from("2022-05-01")),
                media_type: Some(String::from("tv")),
                ..Default::default()
            },
            CatalogListItem {
                id: Some(12),
                name: Some(String::from("Actor")),
                media_type: Some(String::from("person")),
                ..Default::default()
            },
        ];

        let parsed = catalog_items_from(items, None, 20);

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].kind, MediaKind::Movie);
        assert_eq!(parsed[0].year, Some(2023));
        assert_eq!(parsed[1].kind, MediaKind::Tv);
        assert_eq!(parsed[1].title, "Show");
    }

    #[test]
    fn person_search_uses_profile_path_and_skips_titles() {
        let items = vec![
            CatalogListItem {
                id: Some(7),
                name: Some(String::from("Tim")),
                profile_path: Some(String::from("/t.jpg")),
                media_type: Some(String::from("person")),
                ..Default::default()
            },
            CatalogListItem {
                id: Some(8),
                title: Some(String::from("A Film")),
                poster_path: Some(String::from("/m.jpg")),
                media_type: Some(String::from("movie")),
                ..Default::default()
            },
            CatalogListItem {
                id: Some(9),
                name: Some(String::from("Pat")),
                ..Default::default()
            },
        ];

        let parsed = person_items_from(items, 20);

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].kind, MediaKind::Person);
        assert_eq!(parsed[0].title, "Tim");
        assert_eq!(parsed[0].poster_path.as_deref(), Some("/t.jpg"));
        assert_eq!(parsed[1].title, "Pat");
    }
}
