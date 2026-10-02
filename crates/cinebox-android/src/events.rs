//! Device events raised outside the UI thread (Java callbacks, the window's
//! key router) and handed to the app on its next frame.

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

use cinebox::platform::DeviceEvent;
use eframe::egui;

static QUEUE: Mutex<VecDeque<DeviceEvent>> = Mutex::new(VecDeque::new());
static UI: OnceLock<egui::Context> = OnceLock::new();

pub fn attach(ctx: &egui::Context) {
    let _ = UI.set(ctx.clone());
}

pub fn push(event: DeviceEvent) {
    let Ok(mut queue) = QUEUE.lock() else {
        return;
    };

    queue.push_back(event);
    drop(queue);

    if let Some(ctx) = UI.get() {
        ctx.request_repaint();
    }
}

pub fn pop() -> Option<DeviceEvent> {
    QUEUE.lock().ok()?.pop_front()
}
