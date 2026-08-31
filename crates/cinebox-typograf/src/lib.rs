mod data;
mod engine;
mod entities;
mod quote;
mod re;
mod rules;
mod safe_tags;

pub use engine::Typograf;

pub(crate) const PRIVATE: char = '\u{F000}';
pub(crate) const PRIVATE_SEPARATE: char = '\u{F001}';

/// Catalog helper: Russian + US English.
#[must_use]
pub fn typograph(input: &str) -> String {
    thread_local! {
        static ENGINE: Typograf = Typograf::new(["ru", "en-US"]);
    }

    ENGINE.with(|tp| tp.execute(input))
}
