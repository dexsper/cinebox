use cinebox_core::i18n::Msg;
use cinebox_core::{HomeCatalog, HomeRow, HomeRowId, language_key};
use egui::{FontId, RichText, Sense, Ui, WidgetInfo, WidgetType, pos2, vec2};
use egui_material_icons::icons::ICON_CHEVRON_RIGHT;

use crate::jobs;
use crate::nav::NavAction;
use crate::services::Services;
use crate::theme::Theme;
use crate::widgets::button::pointing;
use crate::widgets::{self, poster, scroll};

#[derive(Default)]
pub struct HomeScreen {
    cache: super::swr::Cached<HomeCatalog, (HomeCatalog, bool)>,
}

impl HomeScreen {
    pub fn refresh(&mut self) {
        self.cache.invalidate();
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

        self.cache.sync_lang(svc.settings.general.language);

        let lang_key = language_key(Some(svc.settings.general.language.tmdb_code())).to_owned();
        let db = svc.db.clone();
        let hydrated = self.cache.hydrate(async move {
            let db = db?;

            db.home_catalog(&lang_key).await.ok().flatten()
        });

        if !hydrated {
            widgets::page_spinner(ui, theme);
            return None;
        }

        let disk_fresh = self.cache.disk.as_ref().is_some_and(|(_, fresh)| *fresh);
        let tmdb = jobs::TmdbCtx::from(&svc.settings);
        let db = svc.db.clone();
        let outcome = self
            .cache
            .resolve(disk_fresh, move || jobs::load_home(tmdb, db));

        let mut retry = false;
        let action = match outcome.view {
            super::swr::Swr::Live => match self.cache.bind.read() {
                Some(Ok(catalog)) => catalog_view(ui, catalog, svc, theme),
                _ => None,
            },
            super::swr::Swr::Disk => match self.cache.disk.as_ref() {
                Some((catalog, _)) => catalog_view(ui, catalog, svc, theme),
                None => None,
            },
            super::swr::Swr::Failed => {
                let error = match self.cache.bind.read() {
                    Some(Err(error)) => error.to_string(),
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
