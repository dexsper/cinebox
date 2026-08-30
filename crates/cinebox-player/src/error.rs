/// Player failures. Never includes HTTP headers or passwords.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not create the libmpv render context")]
    MpvInit,
    #[error("mpv: {0}")]
    Mpv(String),
}

impl Error {
    pub(crate) fn mpv(error: impl std::fmt::Display) -> Self {
        Self::Mpv(error.to_string())
    }
}
