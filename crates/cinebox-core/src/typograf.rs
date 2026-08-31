//! Title-focused subset of [Typograf](https://github.com/typograf/typograf) for `ru` + `en-US`.
//!
//! Goal for tiles: wrap on real word boundaries. Non-breaking spaces after
//! prepositions/articles keep “в начале” / “of the” from splitting.

const NBSP: char = '\u{00A0}';
const MDASH: char = '—';
const NDASH: char = '–';
const ELLIPSIS: char = '…';

/// Apply typography suitable for a catalog title.
#[must_use]
pub fn typograph(input: &str) -> String {
    let decoded = decode_entities(input);
    let mut text = collapse_whitespace(&decoded);
    if text.is_empty() {
        return text;
    }

    text = replace_ellipsis(&text);
    text = replace_number_ranges(&text);
    text = replace_spaced_hyphen_with_mdash(&text);
    text = squeeze_space_before_punct(&text);
    text = educate_quotes(&text);

    glue_short_words(&text)
}

const ENTITY_MAX: usize = 10;

fn decode_entities(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;

    while i < chars.len() {
        if chars[i] != '&' {
            out.push(chars[i]);
            i += 1;
            continue;
        }

        let Some((ch, next)) = entity_at(&chars, i) else {
            out.push('&');
            i += 1;
            continue;
        };

        out.push(ch);
        i = next;
    }

    out
}

fn entity_at(chars: &[char], at: usize) -> Option<(char, usize)> {
    let start = at + 1;
    let limit = chars.len().min(start + ENTITY_MAX);
    let mut i = start;
    while i < limit && chars[i] != ';' {
        i += 1;
    }

    let found_semi = chars.get(i).is_some_and(|ch| *ch == ';');
    if !found_semi {
        return None;
    }

    if i == start {
        return None;
    }

    let body: String = chars[start..i].iter().collect();
    let ch = decode_body(&body)?;

    Some((ch, i + 1))
}

fn decode_body(body: &str) -> Option<char> {
    if let Some(digits) = body.strip_prefix('#') {
        return numeric_entity(digits);
    }

    named_entity(body)
}

fn named_entity(name: &str) -> Option<char> {
    let name = name.to_ascii_lowercase();
    match name.as_str() {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some('\u{00A0}'),
        _ => None,
    }
}

fn numeric_entity(body: &str) -> Option<char> {
    if let Some(hex) = body.strip_prefix(['x', 'X']) {
        return codepoint(hex, 16);
    }

    codepoint(body, 10)
}

fn codepoint(digits: &str, radix: u32) -> Option<char> {
    let code = u32::from_str_radix(digits, radix).ok()?;

    char::from_u32(code)
}

fn collapse_whitespace(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_space = false;

    for ch in input.chars() {
        if !ch.is_whitespace() {
            out.push(ch);
            prev_space = false;
            continue;
        }

        if prev_space || out.is_empty() {
            continue;
        }

        out.push(' ');
        prev_space = true;
    }

    out
}

fn replace_ellipsis(text: &str) -> String {
    text.replace("...", "…")
}

fn replace_number_ranges(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());

    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let start = i;
            while chars.get(i).is_some_and(|ch| ch.is_ascii_digit()) {
                i += 1;
            }

            let left: String = chars[start..i].iter().collect();
            let current_is_dash = chars.get(i).is_some_and(|ch| *ch == '-');
            let next_is_digit = chars.get(i + 1).is_some_and(char::is_ascii_digit);

            if current_is_dash && next_is_digit {
                i += 1;
                let right_start = i;

            while chars.get(i).is_some_and(|ch| ch.is_ascii_digit()) {
                i += 1;
            }

            let right: String = chars[right_start..i].iter().collect();
                out.push_str(&left);
                out.push(NDASH);
                out.push_str(&right);

                continue;
            }

            out.push_str(&left);
            continue;
        }

        out.push(chars[i]);
        i += 1;
    }

    out
}

fn replace_spaced_hyphen_with_mdash(text: &str) -> String {
    text.replace(" -- ", &format!("{NBSP}{MDASH} "))
        .replace(" - ", &format!("{NBSP}{MDASH} "))
        .replace(" – ", &format!("{NBSP}{MDASH} "))
        .replace(" — ", &format!("{NBSP}{MDASH} "))
}

fn squeeze_space_before_punct(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();

    for (i, ch) in chars.iter().copied().enumerate() {
        let is_space = ch == ' ';
        let next_is_punctuation = chars
            .get(i + 1)
            .is_some_and(|ch| matches!(*ch, ',' | '.' | ':' | ';' | '!' | '?' | ELLIPSIS));

        if is_space && next_is_punctuation {
            continue;
        }

        out.push(ch);
    }
    out
}

fn has_cyrillic(text: &str) -> bool {
    text.chars()
        .any(|ch| matches!(ch, '\u{0400}'..='\u{04FF}' | '\u{0500}'..='\u{052F}'))
}

fn educate_quotes(text: &str) -> String {
    let ru = has_cyrillic(text);
    let (open, close) = if ru {
        ('«', '»')
    } else {
        ('\u{201C}', '\u{201D}')
    };

    let mut out = String::with_capacity(text.len());
    let mut opening = true;

    for ch in text.chars() {
        if ch == '"' {
            out.push(if opening { open } else { close });
            opening = !opening;

            continue;
        }

        out.push(ch);
    }

    out
}

fn glue_short_words(text: &str) -> String {
    let parts: Vec<&str> = text.split(' ').filter(|part| !part.is_empty()).collect();
    if parts.is_empty() {
        return String::new();
    }

    let mut out = String::with_capacity(text.len());
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            out.push_str(part);
            continue;
        }

        let prev = parts[i - 1];
        let last = i + 1 == parts.len();
        let needs_nbsp = is_short_word(prev) || (last && is_short_word(part));

        out.push(if needs_nbsp { NBSP } else { ' ' });
        out.push_str(part);
    }

    out
}

fn is_short_word(token: &str) -> bool {
    let core = word_core(token);
    if core.is_empty() {
        return false;
    }

    if is_listed_short(&core) {
        return true;
    }

    core.chars().all(char::is_alphabetic) && core.chars().count() <= 2
}

fn word_core(token: &str) -> String {
    token
        .chars()
        .filter(|ch| ch.is_alphabetic() || *ch == '\'')
        .collect::<String>()
        .to_lowercase()
}

fn is_listed_short(word: &str) -> bool {
    matches!(
        word,
        // ru (typograf `ru/shortWord` + particles)
        "а" | "без" | "в" | "во" | "если" | "да" | "до" | "для" | "за" | "и" | "или"
            | "из" | "к" | "ко" | "как" | "ли" | "на" | "но" | "не" | "ни" | "о"
            | "об" | "обо" | "от" | "по" | "про" | "при" | "под" | "с" | "со"
            | "то" | "у" | "же" | "бы" | "ль"
            // en-US (typograf `en-US/shortWord`)
            | "a" | "an" | "and" | "as" | "at" | "bar" | "but" | "by" | "for"
            | "if" | "in" | "nor" | "not" | "of" | "off" | "on" | "or" | "out"
            | "per" | "pro" | "so" | "the" | "to" | "up" | "via" | "yet"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn russian_preposition_nbsp_and_guillemets() {
        let out = typograph("в начале \"Дюны\"");

        assert!(out.contains("в\u{00A0}начале"), "{out:?}");
        assert!(out.contains('«') && out.contains('»'), "{out:?}");
        assert!(!out.contains('"'), "{out:?}");
    }

    #[test]
    fn english_article_nbsp_and_quotes() {
        let out = typograph("The \"Matrix\"");

        assert!(out.starts_with("The\u{00A0}"), "{out:?}");
        assert!(
            out.contains('\u{201C}') && out.contains('\u{201D}'),
            "{out:?}"
        );
    }

    #[test]
    fn dash_range_ellipsis_and_idempotent() {
        let out = typograph("A - B 2019-2020 wait...");

        assert!(out.contains(&format!("A{NBSP}{MDASH}")), "{out:?}");
        assert!(out.contains("2019–2020"), "{out:?}");
        assert!(out.contains(ELLIPSIS), "{out:?}");
        assert_eq!(typograph(&out), out);
    }

    #[test]
    fn last_short_word_glued() {
        let out = typograph("Catch Me");

        assert_eq!(out, "Catch\u{00A0}Me");
    }

    #[test]
    fn html_entities_from_api() {
        let amp = typograph("Tom &amp; Jerry");
        assert!(amp.contains("Tom & Jerry"), "{amp:?}");
        assert!(!amp.contains("&amp;"), "{amp:?}");

        let quotes = typograph("&quot;Hi&quot;");
        assert!(quotes.contains('\u{201C}'), "{quotes:?}");
        assert!(quotes.contains('\u{201D}'), "{quotes:?}");
        assert!(!quotes.contains("&quot;"), "{quotes:?}");

        let apos = typograph("It&#39;s");
        assert!(apos.contains("It's"), "{apos:?}");
    }
}
