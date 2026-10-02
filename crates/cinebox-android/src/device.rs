//! What the OS does for the app, through `CineboxActivity`'s Java methods.
//! Every call runs on the thread driving the app (`android_main`).

use std::sync::{Arc, Mutex};

use cinebox::platform::{
    Device, DeviceEvent, Direction, FieldText, MediaSessionState, SpeechRequest, TextAction,
    TextInputSpec, TextPurpose, UiSound,
};
use eframe::egui;
use jni::objects::{JByteArray, JObject, JValue};
use jni::sys::jint;
use jni::{Env, jni_sig, jni_str};
use winit::platform::android::activity::{AndroidApp, WindowManagerFlags};

use crate::events;
use crate::jvm::Java;

pub struct AndroidDevice {
    app: AndroidApp,
    java: Option<Java>,
    speech: bool,
    /// Last metadata sent; Java decodes the artwork again on every send.
    shown: Mutex<Option<Metadata>>,
}

#[derive(PartialEq)]
struct Metadata {
    title: String,
    subtitle: Option<String>,
    duration_ms: i64,
    artwork: Option<Arc<[u8]>>,
}

impl Metadata {
    fn of(state: &MediaSessionState) -> Self {
        Self {
            title: state.title.clone(),
            subtitle: state.subtitle.clone(),
            duration_ms: millis(state.duration),
            artwork: state.artwork.clone(),
        }
    }
}

impl AndroidDevice {
    pub fn new(app: AndroidApp, java: Option<Java>) -> Self {
        let speech = java.as_ref().is_some_and(speech_available);

        Self {
            app,
            java,
            speech,
            shown: Mutex::new(None),
        }
    }

    fn call<F>(&self, what: &str, call: F)
    where
        F: FnOnce(&mut Env, &JObject) -> jni::errors::Result<()>,
    {
        let Some(java) = &self.java else {
            return;
        };

        let _ = java.with_activity(what, call);
    }

    fn show_metadata(&self, state: &MediaSessionState) {
        let metadata = Metadata::of(state);
        let Ok(mut shown) = self.shown.lock() else {
            return;
        };

        if shown.as_ref() == Some(&metadata) {
            return;
        }

        self.call("setMediaMetadata", |env, activity| {
            let title = env.new_string(&metadata.title)?;
            let subtitle = optional_string(env, metadata.subtitle.as_deref())?;
            let artwork = optional_bytes(env, metadata.artwork.as_deref())?;
            let args = [
                JValue::Object(&title),
                JValue::Object(&subtitle),
                JValue::Long(metadata.duration_ms),
                JValue::Object(&artwork),
            ];
            let sig = jni_sig!("(Ljava/lang/String;Ljava/lang/String;J[B)V");
            env.call_method(activity, jni_str!("setMediaMetadata"), sig, &args)?
                .v()
        });

        *shown = Some(metadata);
    }

    fn clear_media_session(&self) {
        if let Ok(mut shown) = self.shown.lock() {
            *shown = None;
        }

        self.call("clearMediaSession", |env, activity| {
            env.call_method(activity, jni_str!("clearMediaSession"), jni_sig!("()V"), &[])?
                .v()
        });
    }
}

impl Device for AndroidDevice {
    fn attach(&self, ctx: &egui::Context) {
        events::attach(ctx);
    }

    fn keep_screen_on(&self, on: bool) {
        let flags = WindowManagerFlags::KEEP_SCREEN_ON;
        if on {
            self.app.set_window_flags(flags, WindowManagerFlags::empty());
            return;
        }

        self.app.set_window_flags(WindowManagerFlags::empty(), flags);
    }

    fn play_sound(&self, sound: UiSound) {
        let (code, repeat) = sound_code(sound);
        self.call("playUiSound", |env, activity| {
            let args = [JValue::Int(code), JValue::Bool(repeat)];
            env.call_method(activity, jni_str!("playUiSound"), jni_sig!("(IZ)V"), &args)?
                .v()
        });
    }

    fn update_media_session(&self, state: Option<&MediaSessionState>) {
        let Some(state) = state else {
            self.clear_media_session();
            return;
        };

        self.show_metadata(state);
        self.call("setMediaPlayback", |env, activity| {
            let args = [
                JValue::Bool(state.playing),
                JValue::Long(millis(state.position)),
                JValue::Bool(state.can_next),
                JValue::Bool(state.can_previous),
            ];
            let sig = jni_sig!("(ZJZZ)V");
            env.call_method(activity, jni_str!("setMediaPlayback"), sig, &args)?
                .v()
        });
    }

    fn speech_input_available(&self) -> bool {
        self.speech
    }

    fn start_speech_input(&self, request: &SpeechRequest) {
        self.call("startSpeechInput", |env, activity| {
            let language = env.new_string(&request.language)?;
            let args = [JValue::Object(&language)];
            let sig = jni_sig!("(Ljava/lang/String;)V");
            env.call_method(activity, jni_str!("startSpeechInput"), sig, &args)?
                .v()
        });
    }

    fn stop_speech_input(&self) {
        self.call("stopSpeechInput", |env, activity| {
            env.call_method(activity, jni_str!("stopSpeechInput"), jni_sig!("()V"), &[])?
                .v()
        });
    }

    fn soft_keyboard(&self) -> bool {
        true
    }

    fn start_text_input(&self, spec: TextInputSpec, field: &FieldText) {
        let purpose = purpose_code(spec.purpose);
        let action = action_code(spec.action);
        let (start, end) = selection(field);
        self.call("startTextInput", |env, activity| {
            let text = env.new_string(&field.text)?;
            let args = [
                JValue::Int(purpose),
                JValue::Int(action),
                JValue::Object(&text),
                JValue::Int(start),
                JValue::Int(end),
            ];
            let sig = jni_sig!("(IILjava/lang/String;II)V");
            env.call_method(activity, jni_str!("startTextInput"), sig, &args)?
                .v()
        });
    }

    fn update_text_input(&self, field: &FieldText) {
        let (start, end) = selection(field);
        self.call("updateTextInput", |env, activity| {
            let text = env.new_string(&field.text)?;
            let args = [JValue::Object(&text), JValue::Int(start), JValue::Int(end)];
            let sig = jni_sig!("(Ljava/lang/String;II)V");
            env.call_method(activity, jni_str!("updateTextInput"), sig, &args)?
                .v()
        });
    }

    fn stop_text_input(&self) {
        self.call("stopTextInput", |env, activity| {
            env.call_method(activity, jni_str!("stopTextInput"), jni_sig!("()V"), &[])?
                .v()
        });
    }

    fn poll_event(&self) -> Option<DeviceEvent> {
        events::pop()
    }
}

/// In characters, which Java counts as code points.
fn selection(field: &FieldText) -> (jint, jint) {
    let start = jint::try_from(field.selection.start).unwrap_or(jint::MAX);
    let end = jint::try_from(field.selection.end).unwrap_or(jint::MAX);

    (start, end)
}

fn speech_available(java: &Java) -> bool {
    let available = java.with_activity("isSpeechInputAvailable", |env, activity| {
        env.call_method(activity, jni_str!("isSpeechInputAvailable"), jni_sig!("()Z"), &[])?
            .z()
    });

    available.unwrap_or(false)
}

/// Codes of the `UiSounds` constants in Java.
fn sound_code(sound: UiSound) -> (i32, bool) {
    let UiSound::Navigate { direction, repeat } = sound else {
        return (0, false);
    };

    let code = match direction {
        Direction::Up => 1,
        Direction::Down => 2,
        Direction::Left => 3,
        Direction::Right => 4,
    };

    (code, repeat)
}

/// Codes of `TextInputView.PURPOSE_*` in Java.
fn purpose_code(purpose: TextPurpose) -> i32 {
    match purpose {
        TextPurpose::Text => 0,
        TextPurpose::Url => 1,
        TextPurpose::Secret => 2,
        TextPurpose::Verbatim => 3,
    }
}

/// Codes of `TextInputView.ACTION_*` in Java.
fn action_code(action: TextAction) -> i32 {
    match action {
        TextAction::Done => 0,
        TextAction::Search => 1,
    }
}

fn millis(seconds: f64) -> i64 {
    (seconds * 1000.0).round() as i64
}

fn optional_string<'local>(env: &mut Env<'local>, text: Option<&str>) -> jni::errors::Result<JObject<'local>> {
    let Some(text) = text else {
        return Ok(JObject::null());
    };

    Ok(env.new_string(text)?.into())
}

fn optional_bytes<'local>(env: &mut Env<'local>, bytes: Option<&[u8]>) -> jni::errors::Result<JObject<'local>> {
    let Some(bytes) = bytes else {
        return Ok(JObject::null());
    };

    let array: JByteArray<'local> = env.byte_array_from_slice(bytes)?;
    Ok(array.into())
}
