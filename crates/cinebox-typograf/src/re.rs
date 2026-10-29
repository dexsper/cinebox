use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, RwLock};

use fancy_regex::{Captures, Regex};

pub(crate) fn compile(pat: &str) -> Regex {
    Regex::new(pat).unwrap_or_else(|error| panic!("typograf: invalid regex {error} in {pat:?}"))
}

pub(crate) fn compile_i(pat: &str) -> Regex {
    compile(&format!("(?i){pat}"))
}

pub(crate) fn compile_m(pat: &str) -> Regex {
    compile(&format!("(?m){pat}"))
}

pub(crate) fn compile_im(pat: &str) -> Regex {
    compile(&format!("(?im){pat}"))
}

pub(crate) fn compile_is(pat: &str) -> Regex {
    compile(&format!("(?is){pat}"))
}

static CACHE: LazyLock<RwLock<HashMap<String, Arc<Regex>>>> = LazyLock::new(|| RwLock::new(HashMap::new()));

/// Compile-once cache for patterns built at runtime (locale-dependent, etc.).
pub(crate) fn cached(pat: &str) -> Arc<Regex> {
    let read = CACHE.read().unwrap_or_else(std::sync::PoisonError::into_inner);

    if let Some(re) = read.get(pat) {
        return Arc::clone(re);
    }

    drop(read);

    let re = Arc::new(compile(pat));
    let mut write = CACHE.write().unwrap_or_else(std::sync::PoisonError::into_inner);

    Arc::clone(write.entry(pat.to_string()).or_insert(re))
}

pub(crate) fn cached_i(pat: &str) -> Arc<Regex> {
    cached(&format!("(?i){pat}"))
}

pub(crate) fn cached_m(pat: &str) -> Arc<Regex> {
    cached(&format!("(?m){pat}"))
}

pub(crate) fn cached_im(pat: &str) -> Arc<Regex> {
    cached(&format!("(?im){pat}"))
}

pub(crate) fn cached_s(pat: &str) -> Arc<Regex> {
    cached(&format!("(?s){pat}"))
}

pub(crate) fn replace_all<'a>(re: &Regex, text: &'a str, rep: &str) -> Cow<'a, str> {
    replace_all_fn(re, text, |caps| expand_js(rep, caps))
}

pub(crate) fn replace_first<'a>(re: &Regex, text: &'a str, rep: &str) -> Cow<'a, str> {
    re.replace(text, |caps: &Captures<'_, str>| expand_js(rep, caps))
}

pub(crate) fn replace_all_fn<'a>(
    re: &Regex,
    text: &'a str,
    mut replacer: impl FnMut(&Captures<'_, str>) -> String,
) -> Cow<'a, str> {
    re.replace_all(text, |caps: &Captures<'_, str>| replacer(caps))
}

/// Continue a replace chain without re-allocating when nothing matches.
pub(crate) fn chain<'a>(re: &Regex, input: Cow<'a, str>, rep: &str) -> Cow<'a, str> {
    chain_fn(re, input, |caps| expand_js(rep, caps))
}

pub(crate) fn chain_fn<'a>(
    re: &Regex,
    input: Cow<'a, str>,
    replacer: impl FnMut(&Captures<'_, str>) -> String,
) -> Cow<'a, str> {
    match input {
        Cow::Borrowed(s) => replace_all_fn(re, s, replacer),
        Cow::Owned(s) => {
            let rewritten = match replace_all_fn(re, &s, replacer) {
                Cow::Borrowed(_) => None,
                Cow::Owned(out) => Some(out),
            };

            Cow::Owned(rewritten.unwrap_or(s))
        }
    }
}

fn expand_js(template: &str, caps: &Captures<'_, str>) -> String {
    let mut out = String::new();
    let chars: Vec<char> = template.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] != '$' {
            out.push(chars[i]);
            i += 1;
            continue;
        }

        if chars.get(i + 1) == Some(&'$') {
            out.push('$');
            i += 2;
            continue;
        }

        let Some(d1) = chars.get(i + 1).copied().filter(char::is_ascii_digit) else {
            out.push('$');
            i += 1;
            continue;
        };

        let n1 = d1.to_digit(10).unwrap_or(0) as usize;
        let two = chars.get(i + 2).copied().filter(char::is_ascii_digit);

        if let Some(d2) = two {
            let n2 = n1 * 10 + d2.to_digit(10).unwrap_or(0) as usize;

            if caps.get(n2).is_some() {
                out.push_str(caps.get(n2).map_or("", |m| m.as_str()));
                i += 3;
                continue;
            }
        }

        out.push_str(caps.get(n1).map_or("", |m| m.as_str()));
        i += 2;
    }

    out
}

pub(crate) fn is_digit_char(ch: &str) -> bool {
    ch.len() == 1 && ch.as_bytes()[0].is_ascii_digit()
}

/// Unicode whitespace plus BOM.
pub(crate) fn js_trim_start(text: &str) -> &str {
    text.trim_start_matches(|c: char| c.is_whitespace() || c == '\u{FEFF}')
}

pub(crate) fn js_trim_end(text: &str) -> &str {
    text.trim_end_matches(|c: char| c.is_whitespace() || c == '\u{FEFF}')
}

pub(crate) fn js_trim(text: &str) -> &str {
    js_trim_end(js_trim_start(text))
}

pub(crate) fn is_match(re: &Regex, text: &str) -> bool {
    re.is_match(text).unwrap_or(false)
}

pub(crate) fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());

    for ch in text.chars() {
        if r"\.+*?()|[]{}^$".contains(ch) {
            out.push('\\');
        }

        out.push(ch);
    }

    out
}
