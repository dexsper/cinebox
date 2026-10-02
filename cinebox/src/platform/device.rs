//! What the app asks of the operating system beyond a window.

use std::ops::Range;
use std::sync::Arc;

/// One per platform entry point. Every method has a do-nothing default, which
/// is all the desktop needs today.
pub trait Device: Send + Sync {
    /// Called with the app's context, so events raised on other threads can
    /// wake the UI. May be called more than once with the same context.
    fn attach(&self, _ctx: &egui::Context) {}

    fn keep_screen_on(&self, _on: bool) {}

    /// The OS decides whether interface sounds are enabled.
    fn play_sound(&self, _sound: UiSound) {}

    /// `None` once nothing is playing.
    fn update_media_session(&self, _state: Option<&MediaSessionState>) {}

    fn speech_input_available(&self) -> bool {
        false
    }

    /// Progress and the result arrive as [`DeviceEvent::Speech`]. The OS may
    /// first ask the user for the microphone.
    fn start_speech_input(&self, _request: &SpeechRequest) {}

    fn stop_speech_input(&self) {}

    /// Text is typed on an on-screen keyboard that has to be asked for; its
    /// input arrives as [`DeviceEvent::Text`].
    fn soft_keyboard(&self) -> bool {
        false
    }

    /// `field` is what the field holds as typing starts.
    fn start_text_input(&self, _spec: TextInputSpec, _field: &FieldText) {}

    /// The app changed the field while the keyboard is open (a hardware key,
    /// a picked suggestion), so the keyboard edits the same text.
    fn update_text_input(&self, _field: &FieldText) {}

    fn stop_text_input(&self) {}

    fn poll_event(&self) -> Option<DeviceEvent> {
        None
    }
}

pub struct NoDevice;

impl Device for NoDevice {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiSound {
    /// Focus moved; `repeat` while the key is held.
    Navigate { direction: Direction, repeat: bool },
    Activate,
}

/// Transport control from outside the player: remote media keys, the system
/// media session, a voice assistant. Times are in seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MediaCommand {
    Play,
    Pause,
    PlayPause,
    Stop,
    SeekTo(f64),
    SeekBy(f64),
    Next,
    Previous,
}

/// What the system media session shows and accepts. Times are in seconds.
#[derive(Debug, Clone, PartialEq)]
pub struct MediaSessionState {
    pub title: String,
    pub subtitle: Option<String>,
    pub duration: f64,
    /// An encoded image (JPEG/PNG/WebP).
    pub artwork: Option<Arc<[u8]>>,
    pub playing: bool,
    pub position: f64,
    pub can_next: bool,
    pub can_previous: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeechRequest {
    /// BCP 47 tag.
    pub language: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpeechEvent {
    Listening,
    /// What has been heard so far; replaced by the next one.
    Partial(String),
    Final(String),
    /// Nothing heard, cancelled, or the microphone was refused.
    Ended,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextInputSpec {
    pub purpose: TextPurpose,
    pub action: TextAction,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TextPurpose {
    #[default]
    Text,
    /// Typed exactly as is, without suggestions or autocorrect: keys, user names.
    Verbatim,
    Url,
    /// Hidden from suggestions and the keyboard's learning.
    Secret,
}

/// The keyboard's confirm key.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TextAction {
    #[default]
    Done,
    Search,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DeviceEvent {
    Media(MediaCommand),
    Speech(SpeechEvent),
    /// The remote's search key.
    SearchKey,
    Text(TextInputEvent),
}

/// A text field's content as the keyboard sees it. Keyboards and remote
/// typing (a phone app) read the field back and may replace any part of it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FieldText {
    pub text: String,
    /// In characters; empty where it is only the cursor.
    pub selection: Range<usize>,
}

/// What the on-screen keyboard did to the field being edited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextInputEvent {
    /// The field's whole content after the keyboard's edit.
    Edited(FieldText),
    /// The confirm key.
    Action,
    /// Closed by the user (Back), not by the app.
    KeyboardHidden,
}
