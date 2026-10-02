/// Player failures. Never includes HTTP headers or passwords.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not create the libmpv render context")]
    MpvInit,
    #[error("mpv: {0}")]
    Mpv(String),
    #[error("player: {0}")]
    Platform(String),
}

impl Error {
    #[cfg(not(target_os = "android"))]
    pub(crate) fn mpv(error: impl std::fmt::Display) -> Self {
        Self::Mpv(error.to_string())
    }

    #[cfg(not(target_os = "android"))]
    pub(crate) fn mpv_prop(name: &str, error: impl std::fmt::Display) -> Self {
        Self::Mpv(format!("{name}: {error}"))
    }

    /// A platform player refused a call or could not be reached.
    pub fn platform(error: impl std::fmt::Display) -> Self {
        Self::Platform(error.to_string())
    }
}
