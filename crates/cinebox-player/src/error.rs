/// Player failures. Never includes HTTP headers or passwords.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("embedded playback is Windows-only")]
    Unsupported,
    #[error("could not create the video window")]
    Hole,
    #[error("mpv failed to start (libmpv-2.dll missing from the application folder)")]
    MpvInit,
    #[error("mpv: {0}")]
    Mpv(String),
}

impl Error {
    pub(crate) fn mpv(error: impl std::fmt::Display) -> Self {
        Self::Mpv(error.to_string())
    }
}
