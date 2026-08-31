use std::sync::LazyLock;

use crate::data;
use crate::engine::{Context, Typograf};
use crate::re;
use crate::PRIVATE;

pub fn after_colon(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile(&format!("(\\D):([^)\",:.?\\s/\\\\{PRIVATE}])"))
    });

    re::replace_all(&RE, text, "$1: $2")
}

pub fn after_comma(_tp: &Typograf, text: &str, ctx: &Context<'_>) -> String {
    let quotes = match ctx.quote() {
        Some(q) => q.right.to_string(),
        None => data::COMMON_QUOTE.to_string(),
    };
    let class = re::escape(&quotes);
    let re = re::compile(&format!("(.),([^)\",:.?\\s/\\\\{PRIVATE}{class}])"));

    re::replace_all_fn(&re, text, |caps| {
        if re::is_digit_char(&caps[1]) && re::is_digit_char(&caps[2]) {
            return caps[0].to_string();
        }

        format!("{}, {}", &caps[1], &caps[2])
    })
}

pub fn after_exclamation(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let re = re::compile(&format!(
        "!([^).…!;?\\s\\[\\]){PRIVATE}{}])",
        re::escape(data::COMMON_QUOTE)
    ));

    re::replace_all(&re, text, "! $1")
}

pub fn after_question(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let re = re::compile(&format!(
        "\\?([^).…!;?\\s\\[\\]){PRIVATE}{}])",
        re::escape(data::COMMON_QUOTE)
    ));

    re::replace_all(&re, text, "? $1")
}

pub fn after_semicolon(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let re = re::compile(&format!(
        ";([^).…!;?\\s\\[\\]){PRIVATE}{}])",
        re::escape(data::COMMON_QUOTE)
    ));

    re::replace_all(&re, text, "; $1")
}

pub fn before_bracket(_tp: &Typograf, text: &str, ctx: &Context<'_>) -> String {
    let char = ctx.chars();
    let re = re::compile_i(&format!("([{char}.!?,;…)])\\("));

    re::replace_all(&re, text, "$1 (")
}

pub fn bracket(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static OPEN: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(\\() +"));
    static CLOSE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile(" +\\)"));
    let step = re::replace_all(&OPEN, text, "(");

    re::replace_all(&CLOSE, &step, ")")
}

pub fn square_bracket(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static OPEN: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(\\[) +"));
    static CLOSE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile(" +\\]"));
    let step = re::replace_all(&OPEN, text, "[");

    re::replace_all(&CLOSE, &step, "]")
}

pub fn del_before_dot(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile("(^|[^!?:;,.…]) (\\.|\\.\\.\\.)(\\s|$)")
    });

    re::replace_all(&RE, text, "$1$2$3")
}

pub fn del_before_percent(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile("(\\d)( |\u{00A0})(%|‰|‱)")
    });

    re::replace_all(&RE, text, "$1$3")
}

pub fn del_before_punctuation(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile("(^|[^!?:;,.…]) ([!?:;,])(?!\\))")
    });

    re::replace_all(&RE, text, "$1$2")
}

pub fn del_between_exclamation(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("([!?]) (?=[!?])"));

    re::replace_all(&RE, text, "$1")
}

pub fn del_leading_blanks(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile_m("^[ \\t]+"));

    re::replace_all(&RE, text, "")
}

pub fn del_repeat_n(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("\n{3,}"));

    re::replace_all(&RE, text, "\n\n")
}

pub fn del_repeat_space(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile("([^\\n \\t])[ \\t]{2,}(?![\\n \\t])")
    });

    re::replace_all(&RE, text, "$1 ")
}

pub fn del_trailing_blanks(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("[ \\t]+\\n"));

    re::replace_all(&RE, text, "\n")
}

pub fn insert_final_newline(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    if text.ends_with('\n') {
        return text.to_string();
    }

    format!("{text}\n")
}

pub fn replace_tab(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    text.replace('\t', "    ")
}

pub fn trim_left(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    re::js_trim_start(text).to_string()
}

pub fn trim_right(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    re::js_trim_end(text).to_string()
}
