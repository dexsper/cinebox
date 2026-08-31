use std::sync::LazyLock;

use crate::engine::{Context, Typograf};
use crate::re;

const BLOCK: &[&str] = &[
    "address", "article", "aside", "blockquote", "canvas", "dd", "div", "dl", "fieldset",
    "figcaption", "figure", "footer", "form", "h1", "h2", "h3", "h4", "h5", "h6", "header",
    "hgroup", "hr", "li", "main", "nav", "noscript", "ol", "output", "p", "pre", "section",
    "table", "tfoot", "ul", "video",
];

pub fn e_mail(_tp: &Typograf, text: &str, ctx: &Context<'_>) -> String {
    if ctx.is_html {
        return text.to_string();
    }

    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile_i("(^|[\\s;(])([\\w\\-.]{2,64})@([\\w\\-.]{2,64})\\.([a-z]{2,64})([)\\s.,!?]|$)")
    });

    re::replace_all(&RE, text, "$1<a href=\"mailto:$2@$3.$4\">$2@$3.$4</a>$5")
}

pub fn escape(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
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

    out
}

pub fn nbr(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("([^\\n>])\\n(?=[^\\n])"));

    re::replace_all(&RE, text, "$1<br/>\n")
}

pub fn p(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let block_re = re::compile_i(&format!("<({})[>\\s]", BLOCK.join("|")));
    let mut buffer: Vec<String> = text.split("\n\n").map(str::to_string).collect();

    for chunk in &mut buffer {
        if re::js_trim(chunk).is_empty() {
            continue;
        }

        if re::is_match(&block_re, chunk) {
            continue;
        }

        let opened = re::compile("^(\\s*)");
        let closed = re::compile("(\\s*)$");
        *chunk = re::replace_all(&opened, chunk, "$1<p>");
        *chunk = re::replace_all(&closed, chunk, "</p>$1");
    }

    buffer.join("\n\n")
}

pub fn processing_attrs(tp: &Typograf, text: &str, ctx: &Context<'_>) -> String {
    let re_tag = re::compile("(<[-\\w]+\\s)([^>]+?)(?=>)");
    let re_attrs = re::compile_i("(^|\\s)(title|placeholder)=(\"[^\"]*?\"|'[^']*?')");

    re::replace_all_fn(&re_tag, text, |caps| {
        let tag_name = &caps[1];
        let attrs = &caps[2];
        let result_attrs = re::replace_all_fn(&re_attrs, attrs, |ac| {
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

pub fn quot(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("&quot;"));

    re::replace_all(&RE, text, "\"")
}

pub fn strip_tags(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("<[^>]+>"));

    re::replace_all(&RE, text, "")
}

pub fn url(_tp: &Typograf, text: &str, ctx: &Context<'_>) -> String {
    if ctx.is_html {
        return text.to_string();
    }

    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile("(https?|file|ftp)://([a-zA-Z0-9/+-=%&:_.~?]+[a-zA-Z0-9#+]*)")
    });

    re::replace_all_fn(&RE, text, |caps| {
        let protocol = &caps[1];
        let mut path = caps[2].to_string();
        path = re::replace_all(&re::compile("([^/]+/?)(\\?|#)$"), &path, "$1");
        path = re::replace_all(&re::compile("^([^/]+)/$"), &path, "$1");

        if protocol == "http" {
            path = re::replace_all(&re::compile("^([^/]+)(:80)([^\\d]|/|$)"), &path, "$1$3");
        } else if protocol == "https" {
            path = re::replace_all(&re::compile("^([^/]+)(:443)([^\\d]|/|$)"), &path, "$1$3");
        }

        let full_url = format!("{protocol}://{path}");
        let first = format!("<a href=\"{full_url}\">");

        if protocol == "http" || protocol == "https" {
            let mut url = re::replace_all(&re::compile("^www\\."), &path, "");

            if protocol != "http" {
                url = format!("{protocol}://{url}");
            }

            return format!("{first}{url}</a>");
        }

        format!("{first}{full_url}</a>")
    })
}
