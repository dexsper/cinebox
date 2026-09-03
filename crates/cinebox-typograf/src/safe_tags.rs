//! SafeTags: hide HTML/URL/own fragments behind private labels.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::LazyLock;

use fancy_regex::Regex;

use crate::re;
use crate::{PRIVATE, PRIVATE_SEPARATE};

const INLINE: &[&str] = &[
    "a", "abbr", "acronym", "b", "bdo", "big", "br", "button", "cite", "code", "dfn", "em", "i",
    "img", "input", "kbd", "label", "map", "object", "q", "samp", "script", "select", "small",
    "span", "strong", "sub", "sup", "textarea", "time", "tt", "var",
];

static HTML_PAIRS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    let pairs = [
        ("<!--", "-->"),
        ("<!ENTITY", ">"),
        ("<!DOCTYPE", ">"),
        ("<\\?xml", "\\?>"),
        ("<!\\[CDATA\\[", "\\]\\]>"),
        ("<code(\\s[^>]*?)?>", "</code>"),
        ("<kbd(\\s[^>]*?)?>", "</kbd>"),
        ("<object(\\s[^>]*?)?>", "</object>"),
        ("<pre(\\s[^>]*?)?>", "</pre>"),
        ("<samp(\\s[^>]*?)?>", "</samp>"),
        ("<script(\\s[^>]*?)?>", "</script>"),
        ("<style(\\s[^>]*?)?>", "</style>"),
        ("<var(\\s[^>]*?)?>", "</var>"),
    ];

    pairs
        .iter()
        .map(|(start, end)| re::compile_is(&format!("{start}.*?{end}")))
        .collect()
});

static URL: LazyLock<Regex> =
    LazyLock::new(|| re::compile("(https?|file|ftp)://([a-zA-Z0-9/+-=%&:_.~?]+[a-zA-Z0-9#+]*)"));
static TAG_RE: LazyLock<Regex> = LazyLock::new(|| re::compile_is("</?[a-z].*?>"));
static ESCAPED_RE: LazyLock<Regex> = LazyLock::new(|| re::compile_is("&lt;/?[a-z].*?&gt;"));
static LTGT_RE: LazyLock<Regex> = LazyLock::new(|| re::compile_i("&[gl]t;"));
static LABEL_RE: LazyLock<Regex> =
    LazyLock::new(|| re::compile(&format!("{PRIVATE}tf\\d+{PRIVATE}")));
static SEARCH_RE: LazyLock<Regex> = LazyLock::new(|| re::compile(&format!("{PRIVATE}tf\\d")));
static IS_HTML_RE: LazyLock<Regex> = LazyLock::new(|| re::compile_i("(</?[a-z]|<!|&[lg]t;)"));
static REMOVE_CR_RE: LazyLock<Regex> = LazyLock::new(|| re::compile("\\r\\n?"));
static SEPARATE_PARTS_RE: LazyLock<Regex> =
    LazyLock::new(|| re::compile_is("<(title|p|h[1-6]|select|legend)(\\s[^>]*?)?>.*?</\\1>"));

pub struct TagInfo {
    pub group: &'static str,
    pub is_inline: bool,
    pub is_closing: bool,
}

#[derive(Default)]
pub struct SafeTags {
    hidden: HashMap<&'static str, HashMap<String, String>>,
    counter: usize,
}

impl SafeTags {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.hidden.clear();
        self.counter = 0;
    }

    pub fn hide(&mut self, text: &mut String, group: &'static str) {
        self.hidden.insert(group, HashMap::new());

        match group {
            "html" => {
                for tag in HTML_PAIRS.iter() {
                    self.replace_with_label(text, group, tag);
                }
            }
            "url" => {
                self.replace_with_label(text, group, &URL);
            }
            _ => {}
        }
    }

    pub fn hide_html_tags(&mut self, text: &mut String, is_html: bool) {
        if !is_html {
            return;
        }

        self.replace_with_label(text, "html", &TAG_RE);
        self.replace_with_label(text, "html", &ESCAPED_RE);
        self.replace_with_label(text, "html", &LTGT_RE);
    }

    pub fn show(&mut self, text: &mut String, group: &'static str) {
        let hidden = self.hidden.get(group).cloned().unwrap_or_default();
        let rounds = match group {
            "html" => HTML_PAIRS.len(),
            "url" => 1,
            _ => 0,
        };

        for _ in 0..rounds {
            let shown = re::replace_all_fn(&LABEL_RE, text, |caps| {
                hidden
                    .get(&caps[0])
                    .cloned()
                    .unwrap_or_else(|| caps[0].to_string())
            });

            if let Cow::Owned(shown) = shown {
                *text = shown;
            }

            if !re::is_match(&SEARCH_RE, text) {
                break;
            }
        }
    }

    /// `pos` is the byte offset of the quote character; `chars` is the
    /// pre-built `char_indices` of `text`.
    ///
    /// Walks from two characters before `pos` so the closing private mark of a
    /// hide-label is not taken as the whole key.
    pub fn get_prev_tag_info(
        &self,
        chars: &[(usize, char)],
        text: &str,
        pos: usize,
    ) -> Option<TagInfo> {
        let char_pos = chars.binary_search_by_key(&pos, |(b, _)| *b).ok()?;

        if char_pos == 0 {
            return None;
        }

        let position = char_pos - 1;

        if position == 0 {
            return None;
        }

        let mut i = position - 1;

        loop {
            if chars[i].1 == PRIVATE {
                let label = slice_chars(chars, text, i, position + 1)?;

                return self.tag_info(&label);
            }

            if i == 0 {
                break;
            }

            i -= 1;
        }

        None
    }

    /// Slice through the closing private char (inclusive), starting one
    /// character after `pos`.
    pub fn get_next_tag_info(
        &self,
        chars: &[(usize, char)],
        text: &str,
        pos: usize,
    ) -> Option<TagInfo> {
        let char_pos = chars.binary_search_by_key(&pos, |(b, _)| *b).ok()?;
        let position = char_pos + 1;

        if position >= chars.len() {
            return None;
        }

        let mut i = position + 1;

        while i < chars.len() {
            if chars[i].1 == PRIVATE {
                let label = slice_chars(chars, text, position, i + 1)?;

                return self.tag_info(&label);
            }

            i += 1;
        }

        None
    }

    fn tag_info(&self, label: &str) -> Option<TagInfo> {
        for group in ["own", "html", "url"] {
            let Some(value) = self.hidden.get(group).and_then(|m| m.get(label)) else {
                continue;
            };

            if group == "url" {
                return Some(TagInfo {
                    group,
                    is_inline: true,
                    is_closing: false,
                });
            }

            if group == "own" {
                return Some(TagInfo {
                    group,
                    is_inline: false,
                    is_closing: false,
                });
            }

            let name = html_tag_name(value);
            let is_inline = INLINE.iter().any(|n| n.eq_ignore_ascii_case(&name));
            let is_closing = value.starts_with("</");

            return Some(TagInfo {
                group,
                is_inline,
                is_closing,
            });
        }

        None
    }

    fn replace_with_label(&mut self, text: &mut String, group: &'static str, re: &Regex) {
        let mut counter = self.counter;
        let mut hidden = self.hidden.remove(group).unwrap_or_default();

        let hidden_text = re::replace_all_fn(re, text, |caps| {
            let key = format!("{PRIVATE}tf{counter}{PRIVATE}");
            hidden.insert(key.clone(), caps[0].to_string());
            counter += 1;
            key
        });

        if let Cow::Owned(hidden_text) = hidden_text {
            *text = hidden_text;
        }

        self.counter = counter;
        self.hidden.insert(group, hidden);
    }
}

fn html_tag_name(tag: &str) -> String {
    tag.split(|c: char| c == '<' || c.is_whitespace() || c == '>')
        .nth(1)
        .unwrap_or("")
        .to_string()
}

fn slice_chars(chars: &[(usize, char)], text: &str, start: usize, end_exclusive: usize) -> Option<String> {
    let start_byte = chars.get(start)?.0;
    let end_byte = chars
        .get(end_exclusive)
        .map(|(b, _)| *b)
        .unwrap_or(text.len());

    text.get(start_byte..end_byte).map(str::to_string)
}

pub fn is_html(text: &str) -> bool {
    re::is_match(&IS_HTML_RE, text)
}

pub fn remove_cr(text: &str) -> String {
    re::replace_all(&REMOVE_CR_RE, text, "\n").into_owned()
}

pub fn strip_separate(text: &str) -> Cow<'_, str> {
    if !text.contains(PRIVATE_SEPARATE) {
        return Cow::Borrowed(text);
    }

    Cow::Owned(text.replace(PRIVATE_SEPARATE, ""))
}

pub fn separate_parts_re() -> &'static Regex {
    &SEPARATE_PARTS_RE
}
