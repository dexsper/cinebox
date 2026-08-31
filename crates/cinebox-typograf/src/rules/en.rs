use std::sync::LazyLock;

use crate::data;
use crate::engine::{Context, Typograf};
use crate::re;

pub fn dash_en_us(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    dash_en(text)
}

pub fn dash_en_gb(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    dash_en(text)
}

fn dash_en(text: &str) -> String {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile(&format!(
            "[ \u{00A0}]({})([ \u{00A0}\\n])",
            data::COMMON_DASH
        ))
    });

    re::replace_all(&RE, text, "\u{00A0}\u{2014}$2")
}
