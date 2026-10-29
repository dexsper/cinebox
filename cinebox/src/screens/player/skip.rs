//! Per-session skip-segment state for the player.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use cinebox_core::{MediaKind, Settings, Store, TmdbId};
use cinebox_player::{Chapter, Player};
use cinebox_skip::chapters::{ChapterMark, segments_from_chapters};
use cinebox_skip::{MediaSegments, SegmentType, TimeRange};
use egui_async::Bind;
use tracing::warn;

use crate::services::Services;
use crate::services::jobs::{self, JobError};
use crate::theme;

pub(super) const AUTOSKIP_SECS: f64 = 8.0;
const MILLIS_PER_SEC: f64 = 1000.0;

pub(super) struct ActiveSegment {
    pub ty: SegmentType,
    pub start_ms: u64,
    pub end_ms: u64,
    pub is_dismissed: bool,
    pub countdown_started_at: Option<f64>,
}

impl ActiveSegment {
    fn is_same_span(&self, other: &Self) -> bool {
        self.ty == other.ty && self.start_ms == other.start_ms && self.end_ms == other.end_ms
    }

    #[must_use]
    pub(super) fn countdown_frac(&self, now: f64) -> f32 {
        let Some(started_at) = self.countdown_started_at else {
            return 0.0;
        };

        theme::fraction((now - started_at) / AUTOSKIP_SECS)
    }

    #[must_use]
    fn is_countdown_done(&self, now: f64) -> bool {
        self.countdown_started_at.is_some_and(|started_at| now - started_at >= AUTOSKIP_SECS)
    }
}

/// The film or episode whose segments are skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SkipTarget {
    pub kind: MediaKind,
    pub tmdb_id: TmdbId,
    pub season: Option<u32>,
    pub episode: Option<u32>,
}

/// Where playback stands this frame.
#[derive(Debug, Clone, Copy)]
pub(super) struct Playhead {
    pub time_secs: f64,
    /// `0` until the player knows it.
    pub duration_secs: f64,
    pub is_paused: bool,
}

pub(super) struct SkipState {
    requested_for: Option<SkipTarget>,
    /// From the file's own chapter marks; preferred over `segments` per kind.
    chapter_segments: Option<MediaSegments>,
    pub segments: Bind<Option<MediaSegments>, JobError>,
    pub choices: Bind<HashMap<SegmentType, bool>, JobError>,
    pub armed: HashMap<SegmentType, bool>,
    pub skipped_instances: HashSet<(SegmentType, u64, u64)>,
    pub active: Option<ActiveSegment>,
}

impl Default for SkipState {
    fn default() -> Self {
        Self {
            requested_for: None,
            chapter_segments: None,
            segments: Bind::new(true),
            choices: Bind::new(true),
            armed: HashMap::new(),
            skipped_instances: HashSet::new(),
            active: None,
        }
    }
}

impl SkipState {
    pub(super) fn reset(&mut self) {
        self.requested_for = None;
        self.chapter_segments = None;
        self.segments.abort();
        self.segments.clear();
        self.choices.abort();
        self.choices.clear();
        self.armed.clear();
        self.skipped_instances.clear();
        self.active = None;
    }

    /// Called every frame of playback. The time to seek to, in seconds, when an
    /// auto-skip fires this frame.
    pub(super) fn tick(&mut self, now: f64, playhead: Playhead, target: SkipTarget, svc: &Services) -> Option<f64> {
        self.maybe_start_fetch(playhead.duration_secs, target, svc);
        self.poll_results();

        if playhead.is_paused {
            return None;
        }

        self.update_active(playhead.time_secs, playhead.duration_secs, &svc.settings);
        self.tick_countdown(now, playhead.duration_secs)
    }

    fn maybe_start_fetch(&mut self, duration_secs: f64, target: SkipTarget, svc: &Services) {
        if duration_secs <= 0.0 {
            return;
        }

        if self.requested_for == Some(target) {
            return;
        }

        self.requested_for = Some(target);
        let duration_ms = secs_to_ms(duration_secs);

        self.chapter_segments = svc.player.as_deref().and_then(|player| read_chapter_segments(player, duration_ms));

        self.request_segments(duration_ms, target, &svc.settings, svc.db.as_ref());
    }

    fn request_segments(&mut self, duration_ms: u64, target: SkipTarget, settings: &Settings, db: Option<&Arc<Store>>) {
        let SkipTarget { kind, tmdb_id, season, episode } = target;

        let net = jobs::net_config(settings);
        let cache_id = jobs::skip_cache_id(tmdb_id, kind, season, episode, duration_ms);
        let provider_id = u64::from(tmdb_id.get());
        let query = cinebox_skip::SegmentQuery { tmdb_id: provider_id, kind, season, episode, duration_ms };

        self.segments.request(jobs::fetch_skip_segments(net, db.cloned(), query, cache_id));

        if let Some(store) = db {
            self.choices.request(jobs::fetch_skip_choices(Arc::clone(store), kind, tmdb_id));
        }
    }

    fn poll_results(&mut self) {
        self.segments.poll();

        match self.choices.take() {
            Some(Ok(choices)) => self.armed = choices,
            // Without the saved choices nothing is armed, so the banner still shows but never skips by itself.
            Some(Err(error)) => warn!(%error, "failed to load skip choices"),
            None => {}
        }
    }

    /// Chapter marks first, the provider for the kinds the file does not mark.
    fn current_segments(&mut self) -> Option<MediaSegments> {
        let fetched = match self.segments.read() {
            Some(Ok(Some(fetched))) => Some(fetched),
            _ => None,
        };

        let Some(mut segments) = self.chapter_segments.clone() else {
            return fetched.cloned();
        };

        if let Some(fetched) = fetched {
            segments.fill_missing_from(fetched);
        }

        Some(segments)
    }

    fn update_active(&mut self, time_secs: f64, duration_secs: f64, settings: &Settings) {
        let Some(segments) = self.current_segments() else {
            self.active = None;
            return;
        };

        let pos_ms = secs_to_ms(time_secs);
        let dur_ms = secs_to_ms(duration_secs);

        let found = self.find_active_segment(&segments, pos_ms, dur_ms, settings);

        match found {
            None => {
                self.active = None;
            }
            Some((ty, range)) => {
                let start_ms = range.start_or_zero();
                let end_ms = range.end_or_duration(dur_ms);

                let candidate = ActiveSegment { ty, start_ms, end_ms, is_dismissed: false, countdown_started_at: None };
                let is_same = self.active.as_ref().is_some_and(|active| active.is_same_span(&candidate));

                if !is_same {
                    self.active = Some(candidate);
                }
            }
        }
    }

    fn find_active_segment(
        &self,
        segments: &MediaSegments,
        pos_ms: u64,
        dur_ms: u64,
        settings: &Settings,
    ) -> Option<(SegmentType, TimeRange)> {
        for ty in SegmentType::ALL {
            if !is_type_enabled(ty, settings) {
                continue;
            }

            for range in segments.segments_of(ty) {
                if !range.contains(pos_ms, dur_ms) {
                    continue;
                }

                let start_ms = range.start_or_zero();
                let end_ms = range.end_or_duration(dur_ms);

                if self.skipped_instances.contains(&(ty, start_ms, end_ms)) {
                    continue;
                }

                return Some((ty, range.clone()));
            }
        }

        None
    }

    fn tick_countdown(&mut self, now: f64, duration_secs: f64) -> Option<f64> {
        let active = self.active.as_mut()?;

        if active.is_dismissed {
            return None;
        }

        let is_armed = self.armed.get(&active.ty).copied().unwrap_or(false);

        if !is_armed {
            return None;
        }

        if active.countdown_started_at.is_none() {
            active.countdown_started_at = Some(now);
        }

        if !active.is_countdown_done(now) {
            return None;
        }

        let end_ms = active.end_ms;
        let ty = active.ty;
        let start_ms = active.start_ms;

        self.skipped_instances.insert((ty, start_ms, end_ms));
        self.active = None;

        Some(seek_target(end_ms, duration_secs))
    }

    /// User pressed Skip.
    ///
    /// The time to seek to in seconds and the segment's type, whose auto-skip
    /// the caller then saves as armed.
    pub(super) fn on_skip(&mut self, duration_secs: f64) -> Option<(f64, SegmentType)> {
        let active = self.active.take()?;
        let ty = active.ty;
        let end_ms = active.end_ms;

        self.skipped_instances.insert((ty, active.start_ms, end_ms));
        self.armed.insert(ty, true);

        Some((seek_target(end_ms, duration_secs), ty))
    }

    /// User pressed Cancel.
    ///
    /// Always dismisses the current banner for this playback instance.
    /// `Some` only when the auto-skip countdown was already running: the
    /// caller then saves the type as disarmed, so the countdown does not
    /// start next time.
    pub(super) fn on_cancel(&mut self) -> Option<SegmentType> {
        let active = self.active.as_mut()?;
        let was_counting = active.countdown_started_at.is_some();
        let ty = active.ty;

        active.is_dismissed = true;
        active.countdown_started_at = None;

        if !was_counting {
            return None;
        }

        self.armed.insert(ty, false);

        Some(ty)
    }
}

fn is_type_enabled(ty: SegmentType, settings: &Settings) -> bool {
    let skip = &settings.player.skip_segments;

    match ty {
        SegmentType::Intro => skip.intro,
        SegmentType::Recap => skip.recap,
        SegmentType::Credits => skip.credits,
        SegmentType::Preview => skip.preview,
    }
}

/// Where a skip lands: the segment's end, or the file's end when the segment runs to it.
fn seek_target(end_ms: u64, duration_secs: f64) -> f64 {
    if end_ms == 0 {
        return duration_secs;
    }

    ms_to_secs(end_ms)
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "`as` saturates, so a negative or NaN time becomes 0, which matches no segment end"
)]
fn secs_to_ms(secs: f64) -> u64 {
    (secs * MILLIS_PER_SEC).round() as u64
}

#[expect(clippy::cast_precision_loss, reason = "media times in milliseconds stay far below 2^53, where `f64` is exact")]
fn ms_to_secs(ms: u64) -> f64 {
    ms as f64 / MILLIS_PER_SEC
}

fn read_chapter_segments(player: &dyn Player, duration_ms: u64) -> Option<MediaSegments> {
    let chapters = player.chapters();
    let marks: Vec<ChapterMark<'_>> = chapters.iter().map(chapter_mark).collect();

    segments_from_chapters(&marks, duration_ms)
}

fn chapter_mark(chapter: &Chapter) -> ChapterMark<'_> {
    let start_ms = secs_to_ms(chapter.start.max(0.0));
    let title = chapter.title.as_deref().unwrap_or_default();

    ChapterMark { start_ms, title }
}
