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
        let tag_at = (0..words.len()).find(|&at| is_tag_at(&words, at));
        let season_at = words.iter().position(|word| is_season(word) || is_three_d(word));
        let season_at = season_at.or_else(|| issues_at_start(&words));
        let cut = [tag_at, season_at].into_iter().flatten().min();
        let kept = &words[..cut.map_or(words.len(), |at| keep_before(&words, at))];

        let name = kept.join(" ");
        let name = name.trim_matches(TRIM);
        if !name.is_empty() {
            names.push(name.to_owned());
        }

        // A season or 3D marker ends this name only; a tag ends the names.
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
        rip|d-theater|
        (?:\w+-?)?remux(?:-[\w.]+)*|
        web-?dl(?:-[\w.]+)*|web|blu-?ray|hdtv(?:-[\w.]+)*|
        (?:\d+[*xх])?dvd-?\d*(?:-[\w.]+)*|[tт]s(?:-[\w.]+)*|telesynch?|
        (?:hdr?|uhd|fhd)?\d{3,4}[pр]|[24]k|uhd|hevc|avc|x26[45]|h\.?26[45]|hdr\d*\+?|sdr|
        bd3d|overunder|sbs|стереопара|
        \+
    )$")
});

/// A rip: a source in front (`SATRip`, `UHD-BDRip`, `НDRip` with a Cyrillic
/// letter), maybe a codec or resolution after (`DVDRip-AVC`, `BDRip720p`).
/// The source must look like an abbreviation, so `Road Trip` stays a name.
static RIP: LazyLock<Regex> = LazyLock::new(|| re(r"^(?:\w+-)*(\w+?)[Rr]ip(?:\d{3,4}[pр])?(?:-[\w.]+)*$"));

/// Tags only when another tag, a release group (`by …`) or the end follows:
/// `Hybrid 1080p`, `BD Remux`, `DVB by …`, but not `Hybrid Theory`.
const SOFT_TAGS: &[&str] = &["hybrid", "bd", "dvb", "dsr"];

static SEASON: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?ix)^(?:сезон\w*|серии|серия|season|s\d{1,2}(?:e\d+(?:-\d+)?)?|тв-\d+|tv)$")
});

/// `/ Issues 1-45 /` as a segment of its own; inside a name the word belongs to it.
static ISSUES: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)^выпуск\w*:?$"));

fn is_three_d(word: &str) -> bool {
    matches!(word.to_lowercase().as_str(), "3d" | "3д")
}

fn issues_at_start(words: &[&str]) -> Option<usize> {
    let first = words.first()?;

    ISSUES.is_match(first).then_some(0)
}

/// `2`, `1-8`, `9,10,11`, `2x`: a count or range read with the word after it.
static COUNT: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)^\d{1,2}(?:[-–,]\d{1,2})*[xх×]?$"));

fn is_tag_at(words: &[&str], at: usize) -> bool {
    let word = words[at].trim_matches(&[',', ';', ':'][..]);
    if SOFT_TAGS.contains(&word.to_lowercase().as_str()) {
        let Some(next) = words.get(at + 1) else {
            return true;
        };

        return matches!(*next, "от" | "by") || is_tag_at(words, at + 1);
    }

    is_tag(word)
}

/// Tags glued together count by their first part: `MVO+AVO`, `DVD9+DVD5`,
/// `WEB-DLRip/BDRip`, `480p/720p`.
fn is_tag(word: &str) -> bool {
    let first = word.split(['+', '/']).find(|part| !part.is_empty());
    let Some(first) = first else {
        return word == "+";
    };

    TAG.is_match(first) || is_rip(first)
}

fn is_rip(word: &str) -> bool {
    let Some(caps) = RIP.captures(word) else {
        return false;
    };

    let source = caps.get(1).map_or("", |source| source.as_str());
    let capitals = source.chars().filter(|ch| ch.is_uppercase()).count();
    if capitals >= 2 {
        return true;
    }

    KNOWN_RIPS.contains(&source.to_lowercase().as_str())
}

/// Sources a capital or two cannot tell from a word (`CamRip`, `dvdrip`).
const KNOWN_RIPS: &[&str] = &[
    "dvd", "bd", "web", "webdl", "dl", "hdtv", "hd", "sat", "tv", "cam", "dcp", "vhs", "hybrid",
    "theater",
];

fn is_season(word: &str) -> bool {
    SEASON.is_match(word.trim_matches(&[',', ';', ':'][..]))
}

/// Where to cut for a tag at `at`: before the count in front of it too
/// (`2 x MVO`, `2x MVO`, a season range like `1-8` before the season word),
/// and before the preposition of an `in 3D`.
fn keep_before(words: &[&str], at: usize) -> usize {
    let mut cut = at;
    if is_three_d(words[at]) && cut > 0 && matches!(words[cut - 1], "в" | "in") {
        return cut - 1;
    }

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
    fn rips_with_a_source_and_a_codec() {
        let cases = [
            ("Симпсоны / The Simpsons [S04] (1992-1993) SATRip", "Симпсоны / The Simpsons"),
            ("Чужой [01-15 из 16] (2021) WEBRip-AVC от Files-x", "Чужой"),
            ("Брат (1997) HDDVRip-AVC от New-Team", "Брат"),
            ("Оппенгеймер / Oppenheimer (2023) UHD-BDRip 1080p H.265", "Оппенгеймер / Oppenheimer"),
            ("Зелёная миля / The Green Mile (1999) НDRip от Scarabey | D", "Зелёная миля / The Green Mile"),
            ("Чужой / Alien (1979) D-Theater Rip-AVC от ExKinoRay", "Чужой / Alien"),
            ("Оппенгеймер / Oppenheimer (2023) CamRip [H.264/1080p]", "Оппенгеймер / Oppenheimer"),
            ("Матрица / The Matrix (1999) HybridRip-AVC | D, P", "Матрица / The Matrix"),
            ("Шерлок / Sherlock [S03] (2014) WED-DLRip от Scarabey", "Шерлок / Sherlock"),
            ("Побег из Шоушенка / The Shawshank Redemption (1994) BDRip720p от FreeHD", "Побег из Шоушенка / The Shawshank Redemption"),
        ];

        for (title, name) in cases {
            assert_eq!(release_name(title), name, "{title}");
        }
    }

    #[test]
    fn remuxes_discs_and_screen_recordings() {
        let cases = [
            ("Матрица / The Matrix (1999) HDDVD-Remux", "Матрица / The Matrix"),
            ("Интерны [101-121] (2011-2012) SATRemux от SilverCinema", "Интерны"),
            ("Чужой / Alien (1979) BD Remux 1080p Director's Cut", "Чужой / Alien"),
            ("Король Лев / The Lion King (1994) 2*DVD9", "Король Лев / The Lion King"),
            ("Игра престолов / Game of Thrones [S01] (2011) 5xDVD от New-Team", "Игра престолов / Game of Thrones"),
            ("Шерлок / Sherlock [S03] (2014) DVD9+DVD5 R5 от New-Team", "Шерлок / Sherlock"),
            ("Оппенгеймер / Oppenheimer (2023) TS-AVC | D | TS", "Оппенгеймер / Oppenheimer"),
            ("Чебурашка (2022) TeleSynch 720p | AVC", "Чебурашка"),
            ("Реальные пацаны [03x15-16] (2011) DVB by kamyshin", "Реальные пацаны"),
        ];

        for (title, name) in cases {
            assert_eq!(release_name(title), name, "{title}");
        }
    }

    #[test]
    fn glued_tags_and_slash_separated_episodes() {
        let cases = [
            ("Терминатор / Terminator [1984, Боевик] MVO+AVO +Original+Sub", "Терминатор / Terminator"),
            ("Футурама (Сезон 7) / Futurama (Season 7) (2012-2013) WEB-DLRip/BDRip Ukr/Eng", "Футурама / Futurama"),
            ("Рик и Морти /Rick and Morty /s05e00-10 /HD1080p WEBRip /Полный 5 сезон", "Рик и Морти / Rick and Morty"),
            ("Южный парк / South Park / 9,10,11,12 сезоны / WEB-DLRip/BDRip", "Южный парк / South Park"),
            ("Маша и Медведь / Выпуск 1-45 (45) (Олег Кузовков)", "Маша и Медведь"),
        ];

        for (title, name) in cases {
            assert_eq!(release_name(title), name, "{title}");
        }
    }

    #[test]
    fn three_d_goes_with_its_preposition() {
        let title = "Король Лев в 3Д / The Lion King 3D OverUnder / Вертикальная стереопара";

        assert_eq!(release_name(title), "Король Лев / The Lion King");
    }

    #[test]
    fn words_that_only_look_like_tags_stay() {
        let cases = [
            ("Road Trip (2000) BDRip 1080p", "Road Trip"),
            ("Linkin Park - Hybrid Theory (2000) DVD9", "Linkin Park - Hybrid Theory"),
            ("Смешарики. Пин-код. Выпуск 5. Второе солнце / Смешарики / Сезон: 1", "Смешарики. Пин-код. Выпуск 5. Второе солнце / Смешарики"),
            ("Офис / The Office (UK) / Рождественские выпуски / Christmas Specials / 2003 / ПМ", "Офис / The Office / Рождественские выпуски / Christmas Specials"),
        ];

        for (title, name) in cases {
            assert_eq!(release_name(title), name, "{title}");
        }
    }

    #[test]
    fn a_title_of_tags_only_stays_whole() {
        assert_eq!(release_name("WEB-DL 1080p"), "WEB-DL 1080p");
    }
}
