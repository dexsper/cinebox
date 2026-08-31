use std::cell::RefCell;
use std::collections::HashMap;

use crate::data;
use crate::entities;
use crate::re;
use crate::rules::{self, Live, Queue, Rule};
use crate::safe_tags::{self, SafeTags};
use crate::{PRIVATE_SEPARATE};

#[derive(Clone, Debug)]
pub struct Prefs {
    pub locale: Vec<String>,
    pub live: bool,
    pub html_attrs_only: bool,
    pub processing_separate_parts: bool,
}

pub struct Context<'a> {
    pub locale: Vec<String>,
    pub live: bool,
    pub is_html: bool,
    pub html_attrs_only: bool,
    pub safe_tags: &'a RefCell<SafeTags>,
}

impl Context<'_> {
    pub fn chars(&self) -> String {
        data::joined_chars(&self.locale)
    }

    pub fn primary_locale(&self) -> &str {
        self.locale.first().map(String::as_str).unwrap_or("common")
    }

    pub fn short_word(&self) -> Option<&'static str> {
        data::short_word(self.primary_locale())
    }

    pub fn quote(&self) -> Option<&'static data::QuoteData> {
        data::quote(self.primary_locale())
    }
}

pub struct Typograf {
    prefs: Prefs,
    enabled: HashMap<String, bool>,
    safe_tags: RefCell<SafeTags>,
}

impl Typograf {
    pub fn new(locale: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        let locale: Vec<String> = locale.into_iter().map(|s| s.as_ref().to_string()).collect();

        if locale.is_empty() {
            panic!("typograf: locale is required");
        }

        for loc in &locale {
            if !data::has_locale(loc) {
                panic!("typograf: \"{loc}\" is not a supported locale");
            }
        }

        let mut enabled = HashMap::new();

        for rule in rules::all() {
            if rule.inner {
                continue;
            }

            enabled.insert(rule.name.to_string(), rule.enabled);
        }

        Self {
            prefs: Prefs {
                locale,
                live: false,
                html_attrs_only: false,
                processing_separate_parts: true,
            },
            enabled,
            safe_tags: RefCell::new(SafeTags::new()),
        }
    }

    #[must_use]
    pub fn execute(&self, text: &str) -> String {
        self.execute_prefs(text, None)
    }

    pub fn execute_prefs(&self, text: &str, locale: Option<&[String]>) -> String {
        if text.is_empty() {
            return String::new();
        }

        let locale = locale
            .map(|s| s.to_vec())
            .unwrap_or_else(|| self.prefs.locale.clone());

        let mut ctx = Context {
            locale,
            live: self.prefs.live,
            is_html: safe_tags::is_html(text),
            html_attrs_only: self.prefs.html_attrs_only,
            safe_tags: &self.safe_tags,
        };

        self.safe_tags.borrow_mut().reset();

        let mut text = safe_tags::remove_cr(text);

        self.run_queue(&mut text, &mut ctx, Queue::Start);

        ctx.safe_tags.borrow_mut().hide(&mut text, "own");
        self.run_queue(&mut text, &mut ctx, Queue::HideSafeTagsOwn);

        ctx.safe_tags.borrow_mut().hide(&mut text, "html");
        self.run_queue(&mut text, &mut ctx, Queue::HideSafeTagsHtml);

        let is_root_html = ctx.is_html;
        let parts = split_parts(&text, ctx.is_html && self.prefs.processing_separate_parts);

        let mut rebuilt = String::new();

        for part in parts {
            ctx.is_html = safe_tags::is_html(&part);
            let mut piece = part;

            ctx.safe_tags.borrow_mut().hide_html_tags(&mut piece, ctx.is_html);
            ctx.safe_tags.borrow_mut().hide(&mut piece, "url");
            self.run_queue(&mut piece, &mut ctx, Queue::HideSafeTagsUrl);
            self.run_queue(&mut piece, &mut ctx, Queue::HideSafeTags);

            piece = entities::to_utf(&piece);

            if ctx.live {
                piece = piece.replace('\u{00A0}', " ");
            }

            self.run_queue(&mut piece, &mut ctx, Queue::Utf);
            self.run_queue(&mut piece, &mut ctx, Queue::Default);
            self.run_queue(&mut piece, &mut ctx, Queue::HtmlEntities);

            ctx.safe_tags.borrow_mut().show(&mut piece, "url");
            self.run_queue(&mut piece, &mut ctx, Queue::ShowSafeTagsUrl);

            rebuilt.push_str(&safe_tags::strip_separate(&piece));
        }

        text = rebuilt;
        ctx.is_html = is_root_html;

        ctx.safe_tags.borrow_mut().show(&mut text, "html");
        self.run_queue(&mut text, &mut ctx, Queue::ShowSafeTagsHtml);

        ctx.safe_tags.borrow_mut().show(&mut text, "own");
        self.run_queue(&mut text, &mut ctx, Queue::ShowSafeTagsOwn);

        self.run_queue(&mut text, &mut ctx, Queue::End);
        self.safe_tags.borrow_mut().reset();

        text
    }

    pub fn disable_rule(&mut self, mask: &str) {
        self.set_enabled(mask, false);
    }

    pub fn enable_rule(&mut self, mask: &str) {
        self.set_enabled(mask, true);
    }

    pub fn is_enabled(&self, name: &str) -> bool {
        self.enabled.get(name).copied().unwrap_or(true)
    }

    pub fn rule_enabled_by_default(name: &str) -> bool {
        rules::all()
            .iter()
            .find(|r| r.name == name && !r.inner)
            .map(|r| r.enabled)
            .unwrap_or(false)
    }

    /// Inner-rule tests: run only inner handlers with this name, no execute pipeline.
    #[must_use]
    pub fn execute_inner(&self, name: &str, text: &str) -> String {
        let ctx = Context {
            locale: self.prefs.locale.clone(),
            live: false,
            is_html: false,
            html_attrs_only: false,
            safe_tags: &self.safe_tags,
        };

        let mut out = text.to_string();

        for rule in rules::all() {
            if !rule.inner || rule.name != name {
                continue;
            }

            out = (rule.handler)(self, &out, &ctx);
        }

        out
    }

    fn set_enabled(&mut self, mask: &str, enabled: bool) {
        if mask.is_empty() {
            return;
        }

        if !mask.contains('*') {
            self.enabled.insert(mask.to_string(), enabled);
            return;
        }

        let escaped = re::escape(&mask.replace('*', "\u{0001}"));
        let pat = escaped.replace('\u{0001}', ".*");
        let re = re::compile(&pat);

        for rule in rules::all() {
            if rule.inner {
                continue;
            }

            if re::is_match(&re, rule.name) {
                self.enabled.insert(rule.name.to_string(), enabled);
            }
        }
    }

    fn run_queue(&self, text: &mut String, ctx: &mut Context<'_>, queue: Queue) {
        let mut batch: Vec<&Rule> = rules::all()
            .iter()
            .filter(|r| r.queue == queue)
            .collect();

        batch.sort_by(|a, b| a.index.cmp(&b.index).then(a.order.cmp(&b.order)));

        let inners: Vec<&Rule> = batch.iter().copied().filter(|r| r.inner).collect();
        let mains: Vec<&Rule> = batch.iter().copied().filter(|r| !r.inner).collect();

        for rule in inners.into_iter().chain(mains) {
            self.apply_rule(text, ctx, rule);
        }
    }

    fn apply_rule(&self, text: &mut String, ctx: &mut Context<'_>, rule: &Rule) {
        match (ctx.live, rule.live) {
            (true, Live::NotLive) | (false, Live::OnlyLive) => return,
            _ => {}
        }

        let locale_ok = rule.locale == "common" || rule.locale == ctx.primary_locale();

        if !locale_ok {
            return;
        }

        if !self.is_enabled(rule.name) {
            return;
        }

        if ctx.html_attrs_only && !rule.html_attrs {
            return;
        }

        *text = (rule.handler)(self, text, ctx);
    }

    pub fn execute_nested(&self, text: &str, ctx: &Context<'_>) -> String {
        let nested = Typograf {
            prefs: Prefs {
                locale: ctx.locale.clone(),
                live: ctx.live,
                html_attrs_only: true,
                processing_separate_parts: true,
            },
            enabled: self.enabled.clone(),
            safe_tags: RefCell::new(SafeTags::new()),
        };

        nested.execute(text)
    }
}

fn split_parts(text: &str, split: bool) -> Vec<String> {
    if !split {
        return vec![text.to_string()];
    }

    let re = safe_tags::separate_parts_re();
    let mut parts = Vec::new();
    let mut position = 0;

    for mat in re.find_iter(text) {
        let Ok(mat) = mat else {
            continue;
        };

        if position != mat.start() {
            let prefix = if position == 0 {
                String::new()
            } else {
                PRIVATE_SEPARATE.to_string()
            };

            parts.push(format!(
                "{prefix}{}{PRIVATE_SEPARATE}",
                &text[position..mat.start()]
            ));
        }

        parts.push(mat.as_str().to_string());
        position = mat.end();
    }

    if position == 0 {
        return vec![text.to_string()];
    }

    parts.push(format!("{PRIVATE_SEPARATE}{}", &text[position..]));

    parts
}
