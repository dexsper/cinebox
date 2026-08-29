//! libmpv2 playback with a child HWND. Wired in Phase 7.
//!
//! This crate will contain the only `unsafe` in the workspace (HWND / `wid`).

/// Crate identity for workspace wiring checks.
pub const CRATE_NAME: &str = "cinebox-player";
