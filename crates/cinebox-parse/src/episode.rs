//! Season / episode from a torrent file path (Lampa `episodes_parser.js`).

use std::sync::LazyLock;

use regex::Regex;

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

    stem.replace(['_', '.'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parse `S01E02`, `1x02`, `сезон 1 серия 2`, folder `Season 2`, etc.
#[must_use]
pub fn parse_file_episode(path: &str, serial: bool) -> FileEpisode {
    let normalized = path.replace('\\', "/");
    let parts: Vec<&str> = normalized.split('/').collect();
    let fname = parts.last().copied().unwrap_or(path).replace('_', " ");

    let folder = if parts.len() >= 2 {
        parts[parts.len() - 2].replace('_', " ")
    } else {
        String::new()
    };

    let mut season = capture_first(&fname, &SEASON_EPISODE.0);
    let mut episode = capture_first(&fname, &SEASON_EPISODE.1);
    if season.is_none() {
        season = capture_first(&folder, &FOLDER_SEASON);
    }

    if season.is_none() && serial {
        season = Some(1);
    }

    if episode.is_none() {
        episode = leading_number(&file_display_name(path));
    }

    FileEpisode { season, episode }
}

fn basename(path: &str) -> &str {
    path.rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
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
        re(r"(?i)(\d+) season"),
        re(r"(?i)(\d+) сезон"),
        re(r"(?i)\bs(\d+)\b"),
    ]
});

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(
            file_display_name(r"Folder\S01E01.Pilot.Name.mkv"),
            "S01E01 Pilot Name"
        );
    }
}
