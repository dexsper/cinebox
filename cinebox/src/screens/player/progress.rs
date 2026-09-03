//! Watch progress and playback-prefs persistence, off the UI thread.

use std::time::{Duration, Instant};

use cinebox_core::WatchHistoryEntry;
use tracing::warn;

use crate::services::Services;

use super::{PlayerPhase, PlayerScreen};

impl PlayerScreen {
    pub(super) fn save_prefs(&self, svc: &Services) {
        let Some(db) = svc.db.clone() else {
            return;
        };

        let hash = match &self.phase {
            Some(PlayerPhase::Playing(state)) => state.hash.clone(),
            Some(PlayerPhase::Buffering(state)) => state.hash.clone(),
            None => return,
        };

        let prefs = self.prefs;
        egui_async::bind::ASYNC_RUNTIME.spawn(async move {
            if let Err(error) = db.put_torrent_prefs(&hash, &prefs).await {
                warn!(%error, "failed to save torrent playback prefs");
            }
        });
    }

    /// Persists progress off the UI thread; the returned handle is only
    /// awaited by tests.
    pub(super) fn save_progress(
        &mut self,
        svc: &mut Services,
        force: bool,
    ) -> Option<tokio::task::JoinHandle<()>> {
        if !force {
            let recent = self
                .progress_saved_at
                .is_some_and(|at| at.elapsed() < Duration::from_secs(10));

            if recent {
                return None;
            }
        }

        let (entry, hash, time, duration, kind, id, file_id) = {
            let Some(PlayerPhase::Playing(state)) = &mut self.phase else {
                return None;
            };

            if state.error.is_some() {
                return None;
            }

            let file = state.files.get(state.file_index)?;

            let season = file.season;
            let episode = file.episode;
            let episode_title = file.title.clone();
            let file_id = file.id;
            let entry = WatchHistoryEntry {
                kind: state.card.kind,
                id: state.card.id,
                title: state.card.title.clone(),
                poster_path: state.card.poster_path.clone(),
                year: state.card.year,
                vote: state.card.vote,
                season,
                episode,
                episode_title: Some(episode_title),
                time: state.time,
                duration: state.duration,
            };
            let hash = state.hash.clone();
            let time = state.time;
            let duration = state.duration;
            let kind = state.card.kind;
            let id = state.card.id;

            if let Some(file) = state.files.get_mut(state.file_index) {
                file.timecode = time;
            }

            (entry, hash, time, duration, kind, id, file_id)
        };

        let track = svc.settings.torrserver.track_timecode;

        let db_job = svc.db.clone().map(|db| {
            let history_hash = hash.clone();

            egui_async::bind::ASYNC_RUNTIME.spawn(async move {
                let timeline = db.upsert_watch_timeline(
                    kind,
                    id,
                    entry.season,
                    entry.episode,
                    time,
                    duration,
                );

                if let Err(error) = timeline.await {
                    warn!(%error, "failed to save watch timeline");
                }

                if let Err(error) = db.upsert_watch_history(&entry, Some(&history_hash)).await {
                    warn!(%error, "failed to save watch history");
                }
            })
        });

        svc.mark_watched(kind, id);
        self.progress_saved_at = Some(Instant::now());

        if !track {
            return db_job;
        }

        let settings = svc.settings.clone();
        self.viewed_job.request(async move {
            cinebox_torrserver::viewed_set(
                &settings.torrserver.url,
                &settings.torrserver.username,
                settings.torrserver.password.expose(),
                &hash,
                file_id,
                time,
            )
            .await
            .map_err(|error| error.to_string())
        });

        db_job
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::spec;
    use super::super::{PlayerPhase, PlayerScreen, PlayerState};
    use crate::services::{Services, db_block_on};

    #[test]
    fn save_progress_upserts_by_media_id() -> Result<(), cinebox_core::StoreError> {
        let db = std::sync::Arc::new(db_block_on(cinebox_core::Store::memory())?);
        let mut svc = Services::test_with_db(db.clone());
        let mut screen = PlayerScreen {
            phase: Some(PlayerPhase::Playing(PlayerState::from_spec(&spec(1, 3)))),
            ..PlayerScreen::default()
        };

        if let Some(PlayerPhase::Playing(state)) = &mut screen.phase {
            state.time = 42.0;
            state.duration = 2400.0;
        }

        let db_job = screen.save_progress(&mut svc, true);
        if let Some(job) = db_job {
            assert!(db_block_on(job).is_ok());
        }

        let kind = cinebox_core::MediaKind::Tv;
        let id = cinebox_core::TmdbId::new(1);
        let got = db_block_on(db.get_watch_timeline(kind, id, Some(1), Some(2)))?;

        assert_eq!(got, Some((42.0, 2400.0)));

        let recent = db_block_on(db.recently_watched(10))?;
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].id, id);
        assert_eq!(recent[0].title, "Show");
        assert!(svc.is_watched(kind, id));

        Ok(())
    }
}
