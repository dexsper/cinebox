//! Android TV entry point. Everything else is the shared `cinebox` app.
//!
//! `CineboxActivity` (a `NativeActivity`) loads this library and calls [`android_main`].

#![cfg(target_os = "android")]

mod device;
mod events;
mod jvm;
mod keys;
mod natives;

use std::sync::Arc;

use cinebox::platform::{Host, Profile};
use eframe::UserEvent;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use winit::event_loop::EventLoop;
use winit::platform::android::EventLoopBuilderExtAndroid;
use winit::platform::android::activity::AndroidApp;

use device::AndroidDevice;
use keys::KeyRouter;

#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    init_logging();

    let Some(root) = app.internal_data_path() else {
        tracing::error!("Android gave no internal data path; cannot store settings");
        return;
    };
    cinebox_core::paths::set_android_root(root);

    let java = jvm::init(&app);
    let device = AndroidDevice::new(app.clone(), java);
    if let Err(error) = run(app, device) {
        tracing::error!(%error, "running eframe application");
    }

    // winit allows one event loop per process, and Android may start the
    // activity again in this same process: only a fresh process starts clean.
    std::process::exit(0);
}

fn run(app: AndroidApp, device: AndroidDevice) -> eframe::Result {
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .with_android_app(app)
        .build()?;

    let host = Host {
        profile: Profile::tv(),
        device: Arc::new(device),
    };
    let options = cinebox::native_options();
    let creator = cinebox::app_creator(host);
    let eframe_app = eframe::create_native(&cinebox::app_name(), options, creator, &event_loop);

    event_loop.run_app(&mut KeyRouter::new(eframe_app))?;
    Ok(())
}

fn init_logging() {
    let filter = EnvFilter::new("cinebox=info,cinebox_android=info");
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(paranoid_android::layer("cinebox"))
        .try_init();
}
