//! Skip segments from the chapter marks of the file itself.
//!
//! Works offline and always matches the cut being played. Only a chapter whose
//! whole title names a segment counts ("Opening", "End Credits", "OP2"), so a
//! scene title such as "The Opening Night" stays unmarked.

use crate::{MediaSegments, SegmentType, TimeRange};

/// One chapter mark as the player reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChapterMark<'a> {
    pub start_ms: u64,
    pub title: &'a str,
}

const INTRO: &[&str] = &[
    "intro",
    "opening",
    "op",
    "opening credits",
    "opening titles",
    "opening title",
    "main titles",
    "main title",
    "title sequence",
    "начальные титры",
    "вступительные титры",
    "заставка",
    "опенинг",
    "オープニング",
    "片头",
    "片头曲",
];

const RECAP: &[&str] = &["recap", "previously", "previously on", "ранее", "в предыдущих сериях"];

const CREDITS: &[&str] = &[
    "credits",
    "credits start",
    "end credits",
    "ending credits",
    "closing credits",
    "end titles",
    "ending",
    "ed",
    "outro",
    "титры",
    "финальные титры",
    "конечные титры",
    "эндинг",
    "エンディング",
    "片尾",
    "片尾曲",
];

const PREVIEW: &[&str] = &[
    "preview",
    "episode preview",
    "next episode preview",
    "next episode",
    "next time",
    "анонс",
    "превью",
    "予告",
    "次回予告",
    "下集预告",
];

const NAMES: [(SegmentType, &[&str]); 4] = [
    (SegmentType::Intro, INTRO),
    (SegmentType::Recap, RECAP),
    (SegmentType::Credits, CREDITS),
    (SegmentType::Preview, PREVIEW),
];

/// `None` when no chapter is marked, or every marked one looks mislabeled.
#[must_use]
pub fn segments_from_chapters(chapters: &[ChapterMark<'_>], duration_ms: u64) -> Option<MediaSegments> {
    let mut segments = MediaSegments::default();

    for (index, chapter) in chapters.iter().enumerate() {
        let Some(ty) = classify(chapter.title) else {
            continue;
        };

        let end_ms = chapters.get(index + 1).map(|next| next.start_ms);
        let range = TimeRange { start_ms: Some(chapter.start_ms), end_ms };

        if !is_plausible(&range, duration_ms) {
            continue;
        }

        push_merged(segments.segments_of_mut(ty), range);
    }

    if segments.is_empty() {
        return None;
    }

    Some(segments)
}

/// The segment a chapter title names, if any.
///
/// Each side of a bilingual title ("End Credits / Конец") is tried on its own.
#[must_use]
pub fn classify(title: &str) -> Option<SegmentType> {
    title.split('/').find_map(classify_name)
}

fn classify_name(title: &str) -> Option<SegmentType> {
    let name = normalize(title);
    let name = name.as_str();
    let (ty, _) = NAMES.iter().find(|(_, names)| names.contains(&name))?;

    Some(*ty)
}

// Consecutive chapters of one kind ("Ending", then "Ending (cont.)") are one skip.
fn push_merged(ranges: &mut Vec<TimeRange>, range: TimeRange) {
    if let Some(last) = ranges.last_mut()
        && last.end_ms == range.start_ms
    {
        last.end_ms = range.end_ms;
        return;
    }

    ranges.push(range);
}

// A segment over half the file is a mislabeled chapter, not an intro or credits.
fn is_plausible(range: &TimeRange, duration_ms: u64) -> bool {
    if duration_ms == 0 {
        return true;
    }

    let start = range.start_or_zero();
    let end = range.end_or_duration(duration_ms);

    end > start && end - start <= duration_ms / 2
}

// Lowercase words without punctuation, numbering, a trailing number and
// bracketed asides, so "01. Opening", "OP2", "Ending 2" and
// `Opening Credits ("Twas a Long Time Ago...")` match the lists.
fn normalize(title: &str) -> String {
    let lower = without_brackets(title).to_lowercase();
    let spaced = lower.replace(|ch: char| !ch.is_alphanumeric(), " ");
    let words = spaced.split_whitespace().filter(|word| !is_number(word));
    let joined = words.collect::<Vec<_>>().join(" ");

    joined.trim_end_matches(is_digit).to_owned()
}

fn without_brackets(title: &str) -> String {
    let mut depth = 0_u32;
    let mut kept = String::with_capacity(title.len());

    for ch in title.chars() {
        match ch {
            '(' | '[' | '{' => depth = depth.saturating_add(1),
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ if depth == 0 => kept.push(ch),
            _ => {}
        }
    }

    kept
}

fn is_number(word: &str) -> bool {
    word.chars().all(is_digit)
}

fn is_digit(ch: char) -> bool {
    ch.is_ascii_digit()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE_MS: u64 = 60_000;

    fn mark(start_ms: u64, title: &str) -> ChapterMark<'_> {
        ChapterMark { start_ms, title }
    }

    fn range(start_ms: u64, end_ms: Option<u64>) -> TimeRange {
        TimeRange { start_ms: Some(start_ms), end_ms }
    }

    #[test]
    fn classifies_whole_titles_only() {
        assert_eq!(classify("Opening"), Some(SegmentType::Intro));
        assert_eq!(classify("OP2"), Some(SegmentType::Intro));
        assert_eq!(classify("01. Opening Credits"), Some(SegmentType::Intro));
        assert_eq!(classify("End Credits"), Some(SegmentType::Credits));
        assert_eq!(classify("Ending 2"), Some(SegmentType::Credits));
        assert_eq!(classify("Финальные титры"), Some(SegmentType::Credits));
        assert_eq!(classify("Next Episode Preview"), Some(SegmentType::Preview));
        assert_eq!(classify("Previously on"), Some(SegmentType::Recap));

        assert_eq!(classify(" 28. End Credits / Конец"), Some(SegmentType::Credits));
        assert_eq!(classify("Opening Credits (\"Twas a Long Time Ago...\")"), Some(SegmentType::Intro));
        assert_eq!(classify("Episode 26 Preview"), Some(SegmentType::Preview));
        assert_eq!(classify("エンディング"), Some(SegmentType::Credits));
        assert_eq!(classify("次回予告"), Some(SegmentType::Preview));

        assert_eq!(classify("The Opening Night"), None);
        assert_eq!(classify("Credits End"), None);
        assert_eq!(classify("\"Part Of The Journey Is The End\""), None);
        assert_eq!(classify("00:00:00.000"), None);
        assert_eq!(classify("アバン"), None);
        assert_eq!(classify("Chapter 01"), None);
        assert_eq!(classify("Scene 3"), None);
        assert_eq!(classify("Studio Logo"), None);
        assert_eq!(classify(""), None);
    }

    #[test]
    fn chapter_runs_until_the_next_one() {
        let chapters = [mark(0, "Prologue"), mark(90_000, "Opening"), mark(180_000, "Part A")];

        let segments = segments_from_chapters(&chapters, 24 * MINUTE_MS);
        let Some(segments) = segments else { panic!("no segments from a marked opening") };

        assert_eq!(segments.intro, vec![range(90_000, Some(180_000))]);
        assert_eq!(segments.credits, []);
    }

    #[test]
    fn last_chapter_runs_to_the_end() {
        let chapters = [mark(0, "Part 01"), mark(6_490_000, "Credits")];

        let segments = segments_from_chapters(&chapters, 6_620_000);
        let Some(segments) = segments else { panic!("no segments from a marked credits chapter") };

        assert_eq!(segments.credits, vec![range(6_490_000, None)]);
    }

    #[test]
    fn merges_consecutive_chapters_of_one_kind() {
        let chapters = [
            mark(0, "Part B"),
            mark(21 * MINUTE_MS, "Ending"),
            mark(22 * MINUTE_MS, "ED 2"),
            mark(23 * MINUTE_MS, "Preview"),
        ];

        let segments = segments_from_chapters(&chapters, 24 * MINUTE_MS);
        let Some(segments) = segments else { panic!("no segments from marked chapters") };

        assert_eq!(segments.credits, vec![range(21 * MINUTE_MS, Some(23 * MINUTE_MS))]);
        assert_eq!(segments.preview, vec![range(23 * MINUTE_MS, None)]);
    }

    #[test]
    fn drops_a_marked_chapter_over_half_the_file() {
        let chapters = [mark(0, "Intro"), mark(100 * MINUTE_MS, "Credits")];
        let segments = segments_from_chapters(&chapters, 105 * MINUTE_MS);

        let Some(segments) = segments else { panic!("credits dropped along with the intro") };

        assert_eq!(segments.intro, []);
        assert_eq!(segments.credits, vec![range(100 * MINUTE_MS, None)]);
    }

    #[test]
    fn unmarked_chapters_give_none() {
        let chapters = [mark(0, "Chapter 01"), mark(MINUTE_MS, "Chapter 02")];

        assert!(segments_from_chapters(&chapters, 10 * MINUTE_MS).is_none());
        assert!(segments_from_chapters(&[], 10 * MINUTE_MS).is_none());
    }
}
