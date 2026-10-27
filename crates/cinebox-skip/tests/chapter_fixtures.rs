//! Chapter lists of real releases against the segments they must give.
//!
//! `fixtures/chapters.json` holds every distinct chapter list found in the most
//! seeded releases of popular films and anime. Releases without chapters, or
//! with only unnamed ones, expect no segments.

use cinebox_skip::chapters::{ChapterMark, segments_from_chapters};
use cinebox_skip::{MediaSegments, SegmentType, TimeRange};
use serde::Deserialize;

const FIXTURES: &str = include_str!("fixtures/chapters.json");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Release {
    release: String,
    duration_ms: u64,
    chapters: Vec<(u64, String)>,
    #[serde(default)]
    intro: Vec<TimeRange>,
    #[serde(default)]
    recap: Vec<TimeRange>,
    #[serde(default)]
    credits: Vec<TimeRange>,
    #[serde(default)]
    preview: Vec<TimeRange>,
}

impl Release {
    fn marks(&self) -> Vec<ChapterMark<'_>> {
        let marks = self.chapters.iter().map(|(start_ms, title)| ChapterMark {
            start_ms: *start_ms,
            title,
        });

        marks.collect()
    }

    fn expected(&self, ty: SegmentType) -> &[TimeRange] {
        match ty {
            SegmentType::Intro => &self.intro,
            SegmentType::Recap => &self.recap,
            SegmentType::Credits => &self.credits,
            SegmentType::Preview => &self.preview,
        }
    }
}

#[test]
fn real_releases_give_the_labelled_segments() -> Result<(), serde_json::Error> {
    let releases: Vec<Release> = serde_json::from_str(FIXTURES)?;
    let mut failures = Vec::new();

    for release in &releases {
        let marks = release.marks();
        let segments = segments_from_chapters(&marks, release.duration_ms).unwrap_or_default();

        for ty in SegmentType::ALL {
            let expected = release.expected(ty);
            let actual = MediaSegments::segments_of(&segments, ty);

            if actual != expected {
                let name = &release.release;
                failures.push(format!(
                    "{name}: {ty:?} expected {expected:?}, got {actual:?}"
                ));
            }
        }
    }

    assert!(!releases.is_empty());
    assert!(failures.is_empty(), "{}", failures.join("\n"));

    Ok(())
}
