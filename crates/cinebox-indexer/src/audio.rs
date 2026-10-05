//! Voice-over tracks, original audio and subtitles named in a release title.
//!
//! Each tracker spells them its own way (see the tests for real ones):
//!
//! - rutracker: `Dub (studio) + 2x MVO + AVO (translator) + Original + Sub (Rus, Eng)`
//! - kinozal: two Cyrillic letters per track, professional or amateur and
//!   multi-, two- or one-voice, plus dubbing, author and subtitles, with a
//!   count like `7 x` in front
//! - rutor, after a `|`: `D, P, P2, L1, A`, the same in Latin letters

/// The kind of a voice-over track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Voiceover {
    Dubbing,
    Polyphonic,
    TwoVoice,
    /// One well-known translator reading every part.
    Author,
    OneVoice,
}

impl Voiceover {
    pub const ALL: [Self; 5] = [
        Self::Dubbing,
        Self::Polyphonic,
        Self::TwoVoice,
        Self::Author,
        Self::OneVoice,
    ];
}

/// What a title says about the sound and subtitles.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AudioInfo {
    /// Track kinds in [`Voiceover::ALL`] order, with how many of each.
    pub tracks: Vec<(Voiceover, u8)>,
    pub original: bool,
    pub subtitles: bool,
}

/// Reads [`AudioInfo`] from a title already lowercased.
pub(crate) fn audio_lower(lower: &str) -> AudioInfo {
    let mut head_words = lower.split('|');
    let head = words(head_words.next().unwrap_or_default());
    let tail: Vec<&str> = head_words.flat_map(words).collect();

    let mut counted = [0_u8; 5];
    let mut named = [false; 5];
    tally(&head, &mut counted, &mut named, head_code);
    tally(&tail, &mut counted, &mut named, |word| head_code(word).or_else(|| tail_code(word)));

    let mut tracks = Vec::new();
    for (slot, kind) in Voiceover::ALL.into_iter().enumerate() {
        let count = if counted[slot] > 0 { counted[slot] } else { u8::from(named[slot]) };
        if count > 0 {
            tracks.push((kind, count));
        }
    }

    let all = || head.iter().chain(&tail);
    AudioInfo {
        tracks,
        original: all().any(|word| word.starts_with("original") || word.starts_with("оригинал")),
        subtitles: all().any(|word| is_subtitles(word)),
    }
}

fn words(text: &str) -> Vec<&str> {
    let words = text.split(|c: char| !c.is_alphanumeric());

    words.filter(|word| !word.is_empty()).collect()
}

/// Codes count with the number before them (`7 x` code, `2x MVO`); a kind
/// written out as a word only says the kind is there, since kinozal repeats
/// its codes that way after a `|`.
fn tally(
    words: &[&str],
    counted: &mut [u8; 5],
    named: &mut [bool; 5],
    code: impl Fn(&str) -> Option<Voiceover>,
) {
    for (at, word) in words.iter().enumerate() {
        if let Some(kind) = code(word) {
            let slot = slot(kind);
            counted[slot] = counted[slot].saturating_add(times_before(words, at));
            continue;
        }

        if let Some(kind) = spelled(word) {
            named[slot(kind)] = true;
        }
    }
}

fn slot(kind: Voiceover) -> usize {
    Voiceover::ALL.iter().position(|have| *have == kind).unwrap_or_default()
}

/// `2x` or `2 x` in front of a code at `at`, else 1.
fn times_before(words: &[&str], at: usize) -> u8 {
    let before = |back: usize| at.checked_sub(back).and_then(|index| words.get(index));

    let glued = before(1).and_then(|word| word.strip_suffix(['x', 'х']));
    if let Some(count) = glued.and_then(|count| count.parse().ok()) {
        return count;
    }

    let multiplied = before(1).is_some_and(|word| matches!(*word, "x" | "х"));
    if !multiplied {
        return 1;
    }

    before(2).and_then(|count| count.parse().ok()).unwrap_or(1)
}

/// rutracker and kinozal codes, safe anywhere in a title.
fn head_code(word: &str) -> Option<Voiceover> {
    let kind = match word {
        "dub" | "дб" => Voiceover::Dubbing,
        "mvo" | "пм" | "лм" => Voiceover::Polyphonic,
        "dvo" | "пд" | "лд" => Voiceover::TwoVoice,
        "avo" | "ап" => Voiceover::Author,
        "vo" | "ло" => Voiceover::OneVoice,
        _ => return None,
    };

    Some(kind)
}

/// rutor's one-letter codes: only after a `|`, where they cannot be a word.
fn tail_code(word: &str) -> Option<Voiceover> {
    let kind = match word {
        "d" => Voiceover::Dubbing,
        "p" | "l" => Voiceover::Polyphonic,
        "p2" | "l2" => Voiceover::TwoVoice,
        "a" => Voiceover::Author,
        "p1" | "l1" => Voiceover::OneVoice,
        _ => return None,
    };

    Some(kind)
}

fn spelled(word: &str) -> Option<Voiceover> {
    let kind = if word.starts_with("дубляж") || word.starts_with("дублирован") {
        Voiceover::Dubbing
    } else if word.starts_with("многоголос") {
        Voiceover::Polyphonic
    } else if word.starts_with("двухголос") || word.starts_with("двуголос") {
        Voiceover::TwoVoice
    } else if word.starts_with("авторск") {
        Voiceover::Author
    } else if word.starts_with("одноголос") {
        Voiceover::OneVoice
    } else {
        return None;
    };

    Some(kind)
}

fn is_subtitles(word: &str) -> bool {
    matches!(word, "sub" | "subs" | "ст") || word.starts_with("subtitle") || word.starts_with("субтитр")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audio(title: &str) -> AudioInfo {
        audio_lower(&title.to_lowercase())
    }

    #[test]
    fn rutracker_tracks() {
        let info = audio("Дюна [2021, BDRip] Dub (Пифагор) + 2x MVO + AVO (Сербин) + Original (Eng) + Sub (Rus, Eng)");

        let tracks = vec![(Voiceover::Dubbing, 1), (Voiceover::Polyphonic, 2), (Voiceover::Author, 1)];
        assert_eq!(info.tracks, tracks);
        assert!(info.original);
        assert!(info.subtitles);
    }

    #[test]
    fn kinozal_codes_and_their_spelled_repeat() {
        let info = audio("Дом дракона / 2024 / ДБ, 7 x ПМ, ЛД, СТ / WEB-DL (2160p) | Дубляж");

        let tracks = vec![(Voiceover::Dubbing, 1), (Voiceover::Polyphonic, 7), (Voiceover::TwoVoice, 1)];
        assert_eq!(info.tracks, tracks);
        assert!(info.subtitles);
        assert!(!info.original);
    }

    #[test]
    fn rutor_letters_after_a_bar() {
        let info = audio("Начало / Inception (2010) BDRip 1080p от NNNB | D, P2, A");

        let tracks = vec![(Voiceover::Dubbing, 1), (Voiceover::TwoVoice, 1), (Voiceover::Author, 1)];
        assert_eq!(info.tracks, tracks);
    }

    #[test]
    fn single_letters_before_a_bar_are_words() {
        let info = audio("A Quiet Place / Тихое место (2018) BDRip");

        assert!(info.tracks.is_empty());
    }

    #[test]
    fn repeated_codes_add_up() {
        let info = audio("Дюна [2021] MVO (Jaskier) + MVO (HDRezka Studio) + VO (Есарев)");

        let tracks = vec![(Voiceover::Polyphonic, 2), (Voiceover::OneVoice, 1)];
        assert_eq!(info.tracks, tracks);
    }

    #[test]
    fn a_studio_named_like_a_kind_is_not_a_track() {
        let info = audio("Одни из нас / The Last of Us (2023) WEB-DL 1080p | Dubbing-Pro");

        assert!(info.tracks.is_empty());
    }
}
