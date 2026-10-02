//! `static native` methods of `io.github.dexsper.cinebox.Natives`, called on
//! the Java UI thread. The symbol names are that class's JNI names.

use cinebox::platform::{DeviceEvent, MediaCommand, SpeechEvent, TextInputEvent};
use cinebox_player::SEEK_SECS;
use jni::errors::ThrowRuntimeExAndDefault;
use jni::objects::{JClass, JString};
use jni::sys::{jdouble, jint};
use jni::{Env, EnvUnowned};

use crate::events;

/// Codes of `MediaSessionController.COMMAND_*` in Java.
fn media_command(code: jint, seconds: f64) -> Option<MediaCommand> {
    let command = match code {
        0 => MediaCommand::Play,
        1 => MediaCommand::Pause,
        2 => MediaCommand::PlayPause,
        3 => MediaCommand::Stop,
        4 => MediaCommand::SeekTo(seconds),
        5 => MediaCommand::SeekBy(SEEK_SECS),
        6 => MediaCommand::SeekBy(-SEEK_SECS),
        7 => MediaCommand::Next,
        8 => MediaCommand::Previous,
        _ => return None,
    };

    Some(command)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_dexsper_cinebox_Natives_onMediaCommand(
    _env: EnvUnowned<'_>,
    _class: JClass<'_>,
    code: jint,
    seconds: jdouble,
) {
    if let Some(command) = media_command(code, seconds) {
        events::push(DeviceEvent::Media(command));
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_dexsper_cinebox_Natives_onSpeechListening(
    _env: EnvUnowned<'_>,
    _class: JClass<'_>,
) {
    events::push(speech(SpeechEvent::Listening));
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_dexsper_cinebox_Natives_onSpeechPartial<'caller>(
    mut env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    text: JString<'caller>,
) {
    with_text(&mut env, &text, |text| speech(SpeechEvent::Partial(text)));
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_dexsper_cinebox_Natives_onSpeechResult<'caller>(
    mut env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    text: JString<'caller>,
) {
    with_text(&mut env, &text, |text| speech(SpeechEvent::Final(text)));
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_dexsper_cinebox_Natives_onSpeechEnded(
    _env: EnvUnowned<'_>,
    _class: JClass<'_>,
) {
    events::push(speech(SpeechEvent::Ended));
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_dexsper_cinebox_Natives_onComposingText<'caller>(
    mut env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    text: JString<'caller>,
) {
    with_text(&mut env, &text, |text| typed(TextInputEvent::Preedit(text)));
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_dexsper_cinebox_Natives_onCommitText<'caller>(
    mut env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    text: JString<'caller>,
) {
    with_text(&mut env, &text, |text| typed(TextInputEvent::Commit(text)));
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_dexsper_cinebox_Natives_onDeleteSurrounding(
    _env: EnvUnowned<'_>,
    _class: JClass<'_>,
    before: jint,
    after: jint,
) {
    let event = TextInputEvent::DeleteSurrounding {
        before: usize::try_from(before).unwrap_or(0),
        after: usize::try_from(after).unwrap_or(0),
    };
    events::push(typed(event));
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_dexsper_cinebox_Natives_onEditorAction(
    _env: EnvUnowned<'_>,
    _class: JClass<'_>,
) {
    events::push(typed(TextInputEvent::Action));
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_dexsper_cinebox_Natives_onKeyboardHidden(
    _env: EnvUnowned<'_>,
    _class: JClass<'_>,
) {
    events::push(typed(TextInputEvent::KeyboardHidden));
}

fn typed(event: TextInputEvent) -> DeviceEvent {
    DeviceEvent::Text(event)
}

fn speech(event: SpeechEvent) -> DeviceEvent {
    DeviceEvent::Speech(event)
}

fn with_text<F>(env: &mut EnvUnowned<'_>, text: &JString<'_>, event: F)
where
    F: FnOnce(String) -> DeviceEvent,
{
    let outcome = env.with_env(|env: &mut Env<'_>| -> jni::errors::Result<()> {
        let text = text.try_to_string(env)?;
        events::push(event(text));
        Ok(())
    });

    outcome.resolve::<ThrowRuntimeExAndDefault>();
}
