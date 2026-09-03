use std::borrow::Cow;
use std::sync::LazyLock;

use crate::engine::{Context, Typograf};
use crate::re;

const BLOCK: &[&str] = &[
    "address", "article", "aside", "blockquote", "canvas", "dd", "div", "dl", "fieldset",
    "figcaption", "figure", "footer", "form", "h1", "h2", "h3", "h4", "h5", "h6", "header",
    "hgroup", "hr", "li", "main", "nav", "noscript", "ol", "output", "p", "pre", "section",
    "table", "tfoot", "ul", "video",
];

pub fn e_mail<'a>(_tp: &Typograf, text: &'a str, ctx: &Context<'_>) -> Cow<'a, str> {
    if ctx.is_html {
        return Cow::Borrowed(text);
    }

    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile_i("(^|[\\s;(])([\\w\\-.]{2,64})@([\\w\\-.]{2,64})\\.([a-z]{2,64})([)\\s.,!?]|$)")
    });

    re::replace_all(&RE, text, "$1<a href=\"mailto:$2@$3.$4\">$2@$3.$4</a>$5")
}

pub fn escape<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    let needs_escape = text.contains(['&', '<', '>', '"', '\'', '/']);

    if !needs_escape {
        return Cow::Borrowed(text);
    }

    let mut out = String::with_capacity(text.len());

    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            '/' => out.push_str("&#x2F;"),
            _ => out.push(ch),
        }
    }

    Cow::Owned(out)
}

pub fn nbr<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("([^\\n>])\\n(?=[^\\n])"));

    re::replace_all(&RE, text, "$1<br/>\n")
}

pub fn p<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static BLOCK_RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile_i(&format!("<({})[>\\s]", BLOCK.join("|")))
    });
    static OPENED: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("^(\\s*)"));
    static CLOSED: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(\\s*)$"));

    let mut buffer: Vec<String> = text.split("\n\n").map(str::to_string).collect();

    for chunk in &mut buffer {
        if re::js_trim(chunk).is_empty() {
            continue;
        }

        if re::is_match(&BLOCK_RE, chunk) {
            continue;
        }

        *chunk = re::replace_all(&OPENED, chunk, "$1<p>").into_owned();
        *chunk = re::replace_all(&CLOSED, chunk, "</p>$1").into_owned();
    }

    Cow::Owned(buffer.join("\n\n"))
}

pub fn processing_attrs<'a>(tp: &Typograf, text: &'a str, ctx: &Context<'_>) -> Cow<'a, str> {
    static RE_TAG: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile("(<[-\\w]+\\s)([^>]+?)(?=>)")
    });
    static RE_ATTRS: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile_i("(^|\\s)(title|placeholder)=(\"[^\"]*?\"|'[^']*?')")
    });

    re::replace_all_fn(&RE_TAG, text, |caps| {
        let tag_name = &caps[1];
        let attrs = &caps[2];
        let result_attrs = re::replace_all_fn(&RE_ATTRS, attrs, |ac| {
            let space = &ac[1];
            let attr_name = &ac[2];
            let attr_value = &ac[3];
            let mut chars = attr_value.chars();
            let lquote = chars.next().unwrap_or('"');
            let inner: String = {
                let v: Vec<char> = attr_value.chars().collect();

                if v.len() < 2 {
                    String::new()
                } else {
                    v[1..v.len() - 1].iter().collect()
                }
            };
            let rquote = attr_value.chars().last().unwrap_or('"');
            let nested = tp.execute_nested(&inner, ctx);

            format!("{space}{attr_name}={lquote}{nested}{rquote}")
        });

        format!("{tag_name}{result_attrs}")
    })
}

pub fn quot<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    if !text.contains("&quot;") {
        return Cow::Borrowed(text);
    }

    Cow::Owned(text.replace("&quot;", "\""))
}

pub fn strip_tags<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("<[^>]+>"));

    re::replace_all(&RE, text, "")
}

pub fn url<'a>(_tp: &Typograf, text: &'a str, ctx: &Context<'_>) -> Cow<'a, str> {
    if ctx.is_html {
        return Cow::Borrowed(text);
    }

    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile("(https?|file|ftp)://([a-zA-Z0-9/+-=%&:_.~?]+[a-zA-Z0-9#+]*)")
    });
    
    static TRAIL: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("([^/]+/?)(\\?|#)$"));
    static ROOT: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("^([^/]+)/$"));
    static PORT80: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("^([^/]+)(:80)([^\\d]|/|$)"));
    static PORT443: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("^([^/]+)(:443)([^\\d]|/|$)"));
    static WWW: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("^www\\."));

    re::replace_all_fn(&RE, text, |caps| {
        let protocol = &caps[1];
        let mut path = caps[2].to_string();
        path = re::replace_all(&TRAIL, &path, "$1").into_owned();
        path = re::replace_all(&ROOT, &path, "$1").into_owned();

        if protocol == "http" {
            path = re::replace_all(&PORT80, &path, "$1$3").into_owned();
        } else if protocol == "https" {
            path = re::replace_all(&PORT443, &path, "$1$3").into_owned();
        }

        let full_url = format!("{protocol}://{path}");
        let first = format!("<a href=\"{full_url}\">");

        if protocol == "http" || protocol == "https" {
            let mut url = re::replace_all(&WWW, &path, "").into_owned();

            if protocol != "http" {
                url = format!("{protocol}://{url}");
            }

            return format!("{first}{url}</a>");
        }

        format!("{first}{full_url}</a>")
    })
}
