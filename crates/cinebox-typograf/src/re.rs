use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, RwLock};

use fancy_regex::{Captures, Regex};

pub fn compile(pat: &str) -> Regex {
    Regex::new(pat).unwrap_or_else(|e| panic!("typograf: invalid regex {e} in {pat:?}"))
}

pub fn compile_i(pat: &str) -> Regex {
    compile(&format!("(?i){pat}"))
}

pub fn compile_m(pat: &str) -> Regex {
    compile(&format!("(?m){pat}"))
}

pub fn compile_im(pat: &str) -> Regex {
    compile(&format!("(?im){pat}"))
}

pub fn compile_is(pat: &str) -> Regex {
    compile(&format!("(?is){pat}"))
}

static CACHE: LazyLock<RwLock<HashMap<String, Arc<Regex>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Compile-once cache for patterns built at runtime (locale-dependent, etc.).
pub fn cached(pat: &str) -> Arc<Regex> {
    let read = CACHE.read().unwrap_or_else(|e| e.into_inner());

    if let Some(re) = read.get(pat) {
        return Arc::clone(re);
    }

    drop(read);

    let re = Arc::new(compile(pat));
    let mut write = CACHE.write().unwrap_or_else(|e| e.into_inner());

    Arc::clone(write.entry(pat.to_string()).or_insert(re))
}

pub fn cached_i(pat: &str) -> Arc<Regex> {
    cached(&format!("(?i){pat}"))
}

pub fn cached_m(pat: &str) -> Arc<Regex> {
    cached(&format!("(?m){pat}"))
}

pub fn cached_im(pat: &str) -> Arc<Regex> {
    cached(&format!("(?im){pat}"))
}

pub fn cached_s(pat: &str) -> Arc<Regex> {
    cached(&format!("(?s){pat}"))
}

pub fn replace_all<'a>(re: &Regex, text: &'a str, rep: &str) -> Cow<'a, str> {
    replace_all_fn(re, text, |caps| expand_js(rep, caps))
}

pub fn replace_first<'a>(re: &Regex, text: &'a str, rep: &str) -> Cow<'a, str> {
    re.replace(text, |caps: &Captures<'_, str>| expand_js(rep, caps))
}

pub fn replace_all_fn<'a>(
    re: &Regex,
    text: &'a str,
    mut replacer: impl FnMut(&Captures<'_, str>) -> String,
) -> Cow<'a, str> {
    re.replace_all(text, |caps: &Captures<'_, str>| replacer(caps))
}

/// Continue a replace chain without re-allocating when nothing matches.
pub fn chain<'a>(re: &Regex, input: Cow<'a, str>, rep: &str) -> Cow<'a, str> {
    chain_fn(re, input, |caps| expand_js(rep, caps))
}

pub fn chain_fn<'a>(
    re: &Regex,
    input: Cow<'a, str>,
    replacer: impl FnMut(&Captures<'_, str>) -> String,
) -> Cow<'a, str> {
    match input {
        Cow::Borrowed(s) => replace_all_fn(re, s, replacer),
        Cow::Owned(s) => {
            let replaced = match replace_all_fn(re, &s, replacer) {
                Cow::Borrowed(_) => None,
                Cow::Owned(out) => Some(out),
            };

            Cow::Owned(replaced.unwrap_or(s))
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

        let Some(d1) = chars.get(i + 1).copied().filter(|c| c.is_ascii_digit()) else {
            out.push('$');
            i += 1;
            continue;
        };

        let n1 = d1.to_digit(10).unwrap_or(0) as usize;
        let two = chars.get(i + 2).copied().filter(|c| c.is_ascii_digit());

        if let Some(d2) = two {
            let n2 = n1 * 10 + d2.to_digit(10).unwrap_or(0) as usize;

            if caps.get(n2).is_some() {
                out.push_str(caps.get(n2).map(|m| m.as_str()).unwrap_or(""));
                i += 3;
                continue;
            }
        }

        out.push_str(caps.get(n1).map(|m| m.as_str()).unwrap_or(""));
        i += 2;
    }

    out
}

pub fn is_digit_char(ch: &str) -> bool {
    ch.len() == 1 && ch.as_bytes()[0].is_ascii_digit()
}

/// Unicode whitespace plus BOM.
pub fn js_trim_start(text: &str) -> &str {
    text.trim_start_matches(|c: char| c.is_whitespace() || c == '\u{FEFF}')
}

pub fn js_trim_end(text: &str) -> &str {
    text.trim_end_matches(|c: char| c.is_whitespace() || c == '\u{FEFF}')
}

pub fn js_trim(text: &str) -> &str {
    js_trim_end(js_trim_start(text))
}

pub fn is_match(re: &Regex, text: &str) -> bool {
    re.is_match(text).unwrap_or(false)
}

pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());

    for ch in text.chars() {
        if r"\.+*?()|[]{}^$".contains(ch) {
            out.push('\\');
        }

        out.push(ch);
    }

    out
}
