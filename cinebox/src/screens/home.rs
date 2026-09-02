use cinebox_core::i18n::Msg;
use cinebox_core::{HomeCatalog, HomeRow, HomeRowId, UiLanguage, language_key};
use egui::{FontId, RichText, Sense, Ui, WidgetInfo, WidgetType, pos2, vec2};
use egui_async::Bind;
use egui_material_icons::icons::ICON_CHEVRON_RIGHT;

use crate::jobs;
use crate::nav::NavAction;
use crate::services::{Services, db_block_on};
use crate::theme::Theme;
use crate::widgets::button::pointing;
use crate::widgets::{self, poster, scroll};

pub struct HomeScreen {
    catalog: Bind<HomeCatalog, String>,
    disk: Option<HomeCatalog>,
    disk_fresh: bool,
    lang: Option<UiLanguage>,
    force_refresh: bool,
}

impl Default for HomeScreen {
    fn default() -> Self {
        Self {
            catalog: Bind::new(true),
            disk: None,
            disk_fresh: false,
            lang: None,
            force_refresh: false,
        }
    }
}

impl HomeScreen {
    pub fn refresh(&mut self) {
        self.catalog.clear();
        self.disk = None;
        self.disk_fresh = false;
        self.force_refresh = true;
    }

    pub fn ui(&mut self, ui: &mut Ui, svc: &mut Services, theme: &Theme) -> Option<NavAction> {
        if svc.settings.tmdb.api_key.is_empty() {
            ui.label(RichText::new(Msg::NeedTmdbKey.t()).color(theme.muted));
            let settings_size = egui::vec2(160.0, crate::widgets::combo::HEIGHT);
            if crate::widgets::button::label(
                ui,
                theme,
                Msg::NavSettings.t(),
                crate::widgets::button::Opts::secondary(settings_size),
            ) {
                return Some(NavAction::OpenSettings);
            }
            return None;
        }

        let lang = svc.settings.general.language;
        if self.lang != Some(lang) {
            let switched = self.lang.is_some();
            self.lang = Some(lang);
            if switched {
                self.catalog = Bind::new(true);
                self.disk = None;
                self.disk_fresh = false;
                self.force_refresh = true;
            }
        }

        if self.disk.is_none() {
            let lang_key = language_key(Some(svc.settings.general.language.tmdb_code()));
            if let Some((catalog, fresh)) = svc
                .db
                .as_ref()
                .and_then(|db| db_block_on(db.home_catalog(lang_key)).ok().flatten())
            {
                self.disk = Some(catalog);
                self.disk_fresh = fresh;
            }
        }

        let disk_catalog = self.disk.as_ref();
        let skip_network = !self.force_refresh && self.disk_fresh;
        let settings = svc.settings.clone();
        let db = svc.db.clone();
        let outcome = super::swr::resolve(
            &mut self.catalog,
            disk_catalog.is_some(),
            skip_network,
            move || jobs::load_home(settings, db),
        );
        if outcome.from_network {
            self.force_refresh = false;
        }

        let mut retry = false;
        let action = match outcome.view {
            super::swr::Swr::Live => match self.catalog.read() {
                Some(Ok(catalog)) => catalog_view(ui, catalog, svc, theme),
                _ => None,
            },
            super::swr::Swr::Disk => match disk_catalog {
                Some(catalog) => catalog_view(ui, catalog, svc, theme),
                None => None,
            },
            super::swr::Swr::Failed => {
                let error = match self.catalog.read() {
                    Some(Err(error)) => error.clone(),
                    _ => Msg::Failed.t().to_owned(),
                };
                retry = widgets::page_error(ui, theme, &error);
                None
            }
            super::swr::Swr::Pending => {
                widgets::page_spinner(ui, theme);
                None
            }
        };
        if retry {
            self.refresh();
        }

        action
    }
}

fn catalog_view(
    ui: &mut Ui,
    catalog: &HomeCatalog,
    svc: &Services,
    theme: &Theme,
) -> Option<NavAction> {
    let mut action = None;
    scroll::vertical(ui, "home-page", |ui| {
        for (index, row) in catalog.rows.iter().enumerate() {
            if let Some(nav) = shelf(ui, row, index, svc, theme) {
                action = Some(nav);
            }
        }
    });
    action
}

fn shelf(
    ui: &mut Ui,
    row: &HomeRow,
    index: usize,
    svc: &Services,
    theme: &Theme,
) -> Option<NavAction> {
    ui.add_space(12.0);

    let mut action = None;
    if shelf_heading(ui, row.id, theme) {
        action = Some(NavAction::OpenCategory {
            id: row.id,
            items: row.items.clone(),
        });
    }

    if let Some(error) = &row.error {
        ui.label(RichText::new(error).size(theme.text_small).color(theme.err));
    }
    if row.items.is_empty() {
        if row.error.is_none() {
            ui.label(
                RichText::new(Msg::EmptyRow.t())
                    .size(theme.text_small)
                    .color(theme.muted),
            );
        }
        return action;
    }
    scroll::horizontal(ui, format!("home-row-{index}"), |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            for item in &row.items {
                let opened = poster::catalog_tile(
                    ui,
                    item,
                    &svc.images,
                    svc.settings.tmdb.poster_size,
                    theme,
                    svc.is_watched(item.kind, item.id),
                );
                if action.is_none() {
                    action = opened;
                }
            }
        });
    });
    action
}

pub(crate) fn shelf_heading(ui: &mut Ui, id: HomeRowId, theme: &Theme) -> bool {
    let title = id.title_msg().t();
    let icon = ICON_CHEVRON_RIGHT;
    let title_font = theme.title_font(theme.text_section);
    let icon_font = FontId::new(theme.text_icon_md, icon.font_family());
    let title_galley = ui
        .painter()
        .layout_no_wrap(title.to_owned(), title_font, theme.title);
    
    let icon_galley = ui.painter().layout_no_wrap(
        icon.codepoint.to_owned(),
        icon_font,
        theme.muted,
    );
    
    let gap = 4.0;
    let width = title_galley.size().x + gap + icon_galley.size().x;
    let height = title_galley.size().y.max(icon_galley.size().y);
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    let response = pointing(response);
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, title));

    let title_pos = pos2(rect.left(), rect.center().y - title_galley.size().y * 0.5);
    ui.painter().galley(title_pos, title_galley, theme.title);

    let icon_pos = pos2(
        rect.right() - icon_galley.size().x,
        rect.center().y - icon_galley.size().y * 0.5,
    );
    ui.painter().galley(icon_pos, icon_galley, theme.muted);

    response.clicked()
}
