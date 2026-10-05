mod data;
mod engine;
mod entities;
mod quote;
mod re;
mod rules;
mod safe_tags;

pub use engine::{LocaleError, Typograf};

/// Named and numeric HTML entities (`&amp;`, `&#171;`) to their characters,
/// without the rest of the typography.
#[must_use]
pub fn decode_entities(text: &str) -> std::borrow::Cow<'_, str> {
    entities::to_utf(text)
}

pub(crate) const PRIVATE: char = '\u{F000}';
pub(crate) const PRIVATE_SEPARATE: char = '\u{F001}';

/// Catalog helper: Russian + US English.
#[must_use]
pub fn typograph(input: &str) -> String {
    thread_local! {
        static ENGINE: Typograf = match Typograf::new(["ru", "en-US"]) {
            Ok(engine) => engine,
            Err(error) => unreachable!("bundled locales are valid: {error}"),
        };
    }

    ENGINE.with(|tp| tp.execute(input))
}
