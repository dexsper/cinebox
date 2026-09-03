use std::borrow::Cow;

use crate::data::{self, QuoteData};
use crate::engine::Context;
use crate::re;
use crate::PRIVATE;

const MAX_LEVEL_WITH_ERRORS: usize = 2;
const BUFFER_LEFT: &str = "\u{F005}\u{F006}\u{F007}";
const BUFFER_RIGHT: &str = "\u{F008}\u{F009}\u{F0A0}";
const BEFORE_LEFT: &str = " \n\t\u{00a0}\u{202f}[(";
const AFTER_RIGHT: &str = " \n\t\u{00a0}\u{202f}!?.:;#*,…)]";

#[derive(Clone, Copy)]
struct OwnedQuote {
    left: &'static str,
    right: &'static str,
    spacing: Option<usize>,
    remove_duplicate_quotes: bool,
}

impl From<&QuoteData> for OwnedQuote {
    fn from(q: &QuoteData) -> Self {
        Self {
            left: q.left,
            right: q.right,
            spacing: q.spacing,
            remove_duplicate_quotes: q.remove_duplicate_quotes,
        }
    }
}

pub fn process<'a>(text: &'a str, settings: &QuoteData, ctx: &Context<'_>) -> Cow<'a, str> {
    let count = count_quotes(text, Some(settings));

    if count.total == 0 {
        return Cow::Borrowed(text);
    }

    let original = OwnedQuote::from(settings);
    let equal = settings.left.chars().next() == settings.right.chars().next();

    let mut work = original;

    if equal {
        let n = settings.left.chars().count();
        work.left = take_prefix(BUFFER_LEFT, n);
        work.right = take_prefix(BUFFER_RIGHT, n);
    }

    let mut out = text.to_string();

    if work.spacing.is_some() {
        out = remove_spacing(&out, &work);
    }

    out = set_quotes(&out, &work, ctx);

    if work.spacing.is_some() {
        out = set_spacing(&out, &work);
    }

    if work.remove_duplicate_quotes {
        out = remove_duplicates(&out, &work);
    }

    if equal {
        out = return_original(&out, &original, &work);
    }

    Cow::Owned(out)
}

fn take_prefix(s: &'static str, n: usize) -> &'static str {
    let end = s.char_indices().nth(n).map(|(i, _)| i).unwrap_or(s.len());

    &s[..end]
}

struct Counts {
    total: usize,
    left0: usize,
    right0: usize,
}

fn all_quote_chars(settings: Option<&QuoteData>) -> String {
    let mut s = data::COMMON_QUOTE.to_string();

    if let Some(q) = settings {
        s.push_str(q.left);
        s.push_str(q.right);
    }

    s
}

fn count_quotes(text: &str, settings: Option<&QuoteData>) -> Counts {
    let quotes = all_quote_chars(settings);
    let class = char_class(&quotes);
    let re = re::cached(&format!("[{class}]"));
    let mut total = 0;
    let mut left0 = 0;
    let mut right0 = 0;
    let l0 = settings.and_then(|q| q.left.chars().next());
    let r0 = settings.and_then(|q| q.right.chars().next());

    for m in re.find_iter(text) {
        let Ok(m) = m else {
            continue;
        };
        total += 1;
        let ch = m.as_str().chars().next();

        if ch == l0 {
            left0 += 1;
        }

        if ch == r0 {
            right0 += 1;
        }
    }

    Counts {
        total,
        left0,
        right0,
    }
}

fn char_class(chars: &str) -> String {
    let mut out = String::new();

    for ch in chars.chars() {
        if matches!(ch, '-' | ']' | '\\' | '^' | '[') {
            out.push('\\');
        }

        out.push(ch);
    }

    out
}

fn remove_duplicates(text: &str, settings: &OwnedQuote) -> String {
    let lquote = first_char(settings.left);
    let lquote2 = settings.left.chars().nth(1).unwrap_or(lquote);
    let rquote = first_char(settings.right);

    if lquote != lquote2 {
        return text.to_string();
    }

    let doubled_l = format!("{lquote}{lquote}");
    let doubled_r = format!("{rquote}{rquote}");
    let step = text.replace(&doubled_l, &lquote.to_string());

    step.replace(&doubled_r, &rquote.to_string())
}

fn remove_spacing(text: &str, settings: &OwnedQuote) -> String {
    let mut out = text.to_string();
    let left: Vec<char> = settings.left.chars().collect();
    let right: Vec<char> = settings.right.chars().collect();
    let n = left.len().min(right.len());

    for i in 0..n {
        let lq = left[i];
        let rq = right[i];
        let re_l = re::cached(&format!("{lq}([ \u{202F}\u{00A0}])"));
        let re_r = re::cached(&format!("([ \u{202F}\u{00A0}]){rq}"));
        out = re::replace_all(&re_l, &out, &lq.to_string()).into_owned();
        out = re::replace_all(&re_r, &out, &rq.to_string()).into_owned();
    }

    out
}

fn set_spacing(text: &str, settings: &OwnedQuote) -> String {
    let mut out = text.to_string();
    let left: Vec<char> = settings.left.chars().collect();
    let right: Vec<char> = settings.right.chars().collect();
    let len = match settings.spacing {
        Some(usize::MAX) => left.len(),
        Some(n) => n,
        None => 0,
    };
    let n = len.min(left.len()).min(right.len());

    for i in 0..n {
        let lq = left[i];
        let rq = right[i];
        let re_l = re::cached(&format!("{lq}([^\u{202F}])"));
        let re_r = re::cached(&format!("([^\u{202F}]){rq}"));
        out = re::replace_all(&re_l, &out, &format!("{lq}\u{202F}$1")).into_owned();
        out = re::replace_all(&re_r, &out, &format!("$1\u{202F}{rq}")).into_owned();
    }

    out
}

fn set_quotes(text: &str, settings: &OwnedQuote, ctx: &Context<'_>) -> String {
    let quotes = {
        let mut s = data::COMMON_QUOTE.to_string();
        s.push_str(settings.left);
        s.push_str(settings.right);
        s
    };
    let qclass = char_class(&quotes);
    let lquote = first_char(settings.left);
    let lquote2 = settings.left.chars().nth(1).unwrap_or(lquote);
    let rquote = first_char(settings.right);
    let before = char_class(BEFORE_LEFT);
    let after = char_class(AFTER_RIGHT);

    let re_l = re::cached_im(&format!(
        "(^|[{before}])([{qclass}]+)(?=[^\\s{PRIVATE}])"
    ));
    let re_r = re::cached_im(&format!(
        "([^\\s{PRIVATE}])([{qclass}]+)(?=[{after}]|$)"
    ));

    let mut out = re::replace_all_fn(&re_l, text, |caps| {
        format!("{}{}", &caps[1], repeat_char(lquote, caps[2].chars().count()))
    })
    .into_owned();
    out = re::replace_all_fn(&re_r, &out, |caps| {
        format!("{}{}", &caps[1], repeat_char(rquote, caps[2].chars().count()))
    })
    .into_owned();

    out = set_above_tags(&out, settings, ctx, &qclass, lquote, rquote);

    if lquote != lquote2 {
        out = set_inner(&out, settings);
    }

    out
}

fn repeat_char(ch: char, n: usize) -> String {
    ch.to_string().repeat(n)
}

fn first_char(s: &str) -> char {
    s.chars().next().unwrap_or('"')
}

fn set_above_tags(
    text: &str,
    _settings: &OwnedQuote,
    ctx: &Context<'_>,
    qclass: &str,
    lquote: char,
    rquote: char,
) -> String {
    let re = re::cached_m(&format!("(^|.)([{qclass}])(.|$)"));
    let chars: Vec<(usize, char)> = text.char_indices().collect();

    let replaced = re::replace_all_fn(&re, text, |caps| {
        let original = &caps[0];
        let prev = &caps[1];
        let quote = &caps[2];
        let next = &caps[3];
        let pos = caps.get(0).map(|m| m.start()).unwrap_or(0);
        let prev_priv = prev == "\u{F000}";
        let next_priv = next == "\u{F000}";

        if !prev_priv && !next_priv {
            return original.to_string();
        }

        if prev_priv && next_priv {
            if quote == "\"" {
                let above = get_above_two_tags(&chars, text, pos + prev.len(), ctx, lquote, rquote);

                return format!("{prev}{above}{next}");
            }

            return original.to_string();
        }

        let tags = ctx.safe_tags.borrow();
        let quote_pos = pos + prev.len();

        if prev_priv {
            let has_right = AFTER_RIGHT.contains(next);
            let prev_info = tags.get_prev_tag_info(&chars, text, quote_pos);

            if has_right && prev_info.as_ref().is_some_and(|i| i.group == "html") {
                let q = if prev_info.is_some_and(|i| i.is_closing) {
                    rquote
                } else {
                    lquote
                };

                return format!("{prev}{q}{next}");
            }

            let q = if next.is_empty() || has_right {
                rquote
            } else {
                lquote
            };

            return format!("{prev}{q}{next}");
        }

        let has_left = BEFORE_LEFT.contains(prev);
        let next_info = tags.get_next_tag_info(&chars, text, quote_pos);

        if has_left && next_info.as_ref().is_some_and(|i| i.group == "html") {
            let q = if next_info.is_some_and(|i| i.is_closing) {
                rquote
            } else {
                lquote
            };

            return format!("{prev}{q}{next}");
        }

        let q = if prev.is_empty() || has_left {
            lquote
        } else {
            rquote
        };

        format!("{prev}{q}{next}")
    });

    replaced.into_owned()
}

fn get_above_two_tags(
    chars: &[(usize, char)],
    text: &str,
    pos: usize,
    ctx: &Context<'_>,
    lquote: char,
    rquote: char,
) -> String {
    let tags = ctx.safe_tags.borrow();
    let prev_info = tags.get_prev_tag_info(chars, text, pos);
    let next_info = tags.get_next_tag_info(chars, text, pos);

    if let Some(prev) = prev_info {
        if prev.group == "html" {
            if !prev.is_closing {
                return lquote.to_string();
            }

            if next_info.is_some_and(|n| n.is_closing) && prev.is_closing {
                return rquote.to_string();
            }
        }
    }

    text.get(pos..)
        .and_then(|s| s.chars().next())
        .map(|c| c.to_string())
        .unwrap_or_default()
}

fn set_inner(text: &str, settings: &OwnedQuote) -> String {
    let lquote = first_char(settings.left);
    let rquote = first_char(settings.right);
    let left_chars: Vec<char> = settings.left.chars().collect();
    let right_chars: Vec<char> = settings.right.chars().collect();
    let length = left_chars.len();
    let qd = QuoteData {
        left: settings.left,
        right: settings.right,
        spacing: None,
        remove_duplicate_quotes: false,
    };
    let counted = count_quotes(text, Some(&qd));
    let has_errors = counted.left0 != counted.right0;
    let max_level = if has_errors {
        length.min(MAX_LEVEL_WITH_ERRORS)
    } else {
        length
    };

    let mut level = 0usize;
    let mut result = String::new();

    for ch in text.chars() {
        if ch == lquote {
            let idx = level.min(max_level.saturating_sub(1));
            result.push(left_chars[idx]);
            level += 1;

            if has_errors && level > max_level {
                level = max_level;
            }

            continue;
        }

        if ch == rquote {
            level = level.saturating_sub(1);
            let idx = level.min(max_level.saturating_sub(1));
            result.push(right_chars[idx]);
            continue;
        }

        if ch == '"' {
            level = 0;
        }

        result.push(ch);
    }

    result
}

fn return_original(text: &str, original: &OwnedQuote, buffer: &OwnedQuote) -> String {
    let mut map = Vec::new();
    let bl: Vec<char> = buffer.left.chars().collect();
    let br: Vec<char> = buffer.right.chars().collect();
    let ol: Vec<char> = original.left.chars().collect();
    let or_: Vec<char> = original.right.chars().collect();

    for i in 0..bl.len().min(ol.len()) {
        map.push((bl[i], ol[i]));
    }

    for i in 0..br.len().min(or_.len()) {
        map.push((br[i], or_[i]));
    }

    text.chars()
        .map(|ch| map.iter().find(|(from, _)| *from == ch).map(|(_, to)| *to).unwrap_or(ch))
        .collect()
}
