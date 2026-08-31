use crate::data;
use crate::engine::{Context, Typograf};
use crate::re;
use crate::PRIVATE;

const BRACKET_CLASSES: &[&str] = &[
    "typograf-oa-lbracket",
    "typograf-oa-n-lbracket",
    "typograf-oa-sp-lbracket",
];
const COMMA_CLASSES: &[&str] = &["typograf-oa-comma", "typograf-oa-comma-sp"];
const QUOTE_CLASSES: &[&str] = &[
    "typograf-oa-lquote",
    "typograf-oa-n-lquote",
    "typograf-oa-sp-lquote",
];

fn remove_tags(text: &str, class_names: &[&str]) -> String {
    let re = re::compile_s(&format!(
        "<span class=\"({})\">(.*?)</span>",
        class_names.join("|")
    ));

    re::replace_all(&re, text, "$2")
}

fn remove_tags_from_title(text: &str, class_names: &[&str]) -> String {
    let re = re::compile_is("<title>.*?</title>");

    re::replace_all_fn(&re, text, |caps| remove_tags(&caps[0], class_names))
}

pub fn bracket(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static SP: std::sync::LazyLock<fancy_regex::Regex> =
        std::sync::LazyLock::new(|| re::compile("( |\u{00A0})\\("));
    static START: std::sync::LazyLock<fancy_regex::Regex> =
        std::sync::LazyLock::new(|| re::compile_m("^\\("));
    let step = re::replace_all(
        &SP,
        text,
        "<span class=\"typograf-oa-sp-lbracket\">$1</span><span class=\"typograf-oa-lbracket\">(</span>",
    );

    re::replace_all(&START, &step, "<span class=\"typograf-oa-n-lbracket\">(</span>")
}

pub fn bracket_start(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    remove_tags(text, BRACKET_CLASSES)
}

pub fn bracket_end(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    remove_tags_from_title(text, BRACKET_CLASSES)
}

pub fn comma(_tp: &Typograf, text: &str, ctx: &Context<'_>) -> String {
    let char = ctx.chars();
    let re = re::compile_i(&format!("([{char}\\d\u{0301}]+), "));

    re::replace_all(
        &re,
        text,
        "$1<span class=\"typograf-oa-comma\">,</span><span class=\"typograf-oa-comma-sp\"> </span>",
    )
}

pub fn comma_start(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    remove_tags(text, COMMA_CLASSES)
}

pub fn comma_end(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    remove_tags_from_title(text, COMMA_CLASSES)
}

pub fn quote(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let quote = data::quote("ru");
    let left = quote.map(|q| q.left).unwrap_or("«„‚");
    let l0 = left.chars().next().unwrap_or('«');
    let l1 = left.chars().nth(1).unwrap_or('\0');
    let lquotes = if l1 == '\0' {
        format!("([{l0}])")
    } else {
        format!("([{l0}{l1}])")
    };
    let re_new = re::compile(&format!("(^|\n\n|{PRIVATE})({lquotes})"));
    let re_inside = re::compile_i(&format!("([^\\n{PRIVATE}])([ \u{00A0}\\n])({lquotes})"));
    let step = re::replace_all(
        &re_new,
        text,
        "$1<span class=\"typograf-oa-n-lquote\">$2</span>",
    );

    re::replace_all(
        &re_inside,
        &step,
        "$1<span class=\"typograf-oa-sp-lquote\">$2</span><span class=\"typograf-oa-lquote\">$3</span>",
    )
}

pub fn quote_start(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    remove_tags(text, QUOTE_CLASSES)
}

pub fn quote_end(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    remove_tags_from_title(text, QUOTE_CLASSES)
}
