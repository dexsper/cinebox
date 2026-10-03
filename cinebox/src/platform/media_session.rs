//! Keeps the OS media session in step with the player without calling into
//! the OS every frame.

use super::{Device, MediaSessionState};

/// The OS extrapolates the position of a playing session itself; a jump
/// beyond this (a seek, a stall) is worth telling it about.
const DRIFT_SECS: f64 = 2.0;

#[derive(Default)]
pub(crate) struct SessionPublisher {
    last: Option<Published>,
}

struct Published {
    state: MediaSessionState,
    /// `ctx` time of the publish, in seconds.
    at: f64,
}

impl Published {
    fn expected_position(&self, now: f64) -> f64 {
        if !self.state.playing {
            return self.state.position;
        }

        self.state.position + (now - self.at)
    }
}

impl SessionPublisher {
    pub(crate) fn publish(&mut self, device: &dyn Device, state: MediaSessionState, now: f64) {
        if !self.differs(&state, now) {
            return;
        }

        device.update_media_session(Some(&state));
        self.last = Some(Published { state, at: now });
    }

    pub(crate) fn clear(&mut self, device: &dyn Device) {
        if self.last.take().is_some() {
            device.update_media_session(None);
        }
    }

    fn differs(&self, state: &MediaSessionState, now: f64) -> bool {
        let Some(last) = &self.last else {
            return true;
        };

        if !same_apart_from_position(&last.state, state) {
            return true;
        }

        (state.position - last.expected_position(now)).abs() > DRIFT_SECS
    }
}

fn same_apart_from_position(a: &MediaSessionState, b: &MediaSessionState) -> bool {
    let b_at_a = MediaSessionState {
        position: a.position,
        ..b.clone()
    };

    *a == b_at_a
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct Recorder {
        updates: Mutex<Vec<Option<MediaSessionState>>>,
    }

    impl Device for Recorder {
        fn update_media_session(&self, state: Option<&MediaSessionState>) {
            let Ok(mut updates) = self.updates.lock() else {
                return;
            };

            updates.push(state.cloned());
        }
    }

    impl Recorder {
        fn count(&self) -> usize {
            self.updates.lock().map(|updates| updates.len()).unwrap_or(0)
        }
    }

    fn playing_at(position: f64) -> MediaSessionState {
        MediaSessionState {
            title: String::from("Film"),
            subtitle: None,
            duration: 600.0,
            artwork: None,
            playing: true,
            position,
            can_next: false,
            can_previous: false,
        }
    }

    #[test]
    fn steady_playback_is_published_once() {
        let device = Recorder::default();
        let mut publisher = SessionPublisher::default();

        publisher.publish(&device, playing_at(10.0), 0.0);
        publisher.publish(&device, playing_at(10.25), 0.25);
        publisher.publish(&device, playing_at(11.0), 1.0);

        assert_eq!(device.count(), 1);
    }

    #[test]
    fn pause_and_seek_are_published() {
        let device = Recorder::default();
        let mut publisher = SessionPublisher::default();

        publisher.publish(&device, playing_at(10.0), 0.0);
        let paused = MediaSessionState {
            playing: false,
            ..playing_at(11.0)
        };
        publisher.publish(&device, paused, 1.0);
        publisher.publish(&device, playing_at(300.0), 2.0);

        assert_eq!(device.count(), 3);
    }

    #[test]
    fn a_paused_session_does_not_drift() {
        let device = Recorder::default();
        let mut publisher = SessionPublisher::default();
        let paused = MediaSessionState {
            playing: false,
            ..playing_at(42.0)
        };

        publisher.publish(&device, paused.clone(), 0.0);
        publisher.publish(&device, paused, 30.0);

        assert_eq!(device.count(), 1);
    }

    #[test]
    fn clear_reports_only_a_published_session() {
        let device = Recorder::default();
        let mut publisher = SessionPublisher::default();

        publisher.clear(&device);
        publisher.publish(&device, playing_at(0.0), 0.0);
        publisher.clear(&device);
        publisher.clear(&device);

        let Ok(updates) = device.updates.lock() else {
            panic!("recorder lock");
        };
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[1], None);
    }
}
