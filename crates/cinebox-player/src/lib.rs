//! libmpv2 playback with a child HWND. Wired in Phase 7.
//!
//! This crate will contain the only `unsafe` in the workspace (HWND / `wid`).
//! Stream URLs come from TorrServer; pass `cinebox_torrserver::mpv_http_header_fields`
//! as mpv `http-header-fields` when Basic auth is set.

/// Crate identity for workspace wiring checks.
pub const CRATE_NAME: &str = "cinebox-player";
