//! Keeps the Linux desktop from blanking the screen during playback.
//!
//! libmpv cannot do it here: its `stop-screensaver` only reaches video outputs
//! that own a window, and `vo=libmpv` has none, so the request is dropped. The
//! desktop is asked over D-Bus instead, on a thread of its own so that a slow
//! or missing bus never stalls a frame.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use zbus::{Connection, OwnedObjectPath, Variant};

const APP_NAME: &str = "Cinebox";
const REASON: &str = "Playing a video";

/// `org.freedesktop.portal.Inhibit` flag for "the session is idle".
const PORTAL_IDLE: u32 = 8;

#[zbus::proxy(
    interface = "org.freedesktop.portal.Inhibit",
    default_service = "org.freedesktop.portal.Desktop",
    default_path = "/org/freedesktop/portal/desktop"
)]
trait PortalInhibit {
    fn inhibit(&self, window: &str, flags: u32, options: HashMap<&str, Variant<'_>>) -> zbus::Result<OwnedObjectPath>;
}

#[zbus::proxy(interface = "org.freedesktop.portal.Request", default_service = "org.freedesktop.portal.Desktop")]
trait PortalRequest {
    fn close(&self) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.freedesktop.ScreenSaver",
    default_service = "org.freedesktop.ScreenSaver",
    default_path = "/org/freedesktop/ScreenSaver"
)]
trait ScreenSaver {
    fn inhibit(&self, application_name: &str, reason_for_inhibit: &str) -> zbus::Result<u32>;

    #[zbus(name = "UnInhibit")]
    fn un_inhibit(&self, cookie: u32) -> zbus::Result<()>;
}

/// The handle `Inhibit` returned, needed to lift it again.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Hold {
    /// The portal's request object; closing it lifts the inhibition. Works in
    /// Flatpak and on GNOME, which has no `org.freedesktop.ScreenSaver`.
    Portal(OwnedObjectPath),
    /// KDE, Xfce and others outside a sandbox.
    ScreenSaver(u32),
}

trait Inhibitor {
    /// `None` when no service would take the request.
    fn inhibit(&mut self) -> Option<Hold>;

    fn release(&mut self, hold: Hold);
}

/// Requests go to a worker thread, which owns the bus connection.
#[derive(Debug)]
pub(crate) struct IdleInhibit {
    requests: Sender<bool>,
}

impl IdleInhibit {
    #[must_use]
    pub(crate) fn spawn() -> Self {
        let (requests, received) = mpsc::channel();
        let worker = thread::Builder::new().name(String::from("idle-inhibit")).spawn(move || {
            serve(&received, &mut Bus::default());
        });

        // Without the worker every request lands in a closed channel: the
        // screen blanks as before, but the app keeps working.
        if let Err(error) = worker {
            tracing::warn!(%error, "cannot start the idle inhibit thread");
        }

        Self { requests }
    }

    pub(crate) fn set(&self, on: bool) {
        // Fails only once the worker is gone, which `spawn` already logged.
        if self.requests.send(on).is_err() {
            tracing::debug!(on, "idle inhibit worker is not running");
        }
    }
}

/// Runs until every sender is dropped. The process may exit while an
/// inhibition is held: the bus lifts it once the connection closes.
fn serve(requests: &Receiver<bool>, inhibitor: &mut impl Inhibitor) {
    let mut held: Option<Hold> = None;

    while let Ok(first) = requests.recv() {
        // Only the latest request matters; leaving and opening the player in
        // one go must not drop and retake the inhibition.
        let wanted = requests.try_iter().last().unwrap_or(first);
        settle(&mut held, wanted, inhibitor);
    }
}

/// A refused inhibit leaves `held` empty, so the next `true` tries again.
fn settle(held: &mut Option<Hold>, wanted: bool, inhibitor: &mut impl Inhibitor) {
    if wanted == held.is_some() {
        return;
    }

    if wanted {
        *held = inhibitor.inhibit();
        return;
    }

    if let Some(hold) = held.take() {
        inhibitor.release(hold);
    }
}

/// The connection lives as long as the worker: the bus lifts an inhibition
/// once the connection that took it closes. Between calls, `zbus::block_on`
/// hands the connection to a helper thread of its own.
#[derive(Default)]
struct Bus {
    connection: Option<Connection>,
}

impl Bus {
    async fn connection(&mut self) -> Option<&Connection> {
        if self.connection.is_none() {
            match Connection::session().await {
                Ok(connection) => self.connection = Some(connection),
                Err(error) => tracing::warn!(%error, "no D-Bus session bus; the screen may blank during playback"),
            }
        }

        self.connection.as_ref()
    }

    async fn inhibit_any(&mut self) -> Option<Hold> {
        let connection = self.connection().await?;

        let portal_error = match portal_inhibit(connection).await {
            Ok(handle) => return Some(Hold::Portal(handle)),
            Err(error) => error,
        };

        match screensaver_inhibit(connection).await {
            Ok(cookie) => Some(Hold::ScreenSaver(cookie)),
            Err(error) => {
                tracing::warn!(%portal_error, screensaver_error = %error, "cannot keep the screen on during playback");
                None
            }
        }
    }

    async fn release_hold(&mut self, hold: &Hold) -> zbus::Result<()> {
        let Some(connection) = self.connection().await else { return Ok(()) };

        match hold {
            Hold::Portal(handle) => portal_release(connection, handle).await,
            Hold::ScreenSaver(cookie) => screensaver_release(connection, *cookie).await,
        }
    }
}

impl Inhibitor for Bus {
    fn inhibit(&mut self) -> Option<Hold> {
        zbus::block_on(self.inhibit_any())
    }

    fn release(&mut self, hold: Hold) {
        if let Err(error) = zbus::block_on(self.release_hold(&hold)) {
            tracing::warn!(%error, ?hold, "cannot lift the idle inhibition");
        }
    }
}

async fn portal_inhibit(connection: &Connection) -> zbus::Result<OwnedObjectPath> {
    let portal = PortalInhibitProxy::new(connection).await?;
    let options = HashMap::from([("reason", Variant::from(REASON))]);

    portal.inhibit("", PORTAL_IDLE, options).await
}

async fn portal_release(connection: &Connection, handle: &OwnedObjectPath) -> zbus::Result<()> {
    let request = PortalRequestProxy::builder(connection).path(handle.as_ref()).build().await?;
    request.close().await
}

async fn screensaver_inhibit(connection: &Connection) -> zbus::Result<u32> {
    let screensaver = ScreenSaverProxy::new(connection).await?;
    screensaver.inhibit(APP_NAME, REASON).await
}

async fn screensaver_release(connection: &Connection, cookie: u32) -> zbus::Result<()> {
    let screensaver = ScreenSaverProxy::new(connection).await?;
    screensaver.un_inhibit(cookie).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Call {
        Inhibit,
        Release(Hold),
    }

    #[derive(Default)]
    struct Recorder {
        calls: Vec<Call>,
        is_refused: bool,
        next_cookie: u32,
    }

    impl Inhibitor for Recorder {
        fn inhibit(&mut self) -> Option<Hold> {
            self.calls.push(Call::Inhibit);

            if self.is_refused {
                return None;
            }

            self.next_cookie += 1;
            Some(Hold::ScreenSaver(self.next_cookie))
        }

        fn release(&mut self, hold: Hold) {
            self.calls.push(Call::Release(hold));
        }
    }

    fn served(requests: &[bool], mut recorder: Recorder) -> Vec<Call> {
        let (sender, received) = mpsc::channel();
        for &request in requests {
            sender.send(request).unwrap();
        }
        drop(sender);

        serve(&received, &mut recorder);
        recorder.calls
    }

    fn settled(requests: &[bool], mut recorder: Recorder) -> Vec<Call> {
        let mut held = None;
        for &request in requests {
            settle(&mut held, request, &mut recorder);
        }

        recorder.calls
    }

    #[test]
    fn playback_inhibits_once_and_releases_what_it_got() {
        let calls = settled(&[true, true, false, false], Recorder::default());

        assert_eq!(calls, vec![Call::Inhibit, Call::Release(Hold::ScreenSaver(1))]);
    }

    #[test]
    fn stopping_without_an_inhibition_releases_nothing() {
        let calls = settled(&[false], Recorder::default());

        assert!(calls.is_empty());
    }

    #[test]
    fn a_burst_settles_on_its_last_request() {
        let calls = served(&[true, false, true], Recorder::default());

        assert_eq!(calls, vec![Call::Inhibit]);
    }

    #[test]
    fn a_burst_ending_off_never_inhibits() {
        let calls = served(&[true, false], Recorder::default());

        assert!(calls.is_empty());
    }

    #[test]
    fn a_refused_inhibit_is_retried_on_the_next_playback() {
        let recorder = Recorder { is_refused: true, ..Recorder::default() };

        let calls = settled(&[true, false, true], recorder);

        assert_eq!(calls, vec![Call::Inhibit, Call::Inhibit]);
    }
}
