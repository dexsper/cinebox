//! What the Linux desktop does for the app.

use super::Device;
use super::idle_inhibit::IdleInhibit;

#[derive(Debug)]
pub(crate) struct LinuxDevice {
    idle: IdleInhibit,
}

impl LinuxDevice {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self { idle: IdleInhibit::spawn() }
    }
}

impl Device for LinuxDevice {
    fn keep_screen_on(&self, on: bool) {
        self.idle.set(on);
    }
}
