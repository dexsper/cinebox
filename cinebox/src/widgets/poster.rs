//! Catalog poster tiles.

use cinebox_core::{CatalogItem, typograph};
use egui::{
    Align2, CornerRadius, FontId, Image, Rect, Sense, Stroke, Ui, Vec2, pos2, text::LayoutJob, vec2,
};
use egui_material_icons::MaterialIcon;
use egui_material_icons::icons::{ICON_BROKEN_IMAGE, ICON_HIDE_IMAGE};

use crate::widgets::button::pointing;
use crate::images::ImageSlot;
use crate::nav::NavAction;
use crate::theme::Theme;

const CAPTION_GAP: f32 = 4.0;
const LINE_GAP: f32 = 2.0;
const TITLE_ROWS: f32 = 2.0;

fn caption_h(ui: &Ui, theme: &Theme) -> f32 {
    let (title_rows_h, year_h) = ui.ctx().fonts_mut(|f| {
        (
            f.row_height(&theme.ui_font(theme.text_small)) * TITLE_ROWS,
            f.row_height(&theme.ui_font(theme.text_caption)),
        )
    });

    CAPTION_GAP + title_rows_h + LINE_GAP + year_h
}

pub fn catalog_tile(
    ui: &mut Ui,
    item: &CatalogItem,
    poster: ImageSlot<'_>,
    theme: &Theme,
) -> Option<NavAction> {
    let pad = theme.ring_pad();
    let well = vec2(
        theme.tile_w + pad * 2.0,
        pad * 2.0 + theme.tile_h + caption_h(ui, theme),
    );
    let (rect, response) = ui.allocate_exact_size(well, Sense::click());
    let response = pointing(response);
    let poster_rect =
        Rect::from_min_size(rect.min + vec2(pad, pad), vec2(theme.tile_w, theme.tile_h));

    paint_poster(ui, poster_rect, poster, theme);
    if let Some(vote) = item.vote.filter(|v| *v > 0.0) {
        vote_badge(ui, poster_rect, vote, theme);
    }

    if response.hovered() {
        hover_ring(ui, poster_rect, theme);
    }

    let title_pos = pos2(poster_rect.left(), poster_rect.bottom() + CAPTION_GAP);
    let title = wrap_title(ui, &typograph(&item.title), theme);
    let title_h = title.size().y;

    ui.painter().galley(title_pos, title, theme.title);
    let year = item
        .year
        .map(|year| year.to_string())
        .unwrap_or_else(|| String::from("—"));

    ui.painter().text(
        title_pos + vec2(0.0, title_h + LINE_GAP),
        egui::Align2::LEFT_TOP,
        year,
        theme.ui_font(theme.text_caption),
        theme.muted,
    );

    if response.clicked() {
        return Some(NavAction::OpenMedia { item: item.clone() });
    }
    None
}

pub fn hover_ring(ui: &Ui, poster: Rect, theme: &Theme) {
    let pad = theme.ring_pad();
    let ring = poster.expand(pad);
    let radius = theme.radius_poster + pad;
    ui.painter().rect_stroke(
        ring,
        CornerRadius::same(radius.round() as u8),
        Stroke::new(theme.ring_w, theme.ring),
        egui::StrokeKind::Inside,
    );
}

fn wrap_title(ui: &Ui, title: &str, theme: &Theme) -> std::sync::Arc<egui::Galley> {
    wrap_lines(
        ui,
        title,
        theme.title,
        theme.text_small,
        theme.tile_w,
        2,
        theme,
    )
}

/// Wrap `text` to `max_rows` at `width`, ellipsizing overflow.
pub fn wrap_lines(
    ui: &Ui,
    text: &str,
    color: egui::Color32,
    font_size: f32,
    width: f32,
    max_rows: usize,
    theme: &Theme,
) -> std::sync::Arc<egui::Galley> {
    let mut job = LayoutJob::simple(
        text.to_owned(),
        theme.ui_font(font_size),
        color,
        width,
    );
    job.wrap.max_rows = max_rows;
    job.wrap.break_anywhere = false;
    job.wrap.overflow_character = Some('…');
    ui.painter().layout_job(job)
}

pub fn paint_poster(ui: &Ui, rect: Rect, poster: ImageSlot<'_>, theme: &Theme) {
    let rounding = theme.rounding(theme.radius_poster);
    ui.painter()
        .rect_filled(rect, rounding, theme.poster_placeholder);

    match poster {
        ImageSlot::Ready(texture) => {
            let image = Image::new(texture)
                .fit_to_exact_size(rect.size())
                .corner_radius(rounding)
                .maintain_aspect_ratio(false);
            image.paint_at(ui, rect);
        }
        ImageSlot::Loading => paint_slot_icon(ui, rect, ICON_BROKEN_IMAGE, theme),
        ImageSlot::Missing => paint_slot_icon(ui, rect, ICON_HIDE_IMAGE, theme),
    }
}

fn paint_slot_icon(ui: &Ui, rect: Rect, icon: MaterialIcon, theme: &Theme) {
    let size = (rect.width().min(rect.height()) * 0.34).clamp(28.0, 52.0);
    let galley = ui.painter().layout_no_wrap(
        icon.codepoint.to_owned(),
        FontId::new(size, icon.font_family()),
        theme.muted,
    );
    let pos = Align2::CENTER_CENTER
        .anchor_size(rect.center(), galley.size())
        .min;
    ui.painter().galley(pos, galley, theme.muted);
}

fn vote_badge(ui: &Ui, poster: Rect, vote: f32, theme: &Theme) {
    let text = format!("{vote:.1}");
    let galley = ui
        .painter()
        .layout_no_wrap(text, theme.ui_font(theme.text_caption), theme.rate);

    let size = galley.size() + vec2(12.0, 4.0);
    let rect = Rect::from_min_size(
        pos2(
            poster.right() - size.x - 6.0,
            poster.bottom() - size.y - 6.0,
        ),
        size,
    );

    ui.painter()
        .rect_filled(rect, theme.rounding(theme.radius_badge), theme.badge_bg);
    ui.painter()
        .galley(rect.min + vec2(6.0, 2.0), galley, theme.rate);
}

pub fn rounded_image(ui: &mut Ui, texture: ImageSlot<'_>, size: Vec2, theme: &Theme) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    paint_poster(ui, rect, texture, theme);
}
