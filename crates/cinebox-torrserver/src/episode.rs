//! Season / episode from a torrent file path.

use std::sync::LazyLock;

use regex::Regex;

/// A bare number is a season only up to here: past it, `Golgo 13`, `86` and `Mob Psycho 100` are titles.
const MAX_BARE_SEASON: u32 = 9;

/// Words that a bare number after them belongs to, as in `Kaiju No. 8` or `Disc 2`.
const NUMBERED_WORDS: &[&str] = &["no", "vol", "volume", "part", "pt", "ep", "episode", "disc", "disk", "cd", "dvd"];

const BARE_FILE_PATTERN: &str = r"(?:^|\s)(\S+)\s+(\d{1,2})\s+(?:-\s*\d{1,3}\b|\[\d{1,3}\])";

/// Season and episode parsed from a file path.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FileEpisode {
    pub season: Option<u32>,
    pub episode: Option<u32>,
}

/// Human title: basename without extension, `_` / `.` → spaces.
#[must_use]
pub fn file_display_name(path: &str) -> String {
    let base = basename(path);
    let stem = match base.rsplit_once('.') {
        Some((stem, ext)) if ext.len() <= 5 && !stem.is_empty() => stem,
        _ => base,
    };

    stem.replace(['_', '.'], " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Parse `S01E02`, `1x02`, `сезон 1 серия 2`, `2nd Season`, folder `Season 2`, etc.
///
/// The first folder of a TorrServer path is the torrent's name, so a release such as `Title 2 [1080p]` names the
/// season of its bare `07.mkv`. A season named nowhere is 1 for a serial. The season is the release's own: it is
/// off for a show TMDB numbers absolutely, or one whose sequel TMDB lists as a show of its own.
#[must_use]
pub fn parse_file_episode(path: &str, serial: bool) -> FileEpisode {
    let normalized = path.replace('\\', "/");
    let mut parts = normalized.rsplit('/');
    let fname = parts.next().unwrap_or_default().replace('_', " ");
    let folders: Vec<String> = parts.map(|folder| folder.replace('_', " ")).collect();

    let folder_season = || folders.iter().find_map(|folder| capture_first(folder, &FOLDER_SEASON));
    let named_season = capture_first(&fname, &SEASON_EPISODE.0).or_else(folder_season);
    let season = named_season.or_else(|| serial.then(|| serial_season(&fname, &folders)));

    let episode = capture_first(&fname, &SEASON_EPISODE.1).or_else(|| leading_number(&file_display_name(path)));

    FileEpisode { season, episode }
}

/// The season of a serial whose path names none: a bare number after the title, else 1.
fn serial_season(fname: &str, folders: &[String]) -> u32 {
    let folder_season = || folders.iter().find_map(|folder| bare_season(folder, &BARE_FOLDER_SEASON));

    bare_season(fname, &BARE_FILE_SEASON).or_else(folder_season).unwrap_or(1)
}

/// `pattern` captures the title's last word, then the number after it.
fn bare_season(text: &str, pattern: &Regex) -> Option<u32> {
    pattern.captures_iter(text).find_map(|caps| {
        let word = caps.get(1)?.as_str().trim_end_matches('.').to_lowercase();
        let is_title_word = word.chars().any(char::is_alphabetic) && !NUMBERED_WORDS.contains(&word.as_str());
        if !is_title_word {
            return None;
        }

        parse_group(&caps, 2).filter(|season| (2..=MAX_BARE_SEASON).contains(season))
    })
}

fn basename(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().filter(|name| !name.is_empty()).unwrap_or(path)
}

fn leading_number(name: &str) -> Option<u32> {
    static LEAD: LazyLock<Regex> = LazyLock::new(|| re(r"^(\d{1,3})\b"));
    let caps = LEAD.captures(name)?;
    parse_group(&caps, 1)
}

fn capture_first(text: &str, regexes: &[Regex]) -> Option<u32> {
    for re in regexes {
        let Some(caps) = re.captures(text) else {
            continue;
        };

        if let Some(value) = parse_group(&caps, 1) {
            return Some(value);
        }
    }
    None
}

fn parse_group(caps: &regex::Captures<'_>, i: usize) -> Option<u32> {
    caps.get(i)?.as_str().parse().ok()
}

fn re(pattern: &'static str) -> Regex {
    Regex::new(pattern).unwrap_or_else(|error| panic!("file-episode regex {pattern}: {error}"))
}

static SEASON_EPISODE: LazyLock<(Vec<Regex>, Vec<Regex>)> = LazyLock::new(|| {
    (
        vec![
            re(r"(?i)\bs(\d+)\.?ep?\d+\b"),
            re(r"(?i)\b(\d{1,2})[x\-]\d+\b"),
            re(r"(?i)\bs(\d{2})\d{2,3}\b"),
            re(r"(?i)season (\d+) episode \d+"),
            re(r"(?i)сезон (\d+) серия \d+"),
            re(r"(?i)(\d+) season \d+ episode"),
            re(r"(?i)(\d+) сезон \d+ серия"),
            re(r"(?i)season (\d+)"),
            re(r"(?i)сезон (\d+)"),
            re(r"(?i)\b(\d+)(?:st|nd|rd|th) season"),
            re(r"(?i)(\d+) season"),
            re(r"(?i)(\d+) сезон"),
            re(r"(?i)\bs(\d+)\b"),
        ],
        vec![
            re(r"(?i)\bs\d+\.?ep?(\d+)\b"),
            re(r"(?i)\b\d{1,2}[x\-](\d+)\b"),
            re(r"(?i)\bs\d{2}(\d{2,3})\b"),
            re(r"(?i)season \d+ episode (\d+)"),
            re(r"(?i)сезон \d+ серия (\d+)"),
            re(r"(?i)\d+ season (\d+) episode"),
            re(r"(?i)\d+ сезон (\d+) серия"),
            re(r"(?i)episode (\d+)"),
            re(r"(?i)серия (\d+)"),
            re(r"(?i)(\d+) episode"),
            re(r"(?i)(\d+) серия"),
            re(r"(?i)\bep?\.?(\d+)\b"),
            re(r"(?i)\b(\d{1,3}) of \d+"),
            re(r"(?i)\b(\d{1,3}) из \d+"),
            re(r"(?i) - (\d{1,3})\b"),
            re(r"(?i)\[(\d{1,3})\]"),
            re(r"(?i)(\d+) сер"),
        ],
    )
});

static FOLDER_SEASON: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        re(r"(?i)season (\d+)"),
        re(r"(?i)сезон (\d+)"),
        re(r"(?i)\b(\d+)(?:st|nd|rd|th) season"),
        re(r"(?i)(\d+) season"),
        re(r"(?i)(\d+) сезон"),
        re(r"(?i)\bs(\d+)\b"),
    ]
});

/// `Title 2 - 07`, `Title 2 [07]`: the number right before the episode.
static BARE_FILE_SEASON: LazyLock<Regex> = LazyLock::new(|| re(BARE_FILE_PATTERN));

/// `Title 2 [WEB-DL 1080p]`, `Title 2 (2023)`, `Title 2`: the number that ends the title of a release.
static BARE_FOLDER_SEASON: LazyLock<Regex> = LazyLock::new(|| re(r"(?:^|\s)(\S+)\s+(\d{1,2})\s*(?:[\[(]|$)"));

#[cfg(test)]
mod tests {
    use super::*;

    const fn at(season: u32, episode: u32) -> FileEpisode {
        FileEpisode { season: Some(season), episode: Some(episode) }
    }

    fn serial(path: &str) -> FileEpisode {
        parse_file_episode(path, true)
    }

    #[test]
    fn a_bare_number_after_the_title_is_the_season() {
        let path = "Kabushikigaisha Magilumiere 2 [WEB-DL 1080p]/Kabushikigaisha Magilumiere 2 - 07 [1080p].mkv";

        assert_eq!(serial(path), at(2, 7));
    }

    #[test]
    fn an_ordinal_season_is_the_season() {
        let path = "Oshi no Ko 2nd Season [1080p]/[Erai-raws] Oshi no Ko 2nd Season - 07 [1080p].mkv";

        assert_eq!(serial(path), at(2, 7));
    }

    #[test]
    fn the_torrent_folder_names_the_season_of_bare_episodes() {
        assert_eq!(serial("Kabushikigaisha Magilumiere 2 [WEB-DL 1080p]/07.mkv"), at(2, 7));
        assert_eq!(serial("[Judas] Vinland Saga 2 (2023) [1080p]/Episode 07.mkv"), at(2, 7));
        assert_eq!(serial("Spy x Family 2nd Season [1080p]/[SubsPlease] Spy x Family - 07 (1080p).mkv"), at(2, 7));
    }

    #[test]
    fn a_season_folder_above_the_parent_still_counts() {
        assert_eq!(serial("Show Season 2/Extras/07.mkv"), at(2, 7));
    }

    #[test]
    fn short_season_forms_keep_working() {
        assert_eq!(serial("[SubsPlease] Dr. Stone S2 - 07 (1080p).mkv"), at(2, 7));
        assert_eq!(serial("Kimetsu no Yaiba Season 2 [07].mkv"), at(2, 7));
    }

    #[test]
    fn sxxexx_and_alike_keep_working() {
        assert_eq!(serial("Show/Show.S01E02.1080p.WEB-DL.mkv"), at(1, 2));
        assert_eq!(serial("Show 2 [1080p]/Show.S03E04.mkv"), at(3, 4));
        assert_eq!(serial("Show/Show.2x05.mkv"), at(2, 5));
        assert_eq!(serial("Сериал/Сериал сезон 2 серия 5.mkv"), at(2, 5));
    }

    #[test]
    fn a_season_folder_wins_over_a_number_in_the_title() {
        assert_eq!(serial("Show 3 [BD]/Season 2/Show 3 - 08.mkv"), at(2, 8));
    }

    #[test]
    fn numbers_that_are_part_of_the_title_are_not_seasons() {
        let mob_psycho = "[SubsPlease] Mob Psycho 100 (1080p)/[SubsPlease] Mob Psycho 100 - 03 (1080p).mkv";

        assert_eq!(serial(mob_psycho), at(1, 3));
        assert_eq!(serial("86 - Eighty Six [1080p]/86 - 05 [1080p].mkv"), at(1, 5));
        assert_eq!(serial("[SubsPlease] Kaiju No. 8 (1080p)/[SubsPlease] Kaiju No. 8 - 05 (1080p).mkv"), at(1, 5));
        assert_eq!(serial("Golgo 13 [DVDRip]/Golgo 13 - 05.mkv"), at(1, 5));
        assert_eq!(serial("Show [BD]/Disc 2/05.mkv"), at(1, 5));
    }

    #[test]
    fn a_movie_title_ending_in_a_number_has_no_season() {
        let path = "Blade Runner 2049 (2017) [2160p]/Blade.Runner.2049.2017.2160p.mkv";

        assert_eq!(parse_file_episode(path, false).season, None);
    }

    #[test]
    fn sxxexx_and_folder_season() {
        let a = parse_file_episode("Show/S02E07.mkv", true);
        assert_eq!(a.season, Some(2));
        assert_eq!(a.episode, Some(7));

        let b = parse_file_episode("Show/Season 3/08.mkv", true);
        assert_eq!(b.season, Some(3));
        assert_eq!(b.episode, Some(8));
    }

    #[test]
    fn movie_has_no_forced_season() {
        let a = parse_file_episode("Dune.2021.mkv", false);
        assert_eq!(a.season, None);
    }

    #[test]
    fn display_name_strips_ext_and_dots() {
        assert_eq!(file_display_name(r"Folder\S01E01.Pilot.Name.mkv"), "S01E01 Pilot Name");
    }
}
