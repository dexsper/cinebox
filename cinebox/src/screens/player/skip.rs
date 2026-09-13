//! Per-session skip-segment state for the player.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use cinebox_core::{MediaKind, Settings, Store, TmdbId};
use cinebox_skip::{MediaSegments, SegmentType, TimeRange};
use egui_async::Bind;

use crate::jobs::JobError;

pub const AUTOSKIP_SECS: f64 = 8.0;

pub struct ActiveSegment {
    pub ty: SegmentType,
    pub start_ms: u64,
    pub end_ms: u64,
    pub dismissed: bool,
    pub countdown_started_at: Option<f64>,
}

impl ActiveSegment {
    #[must_use]
    pub fn countdown_frac(&self, now: f64) -> f32 {
        let Some(started_at) = self.countdown_started_at else {
            return 0.0;
        };

        ((now - started_at) / AUTOSKIP_SECS).clamp(0.0, 1.0) as f32
    }

    #[must_use]
    pub fn countdown_done(&self, now: f64) -> bool {
        self.countdown_started_at
            .is_some_and(|t| now - t >= AUTOSKIP_SECS)
    }
}

pub struct SkipState {
    requested_for: Option<(MediaKind, TmdbId, Option<u32>, Option<u32>)>,
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
            segments: Bind::new(true),
            choices: Bind::new(true),
            armed: HashMap::new(),
            skipped_instances: HashSet::new(),
            active: None,
        }
    }
}

impl SkipState {
    pub fn reset(&mut self) {
        self.requested_for = None;
        self.segments.abort();
        self.segments.clear();
        self.choices.abort();
        self.choices.clear();
        self.armed.clear();
        self.skipped_instances.clear();
        self.active = None;
    }

    /// Called from `tick()`. Returns `Some(seek_target_secs)` when an
    /// auto-skip fires this frame.
    pub fn tick(
        &mut self,
        now: f64,
        time_secs: f64,
        duration_secs: f64,
        is_paused: bool,
        kind: MediaKind,
        tmdb_id: TmdbId,
        season: Option<u32>,
        episode: Option<u32>,
        settings: &Settings,
        db: &Option<Arc<Store>>,
    ) -> Option<f64> {
        self.maybe_start_fetch(duration_secs, kind, tmdb_id, season, episode, settings, db);
        self.poll_results();

        if is_paused {
            return None;
        }

        self.update_active(time_secs, duration_secs, settings);
        self.tick_countdown(now, duration_secs)
    }

    fn maybe_start_fetch(
        &mut self,
        duration_secs: f64,
        kind: MediaKind,
        tmdb_id: TmdbId,
        season: Option<u32>,
        episode: Option<u32>,
        settings: &Settings,
        db: &Option<Arc<Store>>,
    ) {
        if duration_secs <= 0.0 {
            return;
        }

        let key = (kind, tmdb_id, season, episode);

        if self.requested_for.as_ref() == Some(&key) {
            return;
        }

        self.requested_for = Some(key);

        let net = crate::jobs::net_config(settings);
        let duration_ms = (duration_secs * 1000.0).round() as u64;
        let query = cinebox_skip::SegmentQuery {
            tmdb_id: u64::from(tmdb_id.get()),
            kind,
            season,
            episode,
            duration_ms,
        };

        self.segments
            .request(crate::jobs::fetch_skip_segments(net, db.clone(), query));

        if let Some(db) = db.clone() {
            self.choices
                .request(crate::jobs::fetch_skip_choices(db, kind, tmdb_id));
        }
    }

    fn poll_results(&mut self) {
        let _ = self.segments.read();

        if let Some(Ok(choices)) = self.choices.take() {
            self.armed = choices;
        }
    }

    fn update_active(&mut self, time_secs: f64, duration_secs: f64, settings: &Settings) {
        let segments: MediaSegments = match self.segments.read() {
            Some(Ok(Some(s))) => s.clone(),
            _ => {
                self.active = None;
                return;
            }
        };

        let pos_ms = (time_secs * 1000.0).round() as u64;
        let dur_ms = (duration_secs * 1000.0).round() as u64;

        let found = self.find_active_segment(&segments, pos_ms, dur_ms, settings);

        match found {
            None => {
                self.active = None;
            }
            Some((ty, range)) => {
                let start_ms = range.start_or_zero();
                let end_ms = range.end_or_duration(dur_ms);

                let same = self
                    .active
                    .as_ref()
                    .is_some_and(|a| a.ty == ty && a.start_ms == start_ms && a.end_ms == end_ms);

                if !same {
                    self.active = Some(ActiveSegment {
                        ty,
                        start_ms,
                        end_ms,
                        dismissed: false,
                        countdown_started_at: None,
                    });
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
            if !self.type_enabled(ty, settings) {
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

    fn type_enabled(&self, ty: SegmentType, settings: &Settings) -> bool {
        let s = &settings.player.skip_segments;
        match ty {
            SegmentType::Intro => s.intro,
            SegmentType::Recap => s.recap,
            SegmentType::Credits => s.credits,
            SegmentType::Preview => s.preview,
        }
    }

    fn tick_countdown(&mut self, now: f64, duration_secs: f64) -> Option<f64> {
        let active = self.active.as_mut()?;

        if active.dismissed {
            return None;
        }

        let is_armed = self.armed.get(&active.ty).copied().unwrap_or(false);

        if !is_armed {
            return None;
        }

        if active.countdown_started_at.is_none() {
            active.countdown_started_at = Some(now);
        }

        if !active.countdown_done(now) {
            return None;
        }

        let end_ms = active.end_ms;
        let ty = active.ty;
        let start_ms = active.start_ms;

        self.skipped_instances.insert((ty, start_ms, end_ms));
        self.active = None;

        let target_secs = if end_ms == 0 {
            duration_secs
        } else {
            end_ms as f64 / 1000.0
        };

        Some(target_secs)
    }

    /// User pressed Skip.
    ///
    /// Returns `(seek_target_secs, segment_type)` so the caller can seek and
    /// persist `armed = true` for this type.
    pub fn on_skip(&mut self, duration_secs: f64) -> Option<(f64, SegmentType)> {
        let active = self.active.take()?;
        let ty = active.ty;
        let end_ms = active.end_ms;

        self.skipped_instances.insert((ty, active.start_ms, end_ms));
        self.armed.insert(ty, true);

        let target_secs = if end_ms == 0 {
            duration_secs
        } else {
            end_ms as f64 / 1000.0
        };

        Some((target_secs, ty))
    }

    /// User pressed Cancel.
    ///
    /// Always dismisses the current banner for this playback instance.
    /// Returns `Some(ty)` only when the auto-skip countdown was already
    /// running — the caller should then persist `armed = false` to DB so the
    /// countdown won't fire next time.
    pub fn on_cancel(&mut self) -> Option<SegmentType> {
        let active = self.active.as_mut()?;
        let was_counting = active.countdown_started_at.is_some();
        let ty = active.ty;

        active.dismissed = true;
        active.countdown_started_at = None;

        if !was_counting {
            return None;
        }

        self.armed.insert(ty, false);

        Some(ty)
    }
}
