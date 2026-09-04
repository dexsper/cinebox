//! Split JS source on a delimiter, respecting quotes, regex, and brackets.

use std::collections::HashSet;
use std::sync::LazyLock;

use super::JsError;

pub(super) static OP_CHARS: LazyLock<HashSet<char>> = LazyLock::new(|| {
    let mut set = HashSet::from([';', ',', '[']);

    for op in ALL_OPS {
        if op.chars().any(|c| c.is_ascii_alphabetic()) {
            continue;
        }

        set.extend(op.chars());
    }

    set
});

pub(super) const ALL_OPS: &[&str] = &[
    "?", "??", "||", "&&", "|", "^", "&", "===", "!==", "==", "!=", "<=", ">=", "<", ">",
    ">>", "<<", "+", "-", "*", "%", "/", "**", "void", "typeof", "!",
];

const MATCHING: [(u8, u8); 3] = [(b'(', b')'), (b'{', b'}'), (b'[', b']')];
const QUOTES: &[u8] = b"'\"/";

pub(super) fn matching_close(open: char) -> Option<char> {
    match open {
        '(' => Some(')'),
        '{' => Some('}'),
        '[' => Some(']'),
        _ => None,
    }
}

pub(super) fn separate(
    expr: &str,
    delim: &str,
    max_split: Option<usize>,
    skip_delims: &[&str],
) -> Vec<String> {
    if expr.is_empty() {
        return Vec::new();
    }

    let bytes = expr.as_bytes();
    let delim_bytes = delim.as_bytes();
    let delim_len = delim_bytes.len().saturating_sub(1);

    let mut counters = [0i32; 3];
    let mut start = 0usize;
    let mut splits = 0usize;
    let mut pos = 0usize;
    let mut in_quote: Option<u8> = None;
    let mut escaping = false;
    let mut after_op = true;
    let mut in_regex_char_group = false;
    let mut skipping = 0i32;
    let mut skip_txt: Option<(usize, usize)> = None;
    let mut out = Vec::new();

    for (idx, &byte) in bytes.iter().enumerate() {
        if skip_txt.is_some_and(|(_, end)| idx <= end) {
            continue;
        }

        let mut paren_delta = 0i32;

        if in_quote.is_none() {
            let next_star = bytes.get(idx + 1).is_some_and(|&next| next == b'*');
            let is_block_comment = byte == b'/' && next_star;

            if is_block_comment {
                if let Some(rel) = find_close_comment(&bytes[idx..]) {
                    skip_txt = Some((idx, idx + rel));
                    continue;
                }
            }

            if let Some(slot) = open_slot(byte) {
                counters[slot] += 1;
                paren_delta = 1;
            }

            if let Some(slot) = close_slot(byte) {
                counters[slot] -= 1;
                paren_delta = -1;
            }
        }

        if !escaping {
            let is_quote = QUOTES.contains(&byte);
            let quote_match = in_quote.is_none_or(|q| q == byte);

            if is_quote && quote_match {
                let open_or_close = in_quote.is_some() || after_op || byte != b'/';

                if open_or_close {
                    let closing_quote = in_quote.is_some() && !in_regex_char_group;
                    in_quote = Some(byte);

                    if closing_quote {
                        in_quote = None;
                    }
                }
            }

            let handle_quote = is_quote && quote_match;
            let in_regex = in_quote == Some(b'/');
            let regex_class = byte == b'[' || byte == b']';

            if !handle_quote && in_regex && regex_class {
                in_regex_char_group = byte == b'[';
            }
        }

        escaping = !escaping && in_quote.is_some() && byte == b'\\';
        let after_ws = after_op && byte.is_ascii_whitespace();
        after_op = in_quote.is_none() && (is_op_byte(byte) || paren_delta > 0 || after_ws);

        let delim_hit = pos < delim_bytes.len() && byte == delim_bytes[pos];
        let blocked = !delim_hit || counters.iter().any(|c| *c != 0) || in_quote.is_some();

        if blocked {
            pos = 0;
            skipping = 0;
            continue;
        }

        if skipping > 0 {
            skipping -= 1;
            continue;
        }

        if pos == 0 && !skip_delims.is_empty() {
            let rest = &bytes[idx..];
            let mut skip_now = 0i32;

            for skip in skip_delims {
                if skip.is_empty() {
                    continue;
                }

                if rest.starts_with(skip.as_bytes()) {
                    skip_now = skip.len() as i32 - 1;
                    break;
                }
            }

            if skip_now > 0 {
                skipping = skip_now;
                continue;
            }
        }

        if pos < delim_len {
            pos += 1;
            continue;
        }

        let end = idx - delim_len;
        out.push(slice_skip_comment(bytes, start, end, skip_txt));
        skip_txt = None;
        start = idx + 1;
        pos = 0;
        splits += 1;

        if max_split.is_some_and(|m| splits >= m) {
            break;
        }
    }

    if let Some((s, e)) = skip_txt {
        if s >= start {
            out.push(slice_skip_comment(bytes, start, bytes.len(), Some((s, e))));
            return out;
        }
    }

    out.push(utf8(&bytes[start..]));
    out
}

fn is_op_byte(byte: u8) -> bool {
    byte.is_ascii() && OP_CHARS.contains(&(byte as char))
}

fn slice_skip_comment(
    bytes: &[u8],
    start: usize,
    end: usize,
    skip_txt: Option<(usize, usize)>,
) -> String {
    let Some((s, e)) = skip_txt else {
        return utf8(&bytes[start..end]);
    };

    if s >= start && e <= end {
        let mut piece = utf8(&bytes[start..s]);
        piece.push_str(std::str::from_utf8(&bytes[e + 1..end]).unwrap_or(""));
        return piece;
    }

    utf8(&bytes[start..end])
}

fn utf8(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_owned(),
        Err(_) => String::from_utf8_lossy(bytes).into_owned(),
    }
}

fn find_close_comment(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < 4 {
        return None;
    }

    let mut i = 2;

    while i < bytes.len() {
        let is_close = bytes[i] == b'*' && bytes.get(i + 1).is_some_and(|&c| c == b'/');

        if is_close {
            return Some(i + 1);
        }

        i += 1;
    }

    None
}

fn open_slot(byte: u8) -> Option<usize> {
    MATCHING.iter().position(|(open, _)| *open == byte)
}

fn close_slot(byte: u8) -> Option<usize> {
    MATCHING.iter().position(|(_, close)| *close == byte)
}

pub(super) fn separate_at_paren(
    expr: &str,
    delim: Option<char>,
) -> Result<(String, String), JsError> {
    let close = match delim {
        Some(c) => c,
        None => first_matching_close(expr)?,
    };

    let delim_s = close.to_string();
    let separated = separate(expr, &delim_s, Some(1), &[]);

    if separated.len() < 2 {
        return Err(JsError::msg("no terminating paren"));
    }

    let inner: String = separated[0].chars().skip(1).collect();

    Ok((inner.trim().to_owned(), separated[1].trim().to_owned()))
}

fn first_matching_close(expr: &str) -> Result<char, JsError> {
    let Some(first) = expr.chars().next() else {
        return Err(JsError::msg("no terminating paren"));
    };

    let Some(close) = matching_close(first) else {
        return Err(JsError::msg("no terminating paren"));
    };

    Ok(close)
}

pub(super) fn comma_split(expr: &str) -> Vec<String> {
    separate(expr, ",", None, &[])
}
