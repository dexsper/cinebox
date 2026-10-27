use cinebox_core::{
    CacheHit, CacheKey, CatalogItem, CreditPerson, KIND_MEDIA, LibraryMark, MediaDetails, MediaKind, TmdbId,
    format_money, language_key, media_cache_id, media_ttl, tmdb_image_url,
};
use egui::{Atom, Rect, RichText, Sense, Ui, Vec2, pos2, vec2};
use egui_material_icons::icons::{ICON_LOCAL_MOVIES, ICON_PLAY_CIRCLE};
use rust_i18n::t;

use super::gate;
use crate::app::nav::{MediaRef, NavAction};
use crate::i18n::errors::UserError;
use crate::screens::play::WatchCard;
use crate::services::Services;
use crate::services::jobs;
use crate::theme::Theme;
use crate::widgets::button::ExpandingIcon;
use crate::widgets::lists::{self, MenuPick};
use crate::widgets::page_state::{ErrorChoice, error_page};
use crate::widgets::poster::{TileLook, Wrap};
use crate::widgets::{self, flyout, focus, intro, poster, scroll, skeleton};

use super::trailers::TrailersModal;

const WATCH_BTN_SIZE: Vec2 = vec2(176.0, 46.0);

/// Year/country header and the "Details" heading share this size; both are
/// local to the hero so `theme.text_section` stays untouched for every other
/// screen (home shelf titles, person credits, settings, torrents, trailers).
const HERO_LABEL_SIZE: f32 = 19.0;
/// Tagline size; local so `theme.text_subtitle` keeps its size everywhere
/// else (player overlay, trailers modal, rating pill, Watch/Trailer buttons).
const HERO_TAGLINE_SIZE: f32 = 22.0;
/// Overview body size; local so `theme.text_label` keeps its size on the
/// torrents list and settings screens.
const HERO_OVERVIEW_SIZE: f32 = 18.0;
/// Minimum breathing room above and below the rating/bits group when it is
/// centered between the tagline and the Watch/Trailers buttons.
const MIN_META_GAP: f32 = 12.0;
/// Gap between the rating pill row and the runtime/genres line beneath it.
const RATING_BITS_GAP: f32 = 8.0;

#[derive(Default)]
pub(crate) struct MediaScreen {
    kind: Option<MediaKind>,
    id: Option<TmdbId>,
    preview: Option<CatalogItem>,
    cache: super::swr::Cached<Box<MediaDetails>, CacheHit<Box<MediaDetails>>>,
    intro_at: Option<f64>,
    intro_from: IntroFrom,
    pending_intro: bool,
    is_scroll_reset: bool,
    trailers: TrailersModal,
    /// List popover anchor while it is open; follows the button as the page scrolls.
    lists_anchor: Option<Rect>,
}

/// What the hero grows from as the page opens.
#[derive(Clone, Copy, Default)]
enum IntroFrom {
    /// The catalog tile that was opened.
    #[default]
    Tile,
    /// The torrents page's smaller hero, on Back from it.
    Torrents,
}

/// Poster and text sizes of the hero at a point of its intro.
#[derive(Clone, Copy)]
struct HeroSizes {
    poster: Vec2,
    title: f32,
    head: f32,
}

impl HeroSizes {
    fn at(theme: &Theme, from: IntroFrom, t: f32) -> Self {
        let start = Self::start(theme, from);
        let poster_width = intro::lerp(start.poster.x, theme.poster_w, t);
        let poster_height = intro::lerp(start.poster.y, theme.poster_h, t);
        let poster = vec2(poster_width, poster_height);

        Self {
            poster,
            title: intro::lerp(start.title, theme.text_hero, t),
            head: intro::lerp(start.head, HERO_LABEL_SIZE, t),
        }
    }

    fn start(theme: &Theme, from: IntroFrom) -> Self {
        match from {
            IntroFrom::Tile => {
                Self { poster: vec2(theme.tile_w, theme.tile_h), title: theme.text_small, head: theme.text_caption }
            }
            IntroFrom::Torrents => Self {
                poster: vec2(theme.explorer_poster_w, theme.explorer_poster_h),
                title: theme.text_display,
                head: theme.text_section,
            },
        }
    }
}

/// Where the page stands this frame: how far the hero has grown, and whether
/// it jumps back to the top.
#[derive(Clone, Copy)]
struct PageLayout {
    sizes: HeroSizes,
    is_scroll_reset: bool,
}

/// What the page asked for this frame.
#[derive(Default)]
struct PageOut {
    action: Option<NavAction>,
    open_trailers: bool,
    toggle_lists: bool,
    lists_button: Option<Rect>,
}

/// A titled, horizontally scrolled row of cards.
struct CardRow<'a, T> {
    title: &'a str,
    salt: &'static str,
    items: &'a [T],
}

impl MediaScreen {
    pub(crate) fn ready(&mut self) -> Option<&MediaDetails> {
        if let Some(Ok(details)) = self.cache.bind.read() {
            return Some(details);
        }

        self.cache.disk.as_ref().map(|hit| hit.value.as_ref())
    }

    pub(crate) fn seed(&mut self, item: CatalogItem) {
        if self.kind != Some(item.kind) || self.id != Some(item.id) {
            self.cache.reset();
        }
        self.kind = Some(item.kind);
        self.id = Some(item.id);
        self.preview = Some(item);
        self.intro_from = IntroFrom::Tile;
        self.pending_intro = true;
        self.is_scroll_reset = true;
        self.lists_anchor = None;
        let _ = self.trailers.close();
    }

    /// The hero grows back from the torrents page's, the reverse of its intro.
    pub(crate) fn back_from_torrents(&mut self) {
        self.intro_from = IntroFrom::Torrents;
        self.pending_intro = true;
    }

    pub(crate) fn take_play(&mut self) -> Option<crate::screens::play::PlayRequest> {
        self.trailers.take_play()
    }

    pub(crate) fn on_back(&mut self) -> bool {
        if self.lists_anchor.take().is_some() {
            return true;
        }

        self.trailers.close()
    }

    pub(crate) fn ui(&mut self, ui: &mut Ui, svc: &mut Services, theme: &Theme, media: MediaRef) -> Option<NavAction> {
        if let Some(problem) = gate::tmdb_key_problem(&svc.settings) {
            return gate::tmdb_key(ui, theme, problem);
        }

        let MediaRef { kind, id } = media;
        let now = ui.input(|i| i.time);
        if self.kind != Some(kind) || self.id != Some(id) {
            self.kind = Some(kind);
            self.id = Some(id);
            self.cache.reset();
            self.is_scroll_reset = true;
            self.lists_anchor = None;
            let _ = self.trailers.close();

            let is_different = self.preview.as_ref().is_none_or(|item| item.kind != kind || item.id != id);

            if is_different {
                self.preview = None;
            }
        }

        self.cache.sync_lang(svc.settings.general.language);

        let lang = language_key(Some(svc.settings.general.language.tmdb_code())).to_owned();
        let disk_db = svc.db.clone();
        let hydrated = self.cache.hydrate(async move {
            let db = disk_db?;
            let cache_id = media_cache_id(kind, id);
            let hit = db
                .get_json::<MediaDetails>(CacheKey { language: &lang, kind: KIND_MEDIA, id: &cache_id })
                .await
                .ok()
                .flatten()?;

            let mut value = hit.value;
            value.apply_typography();

            Some(CacheHit { value: Box::new(value), fetched_at: hit.fetched_at })
        });
        self.start_intro_if_pending(now);

        let sizes = HeroSizes::at(theme, self.intro_from, intro::t(self.intro_at, now));
        if intro::running(self.intro_at, now) {
            ui.ctx().request_repaint();
        }

        let layout = PageLayout { sizes, is_scroll_reset: self.is_scroll_reset };
        if !hydrated {
            if let Some(item) = self.preview.as_ref() {
                self.is_scroll_reset = false;
                loading(ui, svc, theme, layout, item);
            } else {
                widgets::page_spinner(ui, theme);
            }
            ui.ctx().request_repaint();
            return None;
        }

        let tmdb = jobs::TmdbCtx::from(&svc.settings);
        let db = svc.db.clone();
        let cache = self.cache.disk.as_ref();
        let fresh = cache.is_some_and(|hit| hit.is_fresh(media_ttl(&hit.value)));
        let outcome = self.cache.resolve(fresh, move || jobs::load_media(tmdb, kind, id, db));

        if outcome.in_flight {
            ui.ctx().request_repaint();
        }

        let mut retry = false;
        let out = match outcome.view {
            super::swr::Swr::Live => match self.cache.bind.read() {
                Some(Ok(details)) => {
                    self.is_scroll_reset = false;
                    ready(ui, details, svc, theme, layout)
                }
                _ => PageOut::default(),
            },
            super::swr::Swr::Disk => match self.cache.disk.as_ref() {
                Some(hit) => {
                    self.is_scroll_reset = false;
                    ready(ui, &hit.value, svc, theme, layout)
                }
                None => PageOut::default(),
            },
            super::swr::Swr::Failed => {
                let error = UserError::from_read(self.cache.bind.read().as_ref());
                let choice = error_page(ui, theme, &error);
                retry = choice == Some(ErrorChoice::Retry);
                PageOut { action: choice.and_then(ErrorChoice::nav), ..PageOut::default() }
            }
            super::swr::Swr::Pending => {
                if let Some(item) = self.preview.as_ref() {
                    self.is_scroll_reset = false;
                    loading(ui, svc, theme, layout, item);
                } else {
                    widgets::page_spinner(ui, theme);
                }

                PageOut::default()
            }
        };

        if retry {
            self.cache.retry();
        }

        if out.open_trailers {
            self.open_trailers_from_details();
        }

        self.lists_popover(ui, svc, theme, &out);

        self.trailers.poll();
        if self.trailers.is_open() {
            self.trailers.show(ui.ctx(), svc, theme);
        }

        out.action
    }

    fn open_trailers_from_details(&mut self) {
        let packed = self.ready().map(|details| {
            let card = WatchCard::from_details(details);

            (card, details.backdrop_path.clone(), details.trailers.clone())
        });

        let Some((card, backdrop_path, items)) = packed else {
            return;
        };

        self.trailers.open(card, backdrop_path, items);
    }

    fn lists_popover(&mut self, ui: &Ui, svc: &mut Services, theme: &Theme, hero: &PageOut) {
        if hero.toggle_lists {
            self.lists_anchor = match self.lists_anchor {
                Some(_) => None,
                None => hero.lists_button,
            };
        }

        let Some(anchor) = self.lists_anchor.and(hero.lists_button) else {
            self.lists_anchor = None;
            return;
        };
        self.lists_anchor = Some(anchor);

        let Some(card) = self.ready().map(WatchCard::from_details) else {
            return;
        };

        let mark = svc.library_mark(card.kind, card.id);
        let mut pick = None;
        let placement = flyout::Placement { id: "cinebox-media-lists", anchor, width: lists::menu_width() };
        let out = flyout::show_below(ui.ctx(), theme, placement, |ui, theme| pick = lists::menu(ui, theme, mark));

        match pick {
            Some(MenuPick::Status(status)) => {
                svc.set_list_status(&card, status);
                self.lists_anchor = None;
            }
            Some(MenuPick::Liked(liked)) => svc.set_liked(&card, liked),
            Some(MenuPick::Clear) => {
                svc.set_list_status(&card, None);
                svc.set_liked(&card, false);
                self.lists_anchor = None;
            }
            None if out.dismissed => self.lists_anchor = None,
            None => {}
        }
    }

    fn start_intro_if_pending(&mut self, now: f64) {
        if self.pending_intro {
            self.intro_at = Some(now);
            self.pending_intro = false;
        }
    }
}

impl super::LiveTmdb for MediaScreen {
    fn forget_live(&mut self) {
        self.cache.forget_live();
    }
}

fn ready(ui: &mut Ui, details: &MediaDetails, svc: &Services, theme: &Theme, layout: PageLayout) -> PageOut {
    let mut out = PageOut::default();
    let head = details.head_line();
    let card = Hero {
        kind: details.kind,
        id: details.id,
        poster_path: details.poster_path.as_deref(),
        title: &details.title,
        head: &head,
        sizes: layout.sizes,
    };

    scroll_page(ui, details.kind, details.id, layout.is_scroll_reset, |ui| {
        ui.add_space(12.0);
        hero(ui, svc, theme, &card, |ui, col_top| {
            let column_bottom = col_top + layout.sizes.poster.y;

            if let Some(tagline) = details.tagline.as_deref() {
                ui.label(RichText::new(tagline).size(HERO_TAGLINE_SIZE).color(theme.title));
            }

            rating_group(ui, details, theme, column_bottom);

            let gap_after = (column_bottom - ui.cursor().top() - WATCH_BTN_SIZE.y).max(MIN_META_GAP);
            ui.add_space(gap_after);
            hero_buttons(ui, details, svc, theme, &mut out);
        });

        if let Some(overview) = details.overview.as_deref() {
            ui.add_space(16.0);
            ui.vertical(|ui| {
                ui.set_max_width(theme.overview_max_w);
                ui.label(
                    RichText::new(t!("media.in_detail").as_ref())
                        .font(theme.title_font(HERO_LABEL_SIZE))
                        .color(theme.title),
                );
                ui.label(RichText::new(overview).size(HERO_OVERVIEW_SIZE).color(theme.body));
            });
        }

        ui.add_space(28.0);
        facts(ui, details, theme);
        if !details.directors.is_empty() {
            let title = t!("media.directors");
            let row = CardRow { title: title.as_ref(), salt: "media-directors", items: &details.directors };
            people(ui, row, svc, theme, &mut out.action);
        }

        if !details.cast.is_empty() {
            let title = t!("media.cast");
            let row = CardRow { title: title.as_ref(), salt: "media-cast", items: &details.cast };
            people(ui, row, svc, theme, &mut out.action);
        }

        let collection = t!("media.collection");
        let recommendations = t!("media.recommendations");
        let similar = t!("media.similar");
        let rows = [
            CardRow { title: collection.as_ref(), salt: "media-collection", items: &details.collection },
            CardRow { title: recommendations.as_ref(), salt: "media-recommendations", items: &details.recommendations },
            CardRow { title: similar.as_ref(), salt: "media-similar", items: &details.similar },
        ];

        for row in rows {
            shelf(ui, row, svc, theme, &mut out.action);
        }
    });

    out
}

/// Rating pill and runtime/genres line, centered between the tagline and the
/// buttons that end at `column_bottom`.
fn rating_group(ui: &mut Ui, details: &MediaDetails, theme: &Theme, column_bottom: f32) {
    let vote = details.vote.filter(|v| *v > 0.0);
    let cert = details.certification.as_deref().filter(|s| !s.is_empty());
    let status = crate::i18n::status_label(details);
    let has_rating = vote.is_some() || cert.is_some() || status.is_some();

    let bits = crate::i18n::detail_bits(details);
    let has_bits = !bits.is_empty();

    if !has_rating && !has_bits {
        return;
    }

    let rating_h = if has_rating { widgets::rating::row_height(ui, theme) } else { 0.0 };
    let bits_h = if has_bits { ui.ctx().fonts_mut(|f| f.row_height(&theme.ui_font(theme.text_section))) } else { 0.0 };
    let inner_gap = if has_rating && has_bits { RATING_BITS_GAP } else { 0.0 };

    let group_h = rating_h + inner_gap + bits_h;
    let remaining = column_bottom - ui.cursor().top() - WATCH_BTN_SIZE.y - group_h;
    ui.add_space((remaining / 2.0).max(MIN_META_GAP));

    if has_rating {
        widgets::rating::row(ui, theme, vote, status.as_deref(), cert);
    }

    if has_rating && has_bits {
        ui.add_space(RATING_BITS_GAP);
    }

    if has_bits {
        ui.label(RichText::new(bits.join(" | ")).size(theme.text_section).color(theme.title));
    }
}

/// Watch, Trailers and the lists button, one D-pad group.
fn hero_buttons(ui: &mut Ui, details: &MediaDetails, svc: &Services, theme: &Theme, out: &mut PageOut) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        let watch = watch_button(ui, theme);
        if watch.clicked() {
            out.action = Some(NavAction::WatchTorrents);
        }
        let mut members = vec![watch.id];

        if !details.trailers.is_empty() {
            let trailers = trailers_button(ui, theme);
            out.open_trailers = trailers.clicked();
            members.push(trailers.id);
        }

        let mark = svc.library_mark(details.kind, details.id);
        let lists = lists_button(ui, theme, mark);
        out.toggle_lists = lists.clicked();
        out.lists_button = Some(lists.rect);
        members.push(lists.id);

        // Coming from the bar above or the page below, the D-pad lands on Watch.
        focus::group(ui.ctx(), members, watch.id);
    });
}

fn loading(ui: &mut Ui, svc: &Services, theme: &Theme, layout: PageLayout, item: &CatalogItem) {
    let year = item.year.map_or_else(|| String::from("—"), |year| year.to_string());
    let card = Hero {
        kind: item.kind,
        id: item.id,
        poster_path: item.poster_path.as_deref(),
        title: &item.title,
        head: &year,
        sizes: layout.sizes,
    };

    let pulse = skeleton::pulse(ui);
    scroll_page(ui, item.kind, item.id, layout.is_scroll_reset, |ui| {
        ui.add_space(12.0);
        hero(ui, svc, theme, &card, |ui, col_top| {
            ui.add_space(8.0);
            skeleton::bar(ui, theme, 280.0, 16.0, pulse);
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                skeleton::bar(ui, theme, 72.0, 32.0, pulse);
                skeleton::bar(ui, theme, 48.0, 32.0, pulse);
            });

            ui.add_space(10.0);
            skeleton::bar(ui, theme, 220.0, 14.0, pulse);

            let used = ui.cursor().top() - col_top;
            let gap = (layout.sizes.poster.y - used - WATCH_BTN_SIZE.y).max(12.0);

            ui.add_space(gap);
            skeleton::bar(ui, theme, WATCH_BTN_SIZE.x, WATCH_BTN_SIZE.y, pulse);
        });

        ui.add_space(16.0);
        skeleton::bar(ui, theme, 120.0, 16.0, pulse);
        ui.add_space(8.0);

        skeleton::bar(ui, theme, theme.overview_max_w.min(ui.available_width()), 14.0, pulse);

        ui.add_space(6.0);
        skeleton::bar(ui, theme, theme.overview_max_w.min(ui.available_width()) * 0.85, 14.0, pulse);

        ui.add_space(6.0);
        skeleton::bar(ui, theme, theme.overview_max_w.min(ui.available_width()) * 0.7, 14.0, pulse);

        ui.add_space(28.0);
        ui.horizontal(|ui| {
            skeleton::bar(ui, theme, 88.0, 36.0, pulse);
            ui.add_space(24.0);
            skeleton::bar(ui, theme, 88.0, 36.0, pulse);
            ui.add_space(24.0);
            skeleton::bar(ui, theme, 120.0, 36.0, pulse);
        });

        ui.add_space(16.0);
        skeleton::bar(ui, theme, 80.0, 16.0, pulse);
        ui.add_space(8.0);

        let scale = poster::card_scale(ui.available_width());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            for _ in 0..5 {
                skeleton::poster(ui, theme, vec2(100.0, 150.0) * scale, pulse);
            }
        });
    });
}

struct Hero<'a> {
    kind: MediaKind,
    id: TmdbId,
    poster_path: Option<&'a str>,
    title: &'a str,
    head: &'a str,
    sizes: HeroSizes,
}

fn hero(ui: &mut Ui, svc: &Services, theme: &Theme, hero: &Hero<'_>, meta: impl FnOnce(&mut Ui, f32)) {
    ui.horizontal(|ui| {
        let poster = poster::rounded_image(ui, hero.sizes.poster, theme, || {
            svc.images.poster_key(hero.kind, hero.id, hero.poster_path, svc.settings.tmdb.poster_size)
        });
        let marks = svc.tile_marks(hero.kind, hero.id);
        poster::paint_marks(ui, poster, marks, theme, poster::UNSCALED);
        ui.add_space(28.0);

        let col_w = ui.available_width();
        ui.vertical(|ui| {
            ui.set_width(col_w);
            let col_top = ui.cursor().top();

            if !hero.head.is_empty() {
                ui.label(RichText::new(hero.head).size(hero.sizes.head).color(theme.title));
                ui.add_space(6.0);
            }

            ui.label(RichText::new(hero.title).font(theme.title_font(hero.sizes.title)).color(theme.title));
            meta(ui, col_top);
        });
    });
}

fn watch_button(ui: &mut Ui, theme: &Theme) -> egui::Response {
    widgets::button::add_named(
        ui,
        theme,
        (
            Atom::grow(),
            ICON_PLAY_CIRCLE.rich_text().size(theme.text_cta_icon).color(theme.btn_primary_fg),
            RichText::new(t!("media.watch").as_ref())
                .font(theme.emphasis_font(theme.text_subtitle))
                .color(theme.btn_primary_fg),
            Atom::grow(),
        ),
        widgets::button::Opts::primary(WATCH_BTN_SIZE),
        Some(t!("media.watch").as_ref()),
    )
}

fn trailers_button(ui: &mut Ui, theme: &Theme) -> egui::Response {
    let label = t!("media.trailers");
    let button = ExpandingIcon {
        id_salt: "media-trailers",
        icon: ICON_LOCAL_MOVIES,
        label: label.as_ref(),
        height: WATCH_BTN_SIZE.y,
        tint: theme.title,
        selected: false,
    };

    widgets::button::expanding_icon(ui, theme, &button)
}

fn lists_button(ui: &mut Ui, theme: &Theme, mark: LibraryMark) -> egui::Response {
    let label = match mark.status {
        Some(status) => crate::i18n::list_status_label(status),
        None if mark.liked => t!("library.liked"),
        None => t!("library.add"),
    };
    let tint = if mark.status.is_none() && mark.liked { theme.liked } else { theme.title };
    let button = ExpandingIcon {
        id_salt: "media-lists",
        icon: lists::button_icon(mark),
        label: label.as_ref(),
        height: WATCH_BTN_SIZE.y,
        tint,
        selected: !mark.is_empty(),
    };

    widgets::button::expanding_icon(ui, theme, &button)
}

fn facts(ui: &mut Ui, details: &MediaDetails, theme: &Theme) {
    ui.horizontal_wrapped(|ui| {
        let release = details
            .released
            .as_deref()
            .map(crate::i18n::format_release_date)
            .or_else(|| details.year.map(|year| year.to_string()));

        if let Some(release) = release {
            fact(ui, t!("media.release").as_ref(), release, theme);
        }

        if let Some(budget) = details.budget {
            fact(ui, t!("media.budget").as_ref(), format_money(budget), theme);
        }

        if !details.countries.is_empty() {
            fact(ui, t!("media.countries").as_ref(), details.countries.join(", "), theme);
        }
    });
}

fn fact(ui: &mut Ui, label: &str, value: String, theme: &Theme) {
    ui.vertical(|ui| {
        ui.label(RichText::new(label).size(theme.text_small).color(theme.muted));
        ui.label(RichText::new(value).size(theme.text_section).color(theme.title));
    });

    ui.add_space(24.0);
}

fn people(ui: &mut Ui, row: CardRow<'_, CreditPerson>, svc: &Services, theme: &Theme, action: &mut Option<NavAction>) {
    const TILE_W: f32 = 100.0;
    const PHOTO: Vec2 = vec2(100.0, 150.0);
    const CAPTION_GAP: f32 = 4.0;
    const LINE_GAP: f32 = 2.0;

    ui.add_space(12.0);
    ui.label(RichText::new(row.title).font(theme.title_font(theme.text_section)).color(theme.title));

    let scale = poster::card_scale(ui.available_width());
    let tile_w = TILE_W * scale;
    let photo = PHOTO * scale;
    let caption_gap = CAPTION_GAP * scale;
    let line_gap = LINE_GAP * scale;
    let name_size = theme.text_caption * scale;
    let role_size = theme.text_micro * scale;
    let pad = theme.ring_pad();
    let (name_slot, role_slot) = ui
        .ctx()
        .fonts_mut(|f| (f.row_height(&theme.ui_font(name_size)) * 2.0, f.row_height(&theme.ui_font(role_size)) * 2.0));

    let well_w = tile_w + pad * 2.0;
    let tile_h = pad + photo.y + caption_gap + name_slot + line_gap + role_slot;
    scroll::horizontal(ui, row.salt, |ui| {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            for person in row.items {
                let size = vec2(well_w, tile_h);
                let (rect, response) = ui.allocate_exact_size(size, Sense::click());
                let response = widgets::button::pointing(response);

                if !poster::in_load_window(ui, rect) {
                    if response.clicked() {
                        *action = Some(NavAction::OpenPerson { person: person.clone() });
                    }

                    continue;
                }

                let url = tmdb_image_url(person.profile_path.as_deref(), "w185");
                let tex = svc.images.slot(url.as_deref());
                let name_wrap = Wrap { color: theme.title, font_size: name_size, width: tile_w, max_rows: 2 };
                let name = poster::wrap_lines(ui, theme, &person.name, name_wrap);
                let role_wrap = Wrap { color: theme.muted, font_size: role_size, ..name_wrap };
                let role = poster::wrap_lines(ui, theme, &person.role, role_wrap);

                let name_h = name.size().y;
                let photo_rect = Rect::from_min_size(rect.min + vec2(pad, pad), photo);

                poster::paint_poster(ui, photo_rect, tex, theme);
                if focus::own_mark(&response) {
                    poster::hover_ring(ui, photo_rect, theme);
                }

                let name_pos = pos2(photo_rect.left(), photo_rect.bottom() + caption_gap);
                ui.painter().galley(name_pos, name, theme.title);
                ui.painter().galley(name_pos + vec2(0.0, name_h + line_gap), role, theme.muted);

                if response.clicked() {
                    *action = Some(NavAction::OpenPerson { person: person.clone() });
                }
            }
        });
    });
}

fn shelf(ui: &mut Ui, row: CardRow<'_, CatalogItem>, svc: &Services, theme: &Theme, action: &mut Option<NavAction>) {
    if row.items.is_empty() {
        return;
    }

    ui.add_space(12.0);
    ui.label(RichText::new(row.title).font(theme.title_font(theme.text_section)).color(theme.title));

    let scale = poster::card_scale(ui.available_width());
    scroll::horizontal(ui, row.salt, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            for item in row.items {
                let look = TileLook { marks: svc.tile_marks(item.kind, item.id), scale };
                if let Some(nav) = poster::catalog_tile(ui, theme, item, svc.posters(), look) {
                    *action = Some(nav);
                }
            }
        });
    });
}

fn scroll_page(ui: &mut Ui, kind: MediaKind, id: TmdbId, is_scroll_reset: bool, add: impl FnOnce(&mut Ui)) {
    scroll::page(ui, ("media-page", kind, id), is_scroll_reset, add);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_item() -> CatalogItem {
        CatalogItem {
            id: TmdbId::new(1),
            kind: MediaKind::Movie,
            title: "Dune".into(),
            year: Some(2021),
            vote: Some(8.0),
            poster_path: Some("/dune.jpg".into()),
        }
    }

    #[test]
    fn seed_starts_intro_once() {
        let mut screen = MediaScreen::default();

        screen.seed(sample_item());
        screen.start_intro_if_pending(1.0);
        assert!(intro::running(screen.intro_at, 1.05));

        screen.start_intro_if_pending(1.1);
        assert!((intro::t(screen.intro_at, 1.1) - intro::t(Some(1.0), 1.1)).abs() < f32::EPSILON);
    }

    #[test]
    fn returning_without_seed_does_not_replay() {
        let mut screen = MediaScreen::default();

        screen.seed(sample_item());
        screen.start_intro_if_pending(0.0);
        assert!(!intro::running(screen.intro_at, 1.0));

        screen.start_intro_if_pending(10.0);
        assert!(!intro::running(screen.intro_at, 10.0));
    }

    #[test]
    fn seed_asks_to_reset_scroll() {
        let mut screen = MediaScreen::default();
        assert!(!screen.is_scroll_reset);

        screen.seed(sample_item());
        assert!(screen.is_scroll_reset);
    }
}
