use std::sync::LazyLock;

use crate::engine::{Context, Typograf};
use crate::re;

pub fn arrow(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static RIGHT: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(^|[^-])->(?!>)"));
    static LEFT: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(^|[^<])<-(?!-)"));
    let step = re::replace_all(&RIGHT, text, "$1→");

    re::replace_all(&LEFT, &step, "$1←")
}

pub fn cf(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile_m("(^|[\\s(\\[+≈±−—–\\-])(\\d+(?:[.,]\\d+)?)[ \u{00A0}\u{2009}]?(C|F)([\\W\\s.,:!?\")\\]]|$)")
    });

    re::replace_all(&RE, text, "$1$2\u{2009}°$3$4")
}

pub fn copy(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static R: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile_i("\\(r\\)"));
    static C: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile_i("(copyright )?\\((c|с)\\)"));
    static TM: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile_i("\\(tm\\)"));
    let a = re::replace_all(&R, text, "®");
    let b = re::replace_all(&C, &a, "©");

    re::replace_all(&TM, &b, "™")
}
