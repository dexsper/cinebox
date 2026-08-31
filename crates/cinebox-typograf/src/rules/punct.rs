use std::sync::LazyLock;

use crate::engine::{Context, Typograf};
use crate::quote;
use crate::re;

pub fn apostrophe(_tp: &Typograf, text: &str, ctx: &Context<'_>) -> String {
    let char = ctx.chars();
    let re = re::compile_i(&format!("([{char}])'([{char}])"));

    re::replace_all(&re, text, "$1’$2")
}

pub fn del_double(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static COMMA: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(^|[^,]),,(?!,)"));
    static COLON: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(^|[^:]):{2}(?!:)"));
    static DOT: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(^|[^!?.])\\.\\.(?!\\.)"));
    static SEMI: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(^|[^;]);;(?!;)"));
    static Q: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(^|[^?])\\?\\?(?!\\?)"));

    let mut out = re::replace_all(&COMMA, text, "$1,");
    out = re::replace_all(&COLON, &out, "$1:");
    out = re::replace_all(&DOT, &out, "$1.");
    out = re::replace_all(&SEMI, &out, "$1;");

    re::replace_all(&Q, &out, "$1?")
}

pub fn hellip(_tp: &Typograf, text: &str, ctx: &Context<'_>) -> String {
    if ctx.primary_locale() == "ru" {
        static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(^|[^.])\\.{3,4}(?=[^.]|$)"));

        return re::replace_all(&RE, text, "$1…");
    }

    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(^|[^.])\\.{3}(\\.?)(?=[^.]|$)"));

    re::replace_all(&RE, text, "$1…$2")
}

pub fn quote_rule(tp: &Typograf, text: &str, ctx: &Context<'_>) -> String {
    let _ = tp;
    let Some(settings) = ctx.quote() else {
        return text.to_string();
    };

    quote::process(text, settings, ctx)
}

pub fn quote_link(_tp: &Typograf, text: &str, ctx: &Context<'_>) -> String {
    let Some(quotes) = ctx.quote() else {
        return text.to_string();
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
    let re = re::compile_s(&format!("(<[aA]\\s[^>]*?>)({lpart})(.*?)({rpart})(</[aA]>)"));

    re::replace_all(&re, text, "$2$1$3$5$4")
}
