//! Discover: a TMDB `discover` grid scoped to one section, with genre, decade, rating, and sort filters.

use cinebox_core::{MediaKind, Section, UiLanguage};
use cinebox_tmdb::{Date, DiscoverQuery, DiscoverSort, genres_for};
use egui::{Align, Layout, RichText, Ui, vec2};
use egui_material_icons::icons::{ICON_CLOSE, ICON_FILTER_LIST};
use rust_i18n::t;

use crate::jobs;
use crate::nav::NavAction;
use crate::screens::paged::PagedGrid;
use crate::services::Services;
use crate::theme::Theme;
use crate::widgets::button::{self, Opts};
use crate::widgets::drawer::Overlay;
use crate::widgets::{chips, combo, scroll};

const FILTERS_BTN_W: f32 = 152.0;
const RATINGS: [u8; 3] = [6, 7, 8];
/// Rating filters without a vote floor surface titles rated by a handful of people.
const RATING_FILTER_MIN_VOTES: u32 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decade {
    From(i32),
    Before1980,
}

impl Decade {
    pub const ALL: [Self; 6] = [
        Self::From(2020),
        Self::From(2010),
        Self::From(2000),
        Self::From(1990),
        Self::From(1980),
        Self::Before1980,
    ];

    fn range(self) -> (Option<Date>, Option<Date>) {
        match self {
            Self::From(year) => (Some(Date::jan1(year)), Some(Date::dec31(year + 9))),
            Self::Before1980 => (None, Some(Date::dec31(1979))),
        }
    }

    fn label(self) -> String {
        match self {
            Self::From(year) => t!("discover.decade_n", year = year).into_owned(),
            Self::Before1980 => t!("discover.earlier").into_owned(),
        }
    }
}

/// User-chosen filters. Section scoping (animation, anime language) is added by [`Self::query`].
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoverFilters {
    pub kind: MediaKind,
    pub genres: Vec<u32>,
    pub decade: Option<Decade>,
    pub min_vote: Option<u8>,
    pub sort: DiscoverSort,
}

impl DiscoverFilters {
    pub fn new(kind: MediaKind) -> Self {
        Self {
            kind,
            genres: Vec::new(),
            decade: None,
            min_vote: None,
            sort: DiscoverSort::Popular,
        }
    }

    pub fn with_genre(kind: MediaKind, genre: u32) -> Self {
        Self {
            genres: vec![genre],
            ..Self::new(kind)
        }
    }

    pub fn query(&self, section: Section) -> DiscoverQuery {
        let mut query = DiscoverQuery::for_section(section, self.kind);
        query.genres.extend_from_slice(&self.genres);
        query.sort = self.sort;

        if let Some(decade) = self.decade {
            (query.released_from, query.released_to) = decade.range();
        }

        if let Some(vote) = self.min_vote {
            query.vote_min = Some(f32::from(vote));
            query.vote_count_min = Some(RATING_FILTER_MIN_VOTES);
        }

        query
    }

    /// Filters that live only in the drawer, for the button badge.
    fn drawer_count(&self) -> usize {
        usize::from(self.decade.is_some())
            + usize::from(self.min_vote.is_some())
            + usize::from(self.sort != DiscoverSort::Popular)
    }

    /// Drop choices the current kind cannot use (TV genres differ; TV has no box office).
    fn fit_kind(&mut self) {
        let offered = genres_for(self.kind);
        self.genres.retain(|genre| offered.contains(genre));

        if !DiscoverSort::available(self.kind).contains(&self.sort) {
            self.sort = DiscoverSort::Popular;
        }
    }
}

pub struct DiscoverScreen {
    section: Option<Section>,
    filters: DiscoverFilters,
    grid: PagedGrid,
    drawer: Overlay,
    lang: Option<UiLanguage>,
    reset_scroll: bool,
}

impl Default for DiscoverScreen {
    fn default() -> Self {
        Self {
            section: None,
            filters: DiscoverFilters::new(MediaKind::Movie),
            grid: PagedGrid::default(),
            drawer: Overlay::default(),
            lang: None,
            reset_scroll: false,
        }
    }
}

impl DiscoverScreen {
    pub fn seed(&mut self, section: Section, filters: DiscoverFilters) {
        self.section = Some(section);
        self.filters = filters;
        self.grid.reset();
        self.drawer.snap_shut();
        self.reset_scroll = true;
    }

    /// Drop live pages so the next paint reloads for a new TMDB language/key.
    pub fn forget_live(&mut self) {
        self.lang = None;
        self.grid.reset();
    }

    /// Escape / Back closes the filter drawer first.
    pub fn on_back(&mut self, now: f64) -> bool {
        self.drawer.on_back(now)
    }

    pub fn ui(
        &mut self,
        ui: &mut Ui,
        svc: &mut Services,
        theme: &Theme,
        section: Section,
    ) -> Option<NavAction> {
        if self.section != Some(section) {
            self.seed(section, DiscoverFilters::new(section.primary_kind()));
        }

        let lang = svc.settings.general.language;
        if self.lang.is_some_and(|seen| seen != lang) {
            self.grid.reset();
        }
        self.lang = Some(lang);

        let before = self.filters.clone();
        let to_top = std::mem::take(&mut self.reset_scroll);
        let filters = &mut self.filters;
        let drawer = &mut self.drawer;
        let out = self.grid.ui(ui, svc, theme, ("discover-page", section.as_key()), to_top, |ui| {
            header(ui, section, filters, drawer, theme);
        });

        let mut overlay = std::mem::take(&mut self.drawer);
        overlay.paint(ui, theme, "cinebox-discover-filters", |ui, theme| {
            filters_drawer(ui, section, &mut self.filters, theme);
        });
        self.drawer = overlay;

        if self.filters != before {
            self.filters.fit_kind();
            self.grid.reset();
            self.reset_scroll = true;
            return out.action;
        }

        if out.wants_page {
            let tmdb = jobs::TmdbCtx::from(&svc.settings);
            let query = self.filters.query(section);
            self.grid
                .request(move |page| jobs::load_discover_page(tmdb, query, page));
        }

        out.action
    }
}

fn header(
    ui: &mut Ui,
    section: Section,
    filters: &mut DiscoverFilters,
    drawer: &mut Overlay,
    theme: &Theme,
) {
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(t!("discover.title").as_ref())
                .font(theme.title_font(theme.text_heading))
                .color(theme.title),
        );
        ui.label(
            RichText::new(crate::i18n::section_title(section).as_ref())
                .font(theme.title_font(theme.text_heading))
                .color(theme.muted),
        );

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let count = filters.drawer_count();
            if filters_button(ui, theme, drawer.is_open(), count) {
                drawer.toggle(ui.input(|i| i.time));
            }
        });
    });

    ui.add_space(10.0);
    scroll::horizontal(ui, ("discover-strip", section.as_key()), |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            if section.kinds().len() > 1 {
                kind_chips(ui, theme, section, &mut filters.kind);
                ui.separator();
            }
            genre_chips(ui, theme, filters);
        });
    });

    active_filters(ui, filters, theme);
    ui.add_space(12.0);
}

fn kind_chips(ui: &mut Ui, theme: &Theme, section: Section, kind: &mut MediaKind) {
    for option in section.kinds() {
        let label = crate::i18n::kind_label(*option);
        if button::label(ui, theme, label.as_ref(), Opts::chip(*kind == *option)) {
            *kind = *option;
        }
    }
}

/// "All genres" plus one toggle per genre; lays out in whatever row or wrap the caller opened.
fn genre_chips(ui: &mut Ui, theme: &Theme, filters: &mut DiscoverFilters) {
    let all = t!("discover.all_genres");
    if button::label(ui, theme, all.as_ref(), Opts::chip(filters.genres.is_empty())) {
        filters.genres.clear();
    }

    for genre in genres_for(filters.kind) {
        let name = crate::i18n::genre_name(*genre);
        let active = filters.genres.contains(genre);
        if button::label(ui, theme, name.as_ref(), Opts::chip(active)) {
            chips::toggle(&mut filters.genres, *genre);
        }
    }
}

/// Removable chips for filters set in the drawer, so they stay visible with it closed.
fn active_filters(ui: &mut Ui, filters: &mut DiscoverFilters, theme: &Theme) {
    if filters.drawer_count() == 0 {
        return;
    }

    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        if let Some(decade) = filters.decade
            && removable_chip(ui, theme, &decade.label())
        {
            filters.decade = None;
        }

        if let Some(vote) = filters.min_vote
            && removable_chip(ui, theme, &format!("{vote}+"))
        {
            filters.min_vote = None;
        }

        if filters.sort != DiscoverSort::Popular
            && removable_chip(ui, theme, &crate::i18n::discover_sort_label(filters.sort))
        {
            filters.sort = DiscoverSort::Popular;
        }
    });
}

fn removable_chip(ui: &mut Ui, theme: &Theme, label: &str) -> bool {
    let pad_y = button::icon_label_pad_y(ui, theme, ICON_CLOSE, button::CHIP_H);
    let opts = Opts::chip(true).pad_y(pad_y);

    button::icon_label(ui, theme, ICON_CLOSE, label, opts)
}

fn filters_button(ui: &mut Ui, theme: &Theme, open: bool, count: usize) -> bool {
    let label = if count > 0 {
        format!("{} ({count})", t!("discover.filters"))
    } else {
        t!("discover.filters").into_owned()
    };

    let size = vec2(FILTERS_BTN_W, combo::HEIGHT);
    let pad_y = button::icon_label_pad_y(ui, theme, ICON_FILTER_LIST, combo::HEIGHT);
    let opts = Opts::secondary(size).selected(open || count > 0).pad_y(pad_y);

    button::icon_label(ui, theme, ICON_FILTER_LIST, &label, opts)
}

fn filters_drawer(ui: &mut Ui, section: Section, filters: &mut DiscoverFilters, theme: &Theme) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(t!("discover.filters").as_ref())
                .font(theme.title_font(theme.text_display))
                .color(theme.title),
        );

        let defaults = DiscoverFilters::new(filters.kind);
        if *filters == defaults {
            return;
        }

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let size = vec2(button::CHIP_MIN_W, button::CHIP_H);
            if button::label(ui, theme, t!("discover.reset").as_ref(), Opts::secondary(size)) {
                *filters = defaults;
            }
        });
    });

    ui.add_space(12.0);
    scroll::vertical(ui, "discover-filters", |ui| {
        if section.kinds().len() > 1 {
            section_label(ui, theme, t!("discover.type").as_ref());
            wrapped(ui, |ui| kind_chips(ui, theme, section, &mut filters.kind));
        }

        section_label(ui, theme, t!("discover.genres").as_ref());
        wrapped(ui, |ui| genre_chips(ui, theme, filters));

        section_label(ui, theme, t!("discover.decade").as_ref());
        let all_years = t!("discover.all_years");
        wrapped(ui, |ui| {
            optional_chips(ui, theme, &mut filters.decade, &Decade::ALL, &all_years, Decade::label);
        });

        section_label(ui, theme, t!("discover.rating").as_ref());
        let any = t!("discover.any");
        wrapped(ui, |ui| {
            optional_chips(ui, theme, &mut filters.min_vote, &RATINGS, &any, |vote| format!("{vote}+"));
        });

        section_label(ui, theme, t!("discover.sort").as_ref());
        wrapped(ui, |ui| {
            for sort in DiscoverSort::available(filters.kind) {
                let label = crate::i18n::discover_sort_label(*sort);
                if button::label(ui, theme, label.as_ref(), Opts::chip(filters.sort == *sort)) {
                    filters.sort = *sort;
                }
            }
        });
    });
}

fn wrapped(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        add(ui);
    });
}

fn section_label(ui: &mut Ui, theme: &Theme, label: &str) {
    ui.add_space(14.0);
    ui.label(
        RichText::new(label)
            .size(theme.text_small)
            .color(theme.muted_bright),
    );
}

/// "Any" (`any_label`) or one of `options`.
fn optional_chips<T: Copy + PartialEq>(
    ui: &mut Ui,
    theme: &Theme,
    value: &mut Option<T>,
    options: &[T],
    any_label: &str,
    label: impl Fn(T) -> String,
) {
    if button::label(ui, theme, any_label, Opts::chip(value.is_none())) {
        *value = None;
    }

    for option in options {
        let active = *value == Some(*option);
        if button::label(ui, theme, &label(*option), Opts::chip(active)) {
            *value = Some(*option);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cinebox_tmdb::genre;

    mod fit_kind {
        use super::*;

        #[test]
        fn drops_genres_the_new_kind_does_not_offer() {
            let mut filters = DiscoverFilters::with_genre(MediaKind::Movie, genre::THRILLER);
            filters.kind = MediaKind::Tv;

            filters.fit_kind();

            assert!(filters.genres.is_empty());
        }

        #[test]
        fn replaces_box_office_sort_for_tv() {
            let mut filters = DiscoverFilters::new(MediaKind::Movie);
            filters.sort = DiscoverSort::Revenue;
            filters.kind = MediaKind::Tv;

            filters.fit_kind();

            assert_eq!(filters.sort, DiscoverSort::Popular);
        }
    }

    mod query {
        use super::*;

        #[test]
        fn keeps_section_scope_alongside_user_genres() {
            let filters = DiscoverFilters::with_genre(MediaKind::Tv, genre::COMEDY);

            let query = filters.query(Section::Anime);

            assert_eq!(query.genres, vec![genre::ANIMATION, genre::COMEDY]);
        }

        #[test]
        fn decade_sets_release_window() {
            let mut filters = DiscoverFilters::new(MediaKind::Movie);
            filters.decade = Some(Decade::From(1990));

            let query = filters.query(Section::Movies);

            assert_eq!(
                (query.released_from, query.released_to),
                (Some(Date::jan1(1990)), Some(Date::dec31(1999)))
            );
        }

        #[test]
        fn rating_filter_adds_vote_floor() {
            let mut filters = DiscoverFilters::new(MediaKind::Movie);
            filters.min_vote = Some(7);

            let query = filters.query(Section::Movies);

            assert_eq!(query.vote_count_min, Some(RATING_FILTER_MIN_VOTES));
        }
    }
}
