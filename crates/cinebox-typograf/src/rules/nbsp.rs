use std::borrow::Cow;
use std::sync::LazyLock;

use crate::data;
use crate::engine::{Context, Typograf};
use crate::re;
use crate::PRIVATE;

pub fn after_number<'a>(_tp: &Typograf, text: &'a str, ctx: &Context<'_>) -> Cow<'a, str> {
    let char = ctx.chars();
    let re = re::cached_i(&format!("(^|\\s)(\\d{{1,5}}) ([{char}]+)"));

    re::replace_all(&re, text, "$1$2\u{00A0}$3")
}

pub fn after_paragraph_mark<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("¶ ?(?=\\d)"));

    re::replace_all(&RE, text, "¶\u{00A0}")
}

pub fn after_section_mark<'a>(_tp: &Typograf, text: &'a str, ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile("§[ \u{00A0}\u{2009}]?(?=\\d|I|V|X)")
    });
    
    let repl = if ctx.primary_locale() == "ru" {
        "§\u{202F}"
    } else {
        "§\u{00A0}"
    };

    re::replace_all(&RE, text, repl)
}

pub fn after_short_word<'a>(_tp: &Typograf, text: &'a str, ctx: &Context<'_>) -> Cow<'a, str> {
    let char = ctx.chars();
    let before = format!(" \u{00A0}({PRIVATE}{}", data::COMMON_QUOTE);
    let sub = format!("(^|[{before}])([{char}]{{1,2}}) ");
    let re = re::cached_im(&sub);
    let step = re::replace_all(&re, text, "$1$2\u{00A0}");

    re::chain(&re, step, "$1$2\u{00A0}")
}

pub fn after_short_word_by_list<'a>(_tp: &Typograf, text: &'a str, ctx: &Context<'_>) -> Cow<'a, str> {
    let Some(short_word) = ctx.short_word() else {
        return Cow::Borrowed(text);
    };

    let before = format!(" \u{00A0}({PRIVATE}{}", data::COMMON_QUOTE);
    let sub = format!("(^|[{before}])({short_word}) ");
    let re = re::cached_im(&sub);
    let step = re::replace_all(&re, text, "$1$2\u{00A0}");

    re::chain(&re, step, "$1$2\u{00A0}")
}

pub fn before_short_last_number<'a>(_tp: &Typograf, text: &'a str, ctx: &Context<'_>) -> Cow<'a, str> {
    let quote_right = ctx.quote().map(|q| q.right).unwrap_or("");
    let ch = ctx.chars();
    let upper = ch.to_uppercase();
    let re = re::cached_m(&format!(
        "([{ch}{upper}]) (?=\\d{{1,2}}[-+−%'\"{quote_right})]?([.!?…]( [{upper}]|$)|$))"
    ));

    re::replace_all(&re, text, "$1\u{00A0}")
}

pub fn before_short_last_word<'a>(_tp: &Typograf, text: &'a str, ctx: &Context<'_>) -> Cow<'a, str> {
    let ch = ctx.chars();
    let upper = ch.to_uppercase();
    let re = re::cached(&format!(
        "([{ch}\\d]) ([{ch}{upper}]{{1,3}}[.!?…])( [{upper}]|$)"
    ));

    re::replace_all(&re, text, "$1\u{00A0}$2$3")
}

pub fn dpi<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(\\d) ?(lpi|dpi)(?!\\w)"));

    re::replace_first(&RE, text, "$1\u{00A0}$2")
}

pub fn nowrap<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    fn inner(caps: &fancy_regex::Captures<'_, str>) -> String {
        static NBSP: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
            re::compile("([^\u{00A0}])\u{00A0}([^\u{00A0}])")
        });
        let inner = re::replace_all(&NBSP, &caps[2], "$1 $2");

        format!("{}{inner}{}", &caps[1], &caps[3])
    }

    static NOWRAP: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(<nowrap>)(.*?)(</nowrap>)"));
    static NOBR: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(<nobr>)(.*?)(</nobr>)"));

    let step = re::replace_all_fn(&NOWRAP, text, inner);

    re::chain_fn(&NOBR, step, inner)
}

pub fn replace_nbsp<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    if !text.contains('\u{00A0}') {
        return Cow::Borrowed(text);
    }

    Cow::Owned(text.replace('\u{00A0}', " "))
}
