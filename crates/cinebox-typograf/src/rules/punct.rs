use std::borrow::Cow;
use std::sync::LazyLock;

use crate::engine::{Context, Typograf};
use crate::quote;
use crate::re;

pub fn apostrophe<'a>(_tp: &Typograf, text: &'a str, ctx: &Context<'_>) -> Cow<'a, str> {
    let char = ctx.chars();
    let re = re::cached_i(&format!("([{char}])'([{char}])"));

    re::replace_all(&re, text, "$1’$2")
}

pub fn del_double<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static COMMA: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(^|[^,]),,(?!,)"));
    static COLON: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(^|[^:]):{2}(?!:)"));
    static DOT: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(^|[^!?.])\\.\\.(?!\\.)"));
    static SEMI: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(^|[^;]);;(?!;)"));
    static Q: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(^|[^?])\\?\\?(?!\\?)"));

    let mut out = re::replace_all(&COMMA, text, "$1,");
    out = re::chain(&COLON, out, "$1:");
    out = re::chain(&DOT, out, "$1.");
    out = re::chain(&SEMI, out, "$1;");

    re::chain(&Q, out, "$1?")
}

pub fn hellip<'a>(_tp: &Typograf, text: &'a str, ctx: &Context<'_>) -> Cow<'a, str> {
    if ctx.primary_locale() == "ru" {
        static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(^|[^.])\\.{3,4}(?=[^.]|$)"));

        return re::replace_all(&RE, text, "$1…");
    }

    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(^|[^.])\\.{3}(\\.?)(?=[^.]|$)"));
    re::replace_all(&RE, text, "$1…$2")
}

pub fn quote_rule<'a>(_tp: &Typograf, text: &'a str, ctx: &Context<'_>) -> Cow<'a, str> {
    let Some(settings) = ctx.quote() else {
        return Cow::Borrowed(text);
    };

    quote::process(text, settings, ctx)
}

pub fn quote_link<'a>(_tp: &Typograf, text: &'a str, ctx: &Context<'_>) -> Cow<'a, str> {
    let Some(quotes) = ctx.quote() else {
        return Cow::Borrowed(text);
    };

    let l1 = quotes.left.chars().next().unwrap_or('"');
    let r1 = quotes.right.chars().next().unwrap_or('"');
    let l2 = quotes.left.chars().nth(1);
    let r2 = quotes.right.chars().nth(1);
    let lpart = match l2 {
        Some(c) => format!("{l1}|{c}"),
        None => l1.to_string(),
    };

    let rpart = match r2 {
        Some(c) => format!("{r1}|{c}"),
        None => r1.to_string(),
    };

    let re = re::cached_s(&format!("(<[aA]\\s[^>]*?>)({lpart})(.*?)({rpart})(</[aA]>)"));
    re::replace_all(&re, text, "$2$1$3$5$4")
}
