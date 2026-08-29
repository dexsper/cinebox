//! Query text sent to Jackett extras / Prowlarr.

use cinebox_core::MediaKind;

/// Card search sent to Jackett extras / Prowlarr query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery {
    pub query: String,
    pub title: String,
    pub original_title: String,
    pub year: Option<u16>,
    pub kind: MediaKind,
    pub is_anime: bool,
    pub genres: Vec<String>,
}

/// Strip Lucene operators so titles like `Spider-Man: Brand New Day` stay a phrase.
#[must_use]
pub(crate) fn sanitize_query(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        let mapped = match ch {
            ':' | '/' | '\\' | '"' | '\'' | '[' | ']' | '{' | '}' | '!' | '(' | ')' | '&' | '|' => {
                ' '
            }
            other => other,
        };
        if !mapped.is_whitespace() {
            out.push(mapped);
            continue;
        }
        if out.ends_with(' ') {
            continue;
        }
        out.push(' ');
    }
    out.trim().to_owned()
}

pub(crate) fn search_text(raw: &str) -> String {
    let cleaned = sanitize_query(raw);
    if cleaned.is_empty() {
        return raw.trim().to_owned();
    }
    cleaned
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_lucene_colon() {
        assert_eq!(
            search_text("Spider-Man: Brand New Day"),
            "Spider-Man Brand New Day"
        );
        assert_eq!(search_text("Dune:  Part Two"), "Dune Part Two");
    }
}
