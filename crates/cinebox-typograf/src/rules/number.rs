use std::sync::LazyLock;

use crate::engine::{Context, Typograf};
use crate::re;
use crate::PRIVATE;

pub fn digit_grouping(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let space = "\u{202F}";
    let grouped = re::compile_m(&format!(
        "(^ ?|\\D |{PRIVATE})(\\d{{1,3}}([ \u{00A0}\u{202F}\u{2009}]\\d{{3}})+)(?! ?[\\d-])"
    ));
    let step = re::replace_all_fn(&grouped, text, |caps| {
        let rest = re::replace_all(&re::compile("\\s"), &caps[2], space);

        format!("{}{rest}", &caps[1])
    });

    let long = re::compile("(\\d{5,}([.,]\\d+)?)");

    re::replace_all_fn(&long, &step, |caps| {
        let whole = &caps[1];
        let marker = whole.find(['.', ',']);
        let (integer, frac) = match marker {
            Some(i) => (&whole[..i], Some(&whole[i..])),
            None => (whole, None),
        };
        let grouped_int = re::replace_all(
            &re::compile("(\\d)(?=(\\d{3})+([^\\d]|$))"),
            integer,
            &format!("$1{space}"),
        );

        match frac {
            Some(f) => format!("{grouped_int}{f}"),
            None => grouped_int,
        }
    })
}

pub fn fraction(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static HALF: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(^|\\D)1/2(\\D|$)"));
    static QUARTER: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(^|\\D)1/4(\\D|$)"));
    static THREE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(^|\\D)3/4(\\D|$)"));
    let a = re::replace_all(&HALF, text, "$1½$2");
    let b = re::replace_all(&QUARTER, &a, "$1¼$2");

    re::replace_all(&THREE, &b, "$1¾$2")
}

pub fn math_signs(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static NE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("!="));
    static LE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("<="));
    static GE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(^|[^=])>="));
    static IFF: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("<=>"));
    static LL: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("<<"));
    static GG: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile(">>"));
    static CONG: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("~="));
    static PM: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(^|[^+])\\+-"));

    let mut out = re::replace_all(&NE, text, "≠");
    out = re::replace_all(&LE, &out, "≤");
    out = re::replace_all(&GE, &out, "$1≥");
    out = re::replace_all(&IFF, &out, "⇔");
    out = re::replace_all(&LL, &out, "≪");
    out = re::replace_all(&GG, &out, "≫");
    out = re::replace_all(&CONG, &out, "≅");

    re::replace_all(&PM, &out, "$1±")
}

pub fn times(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile("(\\d)[ \u{00A0}]?[xх][ \u{00A0}]?(\\d)")
    });

    re::replace_all(&RE, text, "$1×$2")
}
