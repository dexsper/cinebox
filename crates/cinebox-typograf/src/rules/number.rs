use std::borrow::Cow;
use std::sync::LazyLock;

use crate::engine::{Context, Typograf};
use crate::re;
use crate::PRIVATE;

pub fn digit_grouping<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static GROUPED: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile_m(&format!(
            "(^ ?|\\D |{PRIVATE})(\\d{{1,3}}([ \u{00A0}\u{202F}\u{2009}]\\d{{3}})+)(?! ?[\\d-])"
        ))
    });
    
    static SPACE_RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("\\s"));
    static LONG: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(\\d{5,}([.,]\\d+)?)"));
    static INT_RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile("(\\d)(?=(\\d{3})+([^\\d]|$))")
    });

    let space = "\u{202F}";
    let step = re::replace_all_fn(&GROUPED, text, |caps| {
        let rest = re::replace_all(&SPACE_RE, &caps[2], space);

        format!("{}{rest}", &caps[1])
    });

    re::chain_fn(&LONG, step, |caps| {
        let whole = &caps[1];
        let marker = whole.find(['.', ',']);
        let (integer, frac) = match marker {
            Some(i) => (&whole[..i], Some(&whole[i..])),
            None => (whole, None),
        };
        let grouped_int = re::replace_all(&INT_RE, integer, &format!("$1{space}"));

        match frac {
            Some(f) => format!("{grouped_int}{f}"),
            None => grouped_int.into_owned(),
        }
    })
}

pub fn fraction<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static HALF: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(^|\\D)1/2(\\D|$)"));
    static QUARTER: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(^|\\D)1/4(\\D|$)"));
    static THREE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(^|\\D)3/4(\\D|$)"));
    let a = re::replace_all(&HALF, text, "$1½$2");
    let b = re::chain(&QUARTER, a, "$1¼$2");

    re::chain(&THREE, b, "$1¾$2")
}

pub fn math_signs<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static NE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("!="));
    static LE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("<="));
    static GE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(^|[^=])>="));
    static IFF: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("<=>"));
    static LL: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("<<"));
    static GG: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile(">>"));
    static CONG: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("~="));
    static PM: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(^|[^+])\\+-"));

    let mut out = re::replace_all(&NE, text, "≠");
    out = re::chain(&LE, out, "≤");
    out = re::chain(&GE, out, "$1≥");
    out = re::chain(&IFF, out, "⇔");
    out = re::chain(&LL, out, "≪");
    out = re::chain(&GG, out, "≫");
    out = re::chain(&CONG, out, "≅");

    re::chain(&PM, out, "$1±")
}

pub fn times<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile("(\\d)[ \u{00A0}]?[xх][ \u{00A0}]?(\\d)")
    });

    re::replace_all(&RE, text, "$1×$2")
}
