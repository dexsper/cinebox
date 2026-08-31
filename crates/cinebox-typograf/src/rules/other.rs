use crate::data;
use crate::engine::{Context, Typograf};
use crate::re;

pub fn del_bom(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    text.strip_prefix('\u{FEFF}').unwrap_or(text).to_string()
}

pub fn repeat_word(_tp: &Typograf, text: &str, ctx: &Context<'_>) -> String {
    let char = ctx.chars();
    let punc = format!("[;:,.?! \u{00a0}\\n{}]", data::COMMON_QUOTE);
    let re = re::compile_i(&format!("({punc}|^)([{char}]{{2,}})[ \u{00a0}]\\2({punc}|$)"));

    re::replace_all(&re, text, "$1$2$3")
}
