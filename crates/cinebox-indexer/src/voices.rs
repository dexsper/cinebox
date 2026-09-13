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

/// [`voices`] for a title the caller has already lowercased.
pub(crate) fn voices_lower(lower: &str) -> Vec<&'static str> {
    let mut seen = vec![false; VOICES.len()];

    for found in MATCHER.find_overlapping_iter(lower) {
        seen[found.pattern().as_usize()] = true;
    }

    let hits = seen.iter().enumerate().filter(|(_, hit)| **hit);

    hits.map(|(index, _)| VOICES[index].1).collect()
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
