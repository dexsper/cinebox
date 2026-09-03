use std::borrow::Cow;
use std::sync::LazyLock;

use crate::data;
use crate::engine::{Context, Typograf};
use crate::re;
use crate::PRIVATE;

pub fn after_colon<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile(&format!("(\\D):([^)\",:.?\\s/\\\\{PRIVATE}])"))
    });

    re::replace_all(&RE, text, "$1: $2")
}

pub fn after_comma<'a>(_tp: &Typograf, text: &'a str, ctx: &Context<'_>) -> Cow<'a, str> {
    let quotes = match ctx.quote() {
        Some(q) => q.right.to_string(),
        None => data::COMMON_QUOTE.to_string(),
    };
    
    let class = re::escape(&quotes);
    let re = re::cached(&format!("(.),([^)\",:.?\\s/\\\\{PRIVATE}{class}])"));

    re::replace_all_fn(&re, text, |caps| {
        if re::is_digit_char(&caps[1]) && re::is_digit_char(&caps[2]) {
            return caps[0].to_string();
        }

        format!("{}, {}", &caps[1], &caps[2])
    })
}

pub fn after_exclamation<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile(&format!(
            "!([^).…!;?\\s\\[\\]){PRIVATE}{}])",
            re::escape(data::COMMON_QUOTE)
        ))
    });

    re::replace_all(&RE, text, "! $1")
}

pub fn after_question<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile(&format!(
            "\\?([^).…!;?\\s\\[\\]){PRIVATE}{}])",
            re::escape(data::COMMON_QUOTE)
        ))
    });

    re::replace_all(&RE, text, "? $1")
}

pub fn after_semicolon<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile(&format!(
            ";([^).…!;?\\s\\[\\]){PRIVATE}{}])",
            re::escape(data::COMMON_QUOTE)
        ))
    });

    re::replace_all(&RE, text, "; $1")
}

pub fn before_bracket<'a>(_tp: &Typograf, text: &'a str, ctx: &Context<'_>) -> Cow<'a, str> {
    let char = ctx.chars();
    let re = re::cached_i(&format!("([{char}.!?,;…)])\\("));

    re::replace_all(&re, text, "$1 (")
}

pub fn bracket<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static OPEN: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(\\() +"));
    static CLOSE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile(" +\\)"));
    let step = re::replace_all(&OPEN, text, "(");

    re::chain(&CLOSE, step, ")")
}

pub fn square_bracket<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static OPEN: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(\\[) +"));
    static CLOSE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile(" +\\]"));
    let step = re::replace_all(&OPEN, text, "[");

    re::chain(&CLOSE, step, "]")
}

pub fn del_before_dot<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile("(^|[^!?:;,.…]) (\\.|\\.\\.\\.)(\\s|$)")
    });

    re::replace_all(&RE, text, "$1$2$3")
}

pub fn del_before_percent<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile("(\\d)( |\u{00A0})(%|‰|‱)")
    });

    re::replace_all(&RE, text, "$1$3")
}

pub fn del_before_punctuation<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile("(^|[^!?:;,.…]) ([!?:;,])(?!\\))")
    });

    re::replace_all(&RE, text, "$1$2")
}

pub fn del_between_exclamation<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("([!?]) (?=[!?])"));

    re::replace_all(&RE, text, "$1")
}

pub fn del_leading_blanks<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile_m("^[ \\t]+"));

    re::replace_all(&RE, text, "")
}

pub fn del_repeat_n<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("\n{3,}"));

    re::replace_all(&RE, text, "\n\n")
}

pub fn del_repeat_space<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile("([^\\n \\t])[ \\t]{2,}(?![\\n \\t])")
    });

    re::replace_all(&RE, text, "$1 ")
}

pub fn del_trailing_blanks<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("[ \\t]+\\n"));

    re::replace_all(&RE, text, "\n")
}

pub fn insert_final_newline<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    if text.ends_with('\n') {
        return Cow::Borrowed(text);
    }

    Cow::Owned(format!("{text}\n"))
}

pub fn replace_tab<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    if !text.contains('\t') {
        return Cow::Borrowed(text);
    }

    Cow::Owned(text.replace('\t', "    "))
}

pub fn trim_left<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    let trimmed = re::js_trim_start(text);

    if trimmed.len() == text.len() {
        return Cow::Borrowed(text);
    }

    Cow::Owned(trimmed.to_string())
}

pub fn trim_right<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    let trimmed = re::js_trim_end(text);

    if trimmed.len() == text.len() {
        return Cow::Borrowed(text);
    }

    Cow::Owned(trimmed.to_string())
}
