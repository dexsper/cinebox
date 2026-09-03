use std::borrow::Cow;

use crate::data;
use crate::engine::{Context, Typograf};
use crate::re;

pub fn del_bom<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    match text.strip_prefix('\u{FEFF}') {
        Some(stripped) => Cow::Owned(stripped.to_string()),
        None => Cow::Borrowed(text),
    }
}

pub fn repeat_word<'a>(_tp: &Typograf, text: &'a str, ctx: &Context<'_>) -> Cow<'a, str> {
    let char = ctx.chars();
    let punc = format!("[;:,.?! \u{00a0}\\n{}]", data::COMMON_QUOTE);
    let re = re::cached_i(&format!("({punc}|^)([{char}]{{2,}})[ \u{00a0}]\\2({punc}|$)"));

    re::replace_all(&re, text, "$1$2$3")
}
