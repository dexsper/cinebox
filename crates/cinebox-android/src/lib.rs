//! Android TV entry point. Everything else is the shared `cinebox` app.
//!
//! NativeActivity loads this library and calls [`android_main`]; it may do so
//! more than once per process (the activity is recreated, the process stays).

#![cfg(target_os = "android")]

mod jvm;

use std::sync::Arc;

use eframe::winit::platform::android::activity::{AndroidApp, WindowManagerFlags};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    init_logging();

    let Some(root) = app.internal_data_path() else {
        tracing::error!("Android gave no internal data path; cannot store settings");
        return;
    };
    cinebox_core::paths::set_android_root(root);

    jvm::init(&app);

    let host = cinebox::Host::tv(keep_awake(app.clone()));
    let mut options = cinebox::native_options();
    options.android_app = Some(app);

    if let Err(error) = cinebox::run_with(options, host) {
        tracing::error!(%error, "running eframe application");
    }
}

/// The TV screensaver would otherwise start in the middle of a film.
fn keep_awake(app: AndroidApp) -> cinebox::KeepAwake {
    Arc::new(move |on| {
        let flags = WindowManagerFlags::KEEP_SCREEN_ON;
        if on {
            app.set_window_flags(flags, WindowManagerFlags::empty());
            return;
        }

        app.set_window_flags(WindowManagerFlags::empty(), flags);
    })
}

fn init_logging() {
    let filter = EnvFilter::new("cinebox=info,cinebox_android=info");
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(paranoid_android::layer("cinebox"))
        .try_init();
}
