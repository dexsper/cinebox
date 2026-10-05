//! Substring match against the studio list (one name per line in `voices.txt`).

use std::sync::LazyLock;

use aho_corasick::AhoCorasick;

const RAW: &str = include_str!("voices.txt");

/// `(lowercase needle, original line)`. Deduped by needle once at startup.
static VOICES: LazyLock<Vec<(String, &'static str)>> = LazyLock::new(|| {
    let mut out = Vec::new();
    for display in RAW.lines().filter(|line| !line.is_empty()) {
        let lower = display.to_lowercase();
        if out.iter().any(|(have, _)| have == &lower) {
            continue;
        }
        out.push((lower, display));
    }
    out
});

/// One automaton scan finds every studio at once instead of ~900 `contains`
/// passes per title.
static MATCHER: LazyLock<AhoCorasick> = LazyLock::new(|| {
    let needles = VOICES.iter().map(|(needle, _)| needle.as_str());

    AhoCorasick::new(needles).unwrap_or_else(|error| panic!("voices automaton: {error}"))
});

/// Studios whose names appear in `title` (original casing from the list).
#[must_use]
pub fn voices(title: &str) -> Vec<&'static str> {
    if title.is_empty() {
        return Vec::new();
    }

    voices_lower(&title.to_lowercase())
}

/// [`voices`] for a title the caller has already lowercased, in the order
/// they first appear. Whole words only: a short studio name must not match
/// inside a film's name. A name inside a longer one found there is dropped.
pub(crate) fn voices_lower(lower: &str) -> Vec<&'static str> {
    let mut found: Vec<(usize, usize, usize)> = Vec::new();

    for hit in MATCHER.find_overlapping_iter(lower) {
        if !whole_word(lower, hit.start(), hit.end()) {
            continue;
        }

        found.push((hit.start(), hit.end(), hit.pattern().as_usize()));
    }

    let inside_another = |&(start, end, index): &(usize, usize, usize)| {
        found.iter().any(|&(other_start, other_end, other)| {
            let covers = other_start <= start && end <= other_end;
            covers && other != index && other_end - other_start > end - start
        })
    };
    let mut kept: Vec<(usize, usize)> = Vec::new();
    for hit in found.iter().filter(|hit| !inside_another(hit)) {
        let (start, _, index) = *hit;
        if kept.iter().any(|(_, have)| *have == index) {
            continue;
        }

        kept.push((start, index));
    }

    kept.sort_by_key(|(start, _)| *start);
    kept.into_iter().map(|(_, index)| VOICES[index].1).collect()
}

fn whole_word(text: &str, start: usize, end: usize) -> bool {
    let before = text[..start].chars().next_back();
    let after = text[end..].chars().next();

    !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
}

/// Detected studios in catalog order (same sequence as `voices.txt`).
#[must_use]
pub fn studios_in_catalog_order(
    found: impl IntoIterator<Item = &'static str>,
) -> Vec<&'static str> {
    let have: Vec<&'static str> = found.into_iter().collect();
    VOICES
        .iter()
        .filter_map(|(_, display)| have.contains(display).then_some(*display))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_lostfilm_and_skips_missing() {
        let found = voices("Dune.2021.WEB-DL.LostFilm");
        assert!(found.contains(&"LostFilm"), "{found:?}");
        assert!(voices("Dune.2021.WEB-DL").is_empty());
    }

    #[test]
    fn a_studio_inside_another_word_is_not_found() {
        assert!(voices("Интерстеллар / Interstellar (2014) BDRip").is_empty());
        assert_eq!(voices("Офис / The Office | Интер"), vec!["Интер"]);
    }

    #[test]
    fn a_name_inside_a_longer_one_is_dropped() {
        let found = voices("Пацаны / The Boys | Кубик в Кубе");
        assert_eq!(found, vec!["Кубик в Кубе"]);
    }

    #[test]
    fn studios_come_in_title_order() {
        let found = voices("Dune (2021) | LostFilm, HDrezka Studio");
        assert_eq!(found, vec!["LostFilm", "HDrezka"]);

        let found = voices("Dune (2021) | HDrezka Studio, LostFilm");
        assert_eq!(found, vec!["HDrezka", "LostFilm"]);
    }

    #[test]
    fn catalog_order_keeps_list_sequence() {
        let ordered = studios_in_catalog_order(["HDrezka", "LostFilm"]);
        assert_eq!(ordered, vec!["LostFilm", "HDrezka"]);
    }

    #[test]
    fn casefold_duplicates_are_collapsed() {
        let needles: Vec<&str> = VOICES.iter().map(|(n, _)| n.as_str()).collect();
        let mut unique = needles.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(needles.len(), unique.len());
    }
}
