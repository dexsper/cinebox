//! YouTube InnerTube player plus JS signature decipher for libmpv.

#![forbid(unsafe_code)]

mod cipher;
mod error;
mod formats;
mod http;
mod id;
mod jsinterp;
mod resolve;

pub use error::Error;
pub use id::VideoId;
pub use jsinterp::{JSInterpreter, JsFunction, JsValue};
pub use resolve::{resolve, Playback};
