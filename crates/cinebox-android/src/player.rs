//! The app's video player on Android: Media3 ExoPlayer in `VideoPlayer.java`,
//! showing the picture in a SurfaceView below the window.

use std::sync::{Mutex, PoisonError};

use cinebox_core::VideoScale;
use cinebox_player::{
    Error, Failure, Features, Media, PixelRect, Player, Snapshot, Track, TrackKind, VideoDecoder,
    VideoOutput,
};
use jni::objects::{JObject, JString, JValue};
use jni::refs::Global;
use jni::strings::JNIStr;
use jni::{Env, JavaVM, jni_sig, jni_str};

use crate::device::optional_string;
use crate::jvm::Java;

/// `VideoPlayer.FLAG_*` and `VideoPlayer.FAILURE_*`.
const FLAG_PAUSED: i32 = 1;
const FLAG_ENDED: i32 = 2;
const FAILURE_NONE: i32 = 0;
const FAILURE_UNSUPPORTED: i32 = 1;
const FAILURE_NETWORK: i32 = 2;

/// Set through [`Player::on_change`]; Java runs it through `Natives.onPlayerChanged`.
static WAKE: Mutex<Option<Box<dyn Fn() + Send + Sync>>> = Mutex::new(None);

pub fn wake() {
    let wake = WAKE.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(wake) = wake.as_ref() {
        wake();
    }
}

pub struct ExoPlayer {
    vm: JavaVM,
    player: Global<JObject<'static>>,
    /// The app places the picture every frame; Java hears only of changes.
    placed: Mutex<Option<PixelRect>>,
}

impl ExoPlayer {
    pub fn attach(java: &Java) -> Option<Self> {
        let player = java.with_activity("videoPlayer", |env, activity| {
            let sig = jni_sig!("()Lio/github/dexsper/cinebox/VideoPlayer;");
            let player = env
                .call_method(activity, jni_str!("videoPlayer"), sig, &[])?
                .l()?;

            env.new_global_ref(&player)
        })?;

        if player.as_obj().as_raw().is_null() {
            tracing::error!("the activity has no video player");
            return None;
        }

        Some(Self {
            vm: java.vm().clone(),
            player,
            placed: Mutex::new(None),
        })
    }

    fn call<T, F>(&self, what: &str, call: F) -> Result<T, Error>
    where
        F: FnOnce(&mut Env, &JObject) -> jni::errors::Result<T>,
    {
        self.vm
            .attach_current_thread(|env| call(env, self.player.as_obj()))
            .map_err(|error| Error::platform(format!("{what}: {error}")))
    }
}

impl Player for ExoPlayer {
    fn output(&self) -> VideoOutput {
        VideoOutput::Underlay
    }

    /// AC3 and E-AC3 go out undecoded where the TV or receiver takes them, so
    /// level, speed and filters are theirs. Subtitles come as text for the app.
    fn features(&self) -> Features {
        Features {
            volume: false,
            speed: false,
            subtitle_delay: false,
            draws_subtitles: false,
            loudnorm: false,
            decoder_choice: false,
        }
    }

    fn on_change(&self, wake: Box<dyn Fn() + Send + Sync>) {
        let mut slot = WAKE.lock().unwrap_or_else(PoisonError::into_inner);
        *slot = Some(wake);
    }

    /// Java's networking follows the OS proxy by itself, so `http_proxy` is
    /// not passed on.
    fn load(&self, media: &Media<'_>) -> Result<(), Error> {
        let headers = media.headers.join("\n");
        let start_ms = to_millis(media.start_seconds.max(0.0));

        self.call("load", |env, player| {
            let url = env.new_string(media.url)?;
            let headers = env.new_string(&headers)?;
            let audio = optional_string(env, media.audio_url)?;
            let args = [
                JValue::Object(&url),
                JValue::Object(&headers),
                JValue::Object(&audio),
                JValue::Long(start_ms),
            ];

            let sig = jni_sig!("(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;J)V");
            env.call_method(player, jni_str!("load"), sig, &args)?.v()
        })
    }

    fn stop(&self) {
        let result = self.call("stop", |env, player| {
            env.call_method(player, jni_str!("stop"), jni_sig!("()V"), &[])?
                .v()
        });

        if let Err(error) = result {
            tracing::warn!(%error, "player stop failed");
        }
    }

    fn set_paused(&self, paused: bool) -> Result<(), Error> {
        self.call("set_paused", |env, player| {
            let args = [JValue::Bool(paused)];
            env.call_method(player, jni_str!("setPaused"), jni_sig!("(Z)V"), &args)?
                .v()
        })
    }

    fn seek_by(&self, seconds: f64) -> Result<(), Error> {
        self.call("seek_by", |env, player| {
            let args = [JValue::Long(to_millis(seconds))];
            env.call_method(player, jni_str!("seekBy"), jni_sig!("(J)V"), &args)?
                .v()
        })
    }

    fn seek_to(&self, seconds: f64) -> Result<(), Error> {
        self.call("seek_to", |env, player| {
            let args = [JValue::Long(to_millis(seconds))];
            env.call_method(player, jni_str!("seekTo"), jni_sig!("(J)V"), &args)?
                .v()
        })
    }

    /// The app fits the picture itself and hands it to `place_video`.
    fn set_scale(&self, _scale: VideoScale) -> Result<(), Error> {
        Ok(())
    }

    /// Not offered: see `features`.
    fn set_speed(&self, _speed: f64) -> Result<(), Error> {
        Ok(())
    }

    /// Not offered: see `features`.
    fn set_volume(&self, _volume: f64) -> Result<(), Error> {
        Ok(())
    }

    /// Not offered: see `features`.
    fn set_muted(&self, _muted: bool) -> Result<(), Error> {
        Ok(())
    }

    fn select_audio(&self, id: i64) -> Result<(), Error> {
        let id = i32::try_from(id).map_err(Error::platform)?;

        self.call("select_audio", |env, player| {
            let args = [JValue::Int(id)];
            env.call_method(player, jni_str!("selectAudio"), jni_sig!("(I)V"), &args)?
                .v()
        })
    }

    fn select_subtitle(&self, id: Option<i64>) -> Result<(), Error> {
        // `VideoPlayer.selectSubtitle` takes 0 for off.
        let id = i32::try_from(id.unwrap_or(0)).map_err(Error::platform)?;

        self.call("select_subtitle", |env, player| {
            let args = [JValue::Int(id)];
            env.call_method(player, jni_str!("selectSubtitle"), jni_sig!("(I)V"), &args)?
                .v()
        })
    }

    /// The app draws the subtitles, at its own scale.
    fn set_subtitle_scale(&self, _scale: f64) -> Result<(), Error> {
        Ok(())
    }

    /// Not offered: see `features`.
    fn set_subtitle_delay(&self, _seconds: f64) -> Result<(), Error> {
        Ok(())
    }

    fn tracks(&self) -> Vec<Track> {
        let read = self.call("tracks", |env, player| {
            text(env, player, jni_str!("tracks"))
        });

        match read {
            Ok(lines) => parse_tracks(&lines.unwrap_or_default()),
            Err(error) => {
                tracing::warn!(%error, "reading tracks failed");
                Vec::new()
            }
        }
    }

    fn snapshot(&self) -> Snapshot {
        let read = self.call("snapshot", |env, player| {
            let flags = int(env, player, jni_str!("flags"))?;
            let width = int(env, player, jni_str!("videoWidth"))?;
            let height = int(env, player, jni_str!("videoHeight"))?;

            Ok(Snapshot {
                paused: flags & FLAG_PAUSED != 0,
                time: to_seconds(long(env, player, jni_str!("positionMs"))?),
                duration: to_seconds(long(env, player, jni_str!("durationMs"))?),
                eof: flags & FLAG_ENDED != 0,
                volume: 100.0,
                muted: false,
                video_size: video_size(width, height),
                subtitle: text(env, player, jni_str!("subtitle"))?,
                failure: failure(env, player)?,
            })
        });

        read.unwrap_or_else(|error| {
            tracing::warn!(%error, "reading player state failed");
            Snapshot::default()
        })
    }

    fn video_decoder(&self) -> Option<VideoDecoder> {
        let read = self.call("video_decoder", |env, player| {
            let name = text(env, player, jni_str!("decoderName"))?;
            let format = text(env, player, jni_str!("decoderFormat"))?;
            Ok((name, format))
        });

        let (name, format) = read.ok()?;
        let format = format.unwrap_or_default();
        let (codec, color) = format.split_once('\t').unwrap_or((format.as_str(), ""));

        Some(VideoDecoder {
            hwdec: name?,
            codec: codec.to_owned(),
            pixel_format: color.to_owned(),
        })
    }

    fn place_video(&self, rect: PixelRect) {
        let mut placed = self.placed.lock().unwrap_or_else(PoisonError::into_inner);
        if *placed == Some(rect) {
            return;
        }

        let result = self.call("place", |env, player| {
            let args = [
                JValue::Int(rect.x),
                JValue::Int(rect.y),
                JValue::Int(rect.width),
                JValue::Int(rect.height),
            ];
            env.call_method(player, jni_str!("place"), jni_sig!("(IIII)V"), &args)?
                .v()
        });

        match result {
            Ok(()) => *placed = Some(rect),
            Err(error) => tracing::warn!(%error, "placing the video failed"),
        }
    }
}

fn int(env: &mut Env, player: &JObject, name: &'static JNIStr) -> jni::errors::Result<i32> {
    env.call_method(player, name, jni_sig!("()I"), &[])?.i()
}

fn long(env: &mut Env, player: &JObject, name: &'static JNIStr) -> jni::errors::Result<i64> {
    env.call_method(player, name, jni_sig!("()J"), &[])?.j()
}

fn text(
    env: &mut Env,
    player: &JObject,
    name: &'static JNIStr,
) -> jni::errors::Result<Option<String>> {
    let value = env
        .call_method(player, name, jni_sig!("()Ljava/lang/String;"), &[])?
        .l()?;

    let value = env.cast_local::<JString>(value)?;
    if value.as_raw().is_null() {
        return Ok(None);
    }

    value.try_to_string(env).map(Some)
}

fn failure(env: &mut Env, player: &JObject) -> jni::errors::Result<Option<Failure>> {
    let failure = match int(env, player, jni_str!("failure"))? {
        FAILURE_NONE => None,
        FAILURE_UNSUPPORTED => Some(Failure::Unsupported),
        FAILURE_NETWORK => Some(Failure::Network),
        _ => {
            let message = text(env, player, jni_str!("failureMessage"))?;
            Some(Failure::Other(message.unwrap_or_default()))
        }
    };

    Ok(failure)
}

/// `VideoPlayer.tracks()`: one `kind`, `id`, `selected`, `language`, `label`
/// line per track, tab-separated.
fn parse_tracks(lines: &str) -> Vec<Track> {
    lines.lines().filter_map(parse_track).collect()
}

fn parse_track(line: &str) -> Option<Track> {
    let mut fields = line.split('\t');
    let kind = match fields.next()? {
        "video" => TrackKind::Video,
        "audio" => TrackKind::Audio,
        "sub" => TrackKind::Subtitle,
        _ => return None,
    };

    let id = fields.next()?.parse().ok()?;
    let selected = fields.next()? == "1";
    let lang = non_empty(fields.next());
    let title = non_empty(fields.next());

    Some(Track {
        id,
        kind,
        lang,
        title,
        selected,
    })
}

fn non_empty(field: Option<&str>) -> Option<String> {
    field.filter(|value| !value.is_empty()).map(str::to_owned)
}

fn video_size(width: i32, height: i32) -> Option<[u32; 2]> {
    let width = u32::try_from(width).ok().filter(|w| *w > 0)?;
    let height = u32::try_from(height).ok().filter(|h| *h > 0)?;

    Some([width, height])
}

fn to_millis(seconds: f64) -> i64 {
    (seconds * 1000.0).round() as i64
}

fn to_seconds(millis: i64) -> f64 {
    millis as f64 / 1000.0
}
