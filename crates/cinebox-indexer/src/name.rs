//! The film or show name out of a release title, without the tags around it.
//!
//! Trackers write titles in a few house styles (see the tests for real ones):
//!
//! - rutracker: `Names (Director) [year, country, genre, source] Dub + Sub`
//! - kinozal: `Names / year / voice codes / source (resolution) | studio`
//! - rutor: `Name N season (episodes) / Name (year) source resolution | studio`
//!
//! All keep the names first, split by ` / `. What follows a name is dropped:
//! bracketed groups, `|` sections, a year of its own, and the first tag word.

use std::sync::LazyLock;

use regex::Regex;

/// The names a release title starts with, in the local language and the
/// original one. Falls back to the whole title when nothing is left.
#[must_use]
pub fn release_name(title: &str) -> String {
    let head = title.split('|').next().unwrap_or_default();
    let flat = without_groups(head);

    let mut names = Vec::new();
    for segment in SEPARATOR.split(&flat) {
        let segment = segment.trim();
        if segment.is_empty() || YEAR_SEGMENT.is_match(segment) {
            break;
        }

        let words: Vec<&str> = segment.split_whitespace().collect();
        let tag_at = words.iter().position(|word| is_tag(word));
        let season_at = words.iter().position(|word| is_season(word));
        let cut = [tag_at, season_at].into_iter().flatten().min();
        let kept = &words[..cut.map_or(words.len(), |at| keep_before(&words, at))];

        let name = kept.join(" ");
        let name = name.trim_matches(TRIM);
        if !name.is_empty() {
            names.push(name.to_owned());
        }

        // A season marker ends this name only; a tag ends the names.
        if tag_at.is_some() {
            break;
        }
    }

    if names.is_empty() {
        return title.trim().to_owned();
    }

    names.join(" / ")
}

const TRIM: &[char] = &[' ', ',', ':', ';', '-', '–', '.', '+'];

/// ` / ` between names; `AC/DC` stays whole.
static SEPARATOR: LazyLock<Regex> = LazyLock::new(|| re(r"\s+/\s*|\s*/\s+"));

static YEAR_SEGMENT: LazyLock<Regex> =
    LazyLock::new(|| re(r"^(?:19|20)\d{2}(?:\s*[-–]\s*(?:19|20)?\d{2})?$"));

static TAG: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?ix)^(?:
        dub|mvo|dvo|avo|vo|original|sub|subs|
        web-?dl(?:rip)?(?:-\w+)?|webrip|web|blu-?ray|bdrip(?:-\w+)?|bdremux|remux|hdrip|
        hdtv(?:rip)?|dvdrip|dvd\d*|camrip|ts|
        \d{3,4}[pр]|4k|uhd|hevc|avc|x26[45]|h\.?26[45]|hdr\d*\+?|sdr|
        \+
    )$")
});

static SEASON: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?ix)^(?:сезон\w*|серии|серия|season|s\d{1,2}(?:e\d+)?|тв-\d+|tv)$")
});

/// `2`, `1-8`, `2x`: a count or range read with the word after it.
static COUNT: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)^\d{1,2}(?:[-–]\d{1,2})?[xх×]?$"));

fn is_tag(word: &str) -> bool {
    TAG.is_match(word.trim_matches(&[',', ';', ':'][..]))
}

fn is_season(word: &str) -> bool {
    SEASON.is_match(word.trim_matches(&[',', ';', ':'][..]))
}

/// Where to cut for a tag at `at`: before the count in front of it too
/// (`2 x MVO`, `2x MVO`, a season range like `1-8` before the season word).
fn keep_before(words: &[&str], at: usize) -> usize {
    let mut cut = at;
    if cut > 0 && matches!(words[cut - 1], "x" | "х" | "×") {
        cut -= 1;
    }

    if cut > 0 && COUNT.is_match(words[cut - 1]) {
        cut -= 1;
    }

    cut
}

/// Drops `(...)` and `[...]` with what is inside, and anything after an
/// unclosed one (feeds cut long titles off mid-group).
fn without_groups(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut open: Vec<char> = Vec::new();

    for ch in text.chars() {
        match ch {
            '(' => open.push(')'),
            '[' => open.push(']'),
            _ if open.last() == Some(&ch) => {
                open.pop();
                out.push(' ');
            }
            _ if open.is_empty() => out.push(ch),
            _ => {}
        }
    }

    out
}

fn re(pattern: &'static str) -> Regex {
    Regex::new(pattern).unwrap_or_else(|error| panic!("name regex {pattern}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rutracker_style() {
        let title = "Дюна / Dune: Part One (Дени Вильнев / Denis Villeneuve) [2021, США, \
                     фантастика, BDRip 1080p] Dub (Пифагор) + MVO (Jaskier) + Original";

        assert_eq!(release_name(title), "Дюна / Dune: Part One");
    }

    #[test]
    fn rutracker_series() {
        let title = "Игра престолов / Game of Thrones / Сезон: 1 / Серии: 1-10 из 10 \
                     (Дэвид Наттер) [2011, США, WEB-DL 2160p] Dub + Original (Eng)";

        assert_eq!(release_name(title), "Игра престолов / Game of Thrones");
    }

    #[test]
    fn kinozal_style() {
        let title = "Дом дракона (2 сезон: 1-8 серии из 8) / House of the Dragon / 2024 / \
                     ДБ, 7 x ПМ, СТ / 4K, HEVC, SDR / WEB-DL (2160p)";

        assert_eq!(release_name(title), "Дом дракона / House of the Dragon");
    }

    #[test]
    fn rutor_style_keeps_the_name_after_a_season() {
        let title = "Пацаны 1 сезон (1-8 из 8) / The Boys (2019) BDRip 1080p | HEVC @ LostFilm";

        assert_eq!(release_name(title), "Пацаны / The Boys");
    }

    #[test]
    fn season_range_goes_with_its_word() {
        let title = "Игра престолов 1-8 сезон (1-73 из 73) / Game of Thrones (2011-2019) BDRemux";

        assert_eq!(release_name(title), "Игра престолов / Game of Thrones");
    }

    #[test]
    fn counted_tag_goes_with_its_count() {
        let title = "Оппенгеймер / Oppenheimer [2023, США, BDRip] 2x MVO + Original";

        assert_eq!(release_name(title), "Оппенгеймер / Oppenheimer");
    }

    #[test]
    fn bracketed_episodes_and_tags_after_the_name() {
        let title = "Одни из нас / The Last of Us [01x01-02 из 09] (2023) WEB-DL 1080p | Dubbing-Pro";

        assert_eq!(release_name(title), "Одни из нас / The Last of Us");
    }

    #[test]
    fn numbers_in_a_name_stay() {
        let title = "Бегущий по лезвию 2049 / Blade Runner 2049 / 2017 / ДБ / BDRip";

        assert_eq!(release_name(title), "Бегущий по лезвию 2049 / Blade Runner 2049");
    }

    #[test]
    fn slash_inside_a_word_is_not_a_separator() {
        assert_eq!(release_name("AC/DC: Live at River Plate (2011) BDRip"), "AC/DC: Live at River Plate");
    }

    #[test]
    fn unclosed_group_is_dropped() {
        assert_eq!(release_name("Шерлок / Sherlock [2010, Великобрит"), "Шерлок / Sherlock");
    }

    #[test]
    fn a_title_of_tags_only_stays_whole() {
        assert_eq!(release_name("WEB-DL 1080p"), "WEB-DL 1080p");
    }
}
