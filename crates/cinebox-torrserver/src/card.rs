//! The title card a torrent carries in its free-form `data` field.
//!
//! TorrServer stores `data` verbatim, and its clients agree on one shape, the one Lampa writes: a JSON
//! string `{"lampa":true,"movie":{...}}`, where `movie` is a trimmed TMDB card. The marker key is written
//! only so that those clients recognise the card; reading does not require it. TorrServer itself fills an
//! empty `data` with `{"TorrServer":{"Files":[...]}}` when it saves a torrent, and that list is kept.

use cinebox_core::{MediaDetails, MediaKind, TmdbId, tmdb_image_url};
use serde::Serialize;
use serde_json::{Map, Value};

use super::status::FileStat;

/// The key other clients look for before they read `movie`.
const MARKER_KEY: &str = "lampa";

/// A trimmed TMDB card in the shape other TorrServer clients read. Empty fields are not written.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct TorrCard {
    pub id: u32,

    /// Where `id` comes from: `tmdb` (or empty) and `cub` are TMDB ids, anything else is not.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub source: String,

    #[serde(skip_serializing_if = "String::is_empty")]
    pub title: String,

    #[serde(skip_serializing_if = "String::is_empty")]
    pub original_title: String,

    /// Set for a show only: clients tell a show from a movie by it.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,

    #[serde(skip_serializing_if = "String::is_empty")]
    pub original_name: String,

    #[serde(skip_serializing_if = "String::is_empty")]
    pub overview: String,

    #[serde(skip_serializing_if = "String::is_empty")]
    pub poster_path: String,

    #[serde(skip_serializing_if = "String::is_empty")]
    pub backdrop_path: String,

    #[serde(skip_serializing_if = "String::is_empty")]
    pub release_date: String,

    #[serde(skip_serializing_if = "String::is_empty")]
    pub first_air_date: String,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub genre_ids: Vec<u32>,

    #[serde(skip_serializing_if = "String::is_empty")]
    pub original_language: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub vote_average: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub number_of_seasons: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub number_of_episodes: Option<u32>,

    #[serde(skip_serializing_if = "String::is_empty")]
    pub status: String,

    /// Poster URL at `w300`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub img: String,

    /// Poster URL at the size the torrent's own `poster` uses.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub poster: String,
}

impl TorrCard {
    /// The card for a title Cinebox adds a torrent of.
    #[must_use]
    pub fn from_details(details: &MediaDetails, poster_url: &str) -> Self {
        let original = details.original_title.clone().unwrap_or_default();
        let released = details.released.clone().unwrap_or_default();
        let img = tmdb_image_url(details.poster_path.as_deref(), "w300").unwrap_or_default();
        let card = Self {
            id: details.id.get(),
            source: String::from("tmdb"),
            title: details.title.clone(),
            original_title: original.clone(),
            overview: details.overview.clone().unwrap_or_default(),
            poster_path: details.poster_path.clone().unwrap_or_default(),
            backdrop_path: details.backdrop_path.clone().unwrap_or_default(),
            release_date: released.clone(),
            genre_ids: details.genre_ids.clone(),
            original_language: details.original_language.clone().unwrap_or_default(),
            vote_average: details.vote.map(f64::from),
            status: details.status.clone().unwrap_or_default(),
            img,
            poster: poster_url.to_owned(),
            ..Self::default()
        };

        if details.kind != MediaKind::Tv {
            return card;
        }

        Self {
            name: details.title.clone(),
            original_name: original,
            first_air_date: released,
            number_of_seasons: details.number_of_seasons,
            number_of_episodes: details.number_of_episodes,
            ..card
        }
    }

    /// The TMDB title this card names, or `None` when its id comes from another catalog.
    #[must_use]
    pub fn tmdb_ref(&self) -> Option<(MediaKind, TmdbId)> {
        if !matches!(self.source.as_str(), "" | "tmdb" | "cub") {
            return None;
        }

        let is_show = !self.name.is_empty() || !self.original_name.is_empty() || self.number_of_seasons.is_some();
        let kind = if is_show { MediaKind::Tv } else { MediaKind::Movie };

        Some((kind, TmdbId::new(self.id)))
    }

    /// `tv` or `movie`, the torrent `category` clients expect next to this card.
    #[must_use]
    pub fn category(&self) -> &'static str {
        if self.name.is_empty() { "movie" } else { "tv" }
    }

    /// Reads a `movie` object written by any client: unknown fields are ignored and a field of an
    /// unexpected type reads as empty. `None` without a positive `id`.
    fn from_value(movie: &Value) -> Option<Self> {
        let id = number(movie, "id").and_then(|id| u32::try_from(id).ok()).filter(|id| *id > 0)?;
        let genres = movie.get("genre_ids").and_then(Value::as_array);
        let genre_ids = genres.into_iter().flatten().filter_map(as_u32).collect();

        Some(Self {
            id,
            source: text(movie, "source"),
            title: text(movie, "title"),
            original_title: text(movie, "original_title"),
            name: text(movie, "name"),
            original_name: text(movie, "original_name"),
            overview: text(movie, "overview"),
            poster_path: text(movie, "poster_path"),
            backdrop_path: text(movie, "backdrop_path"),
            release_date: text(movie, "release_date"),
            first_air_date: text(movie, "first_air_date"),
            genre_ids,
            original_language: text(movie, "original_language"),
            vote_average: movie.get("vote_average").and_then(Value::as_f64),
            number_of_seasons: movie.get("number_of_seasons").and_then(as_u32),
            number_of_episodes: movie.get("number_of_episodes").and_then(as_u32),
            status: text(movie, "status"),
            img: text(movie, "img"),
            poster: text(movie, "poster"),
        })
    }
}

/// `data` holding only `card`.
#[must_use]
pub fn encode_data(card: &TorrCard) -> String {
    with_card("", card)
}

/// `existing` with `card` put in, keeping whatever else it holds (such as TorrServer's own file list).
/// Data that is not a JSON object is replaced.
#[must_use]
pub fn with_card(existing: &str, card: &TorrCard) -> String {
    let parsed = serde_json::from_str::<Value>(existing).ok();
    let mut root = match parsed {
        Some(Value::Object(root)) => root,
        _ => Map::new(),
    };

    root.insert(String::from(MARKER_KEY), Value::Bool(true));
    root.insert(String::from("movie"), serde_json::to_value(card).unwrap_or_default());

    Value::Object(root).to_string()
}

/// The card in a torrent's `data`, or `None` when it holds none: empty, not JSON, another format,
/// or a `movie` without an id.
#[must_use]
pub fn decode_data(data: &str) -> Option<TorrCard> {
    let root: Value = serde_json::from_str(data).ok()?;

    TorrCard::from_value(root.get("movie")?)
}

/// The file list TorrServer keeps in `data` for a saved torrent it has no card for. Empty when absent.
#[must_use]
pub fn stored_files(data: &str) -> Vec<FileStat> {
    let Ok(root) = serde_json::from_str::<Value>(data) else { return Vec::new() };
    let Some(files) = root.pointer("/TorrServer/Files").cloned() else { return Vec::new() };

    serde_json::from_value(files).unwrap_or_default()
}

fn text(object: &Value, key: &str) -> String {
    match object.get(key) {
        Some(Value::String(value)) => value.trim().to_owned(),
        Some(Value::Number(value)) => value.to_string(),
        Some(Value::Null | Value::Bool(_) | Value::Array(_) | Value::Object(_)) | None => String::new(),
    }
}

fn number(object: &Value, key: &str) -> Option<u64> {
    match object.get(key)? {
        Value::Number(value) => value.as_u64(),
        Value::String(value) => value.trim().parse().ok(),
        Value::Null | Value::Bool(_) | Value::Array(_) | Value::Object(_) => None,
    }
}

fn as_u32(value: &Value) -> Option<u32> {
    value.as_u64().and_then(|value| u32::try_from(value).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn show_details() -> MediaDetails {
        MediaDetails {
            id: TmdbId::new(1399),
            kind: MediaKind::Tv,
            title: String::from("Игра престолов"),
            original_title: Some(String::from("Game of Thrones")),
            original_language: Some(String::from("en")),
            tagline: None,
            overview: Some(String::from("Семь благородных семей")),
            year: Some(2011),
            released: Some(String::from("2011-04-17")),
            status: Some(String::from("Ended")),
            runtime_minutes: None,
            number_of_seasons: Some(8),
            number_of_episodes: Some(73),
            certification: None,
            vote: Some(8.5),
            budget: None,
            genre_ids: vec![18, 10765],
            genres: Vec::new(),
            countries: Vec::new(),
            poster_path: Some(String::from("/poster.jpg")),
            backdrop_path: Some(String::from("/backdrop.jpg")),
            directors: Vec::new(),
            cast: Vec::new(),
            collection: Vec::new(),
            recommendations: Vec::new(),
            similar: Vec::new(),
            trailers: Vec::new(),
        }
    }

    #[test]
    fn a_show_card_survives_a_round_trip() {
        let card = TorrCard::from_details(&show_details(), "https://image.tmdb.org/t/p/w500/poster.jpg");

        let decoded = decode_data(&encode_data(&card));

        assert_eq!(decoded, Some(card));
    }

    #[test]
    fn a_show_card_names_the_show() {
        let card = TorrCard::from_details(&show_details(), "");

        assert_eq!(card.name, "Игра престолов");
        assert_eq!(card.first_air_date, "2011-04-17");
        assert_eq!(card.img, "https://image.tmdb.org/t/p/w300/poster.jpg");
        assert_eq!(card.category(), "tv");
        assert_eq!(card.tmdb_ref(), Some((MediaKind::Tv, TmdbId::new(1399))));
    }

    #[test]
    fn a_movie_card_leaves_the_show_fields_out() {
        let details = MediaDetails { kind: MediaKind::Movie, number_of_seasons: None, ..show_details() };
        let card = TorrCard::from_details(&details, "");

        let data = encode_data(&card);

        assert!(data.contains(&format!(r#""{MARKER_KEY}":true"#)), "{data}");
        assert!(data.contains(r#""id":1399"#), "{data}");
        assert!(!data.contains("\"name\""), "{data}");
        assert!(!data.contains("first_air_date"), "{data}");
        assert_eq!(card.tmdb_ref(), Some((MediaKind::Movie, TmdbId::new(1399))));
    }

    #[test]
    fn reads_a_card_written_by_another_client() {
        // Trimmed `data` of a torrent another TorrServer client added: numbers as strings, nulls,
        // and fields this crate does not know.
        let data = r#"{"movie":{"poster_path":"/u3bZgnGQ9T01sWNhyveQz0wH0Hl.jpg",
            "overview":"Кровавая борьба за Железный трон","genre_ids":[10765,18,"x"],"id":1399,
            "original_language":"en","backdrop_path":"/2OMB0ynKlyIenMJWI2Dy9IWT4c.jpg","popularity":352.6,
            "vote_count":24000,"vote_average":8.456,"imdb_id":"tt0944947","kinopoisk_id":"464963",
            "original_name":"Game of Thrones","name":"Игра престолов","first_air_date":"2011-04-17",
            "origin_country":["US"],"status":"Ended","pg":"18+","release_quality":"","imdb_rating":"9.2",
            "kp_rating":9.0,"source":"tmdb","number_of_seasons":8,"number_of_episodes":73,
            "next_episode_to_air":null,"img":"https://image.tmdb.org/t/p/w300/u3bZgnGQ9T01sWNhyveQz0wH0Hl.jpg"}}"#;

        let Some(card) = decode_data(data) else { panic!("the card should decode") };

        assert_eq!(card.tmdb_ref(), Some((MediaKind::Tv, TmdbId::new(1399))));
        assert_eq!(card.name, "Игра престолов");
        assert_eq!(card.genre_ids, vec![10765, 18]);
        assert_eq!(card.number_of_episodes, Some(73));
    }

    #[test]
    fn data_without_a_card_reads_as_none() {
        let cases = ["", "not json", "[1,2]", r#"{"TorrServer":{"Files":[]}}"#, r#"{"movie":{"title":"No id"}}"#];

        for data in cases {
            assert_eq!(decode_data(data), None, "{data}");
        }
    }

    #[test]
    fn an_id_from_another_catalog_is_not_a_tmdb_ref() {
        let card = TorrCard { id: 464_963, source: String::from("kp"), ..TorrCard::default() };

        assert_eq!(card.tmdb_ref(), None);
    }

    #[test]
    fn putting_a_card_in_keeps_the_stored_file_list() {
        let existing = r#"{"TorrServer":{"Files":[{"id":1,"path":"Show/S01E01.mkv","length":10}]}}"#;
        let card = TorrCard { id: 7, title: String::from("Show"), ..TorrCard::default() };

        let data = with_card(existing, &card);

        assert_eq!(decode_data(&data), Some(card));
        assert_eq!(stored_files(&data), vec![FileStat { id: 1, path: String::from("Show/S01E01.mkv"), length: 10 }]);
    }
}
