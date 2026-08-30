//! Catalog poster tiles.

use cinebox_core::{CatalogItem, typograph};
use egui::{
    CornerRadius, FontId, Image, Rect, Sense, Stroke, TextureHandle, Ui, Vec2, pos2, text::LayoutJob,
    vec2,
};

use crate::nav::NavAction;
use crate::theme::Theme;

pub fn catalog_tile(
    ui: &mut Ui,
    item: &CatalogItem,
    poster: Option<&TextureHandle>,
    theme: &Theme,
) -> Option<NavAction> {
    let pad = theme.ring_pad();
    let well = vec2(theme.tile_w + pad * 2.0, theme.catalog_shelf_height() - 8.0);
    let (rect, response) = ui.allocate_exact_size(well, Sense::click());
    let poster_rect = Rect::from_min_size(rect.min + vec2(pad, pad), vec2(theme.tile_w, theme.tile_h));
   
    paint_poster(ui, poster_rect, poster, theme);
    if let Some(vote) = item.vote.filter(|v| *v > 0.0) {
        vote_badge(ui, poster_rect, vote, theme);
    }

    if response.hovered() {
        let ring = Rect::from_min_size(rect.min, vec2(theme.tile_w + pad * 2.0, theme.tile_h + pad * 2.0));
        ui.painter().rect_stroke(
            ring,
            CornerRadius::same((theme.radius_poster + pad).round() as u8),
            Stroke::new(theme.ring_w, theme.ring),
            egui::StrokeKind::Inside,
        );
    }

    let title_pos = pos2(poster_rect.left(), poster_rect.bottom() + 4.0);
    let title = wrap_title(ui, &typograph(&item.title), theme);
    let title_h = title.size().y;
    
    ui.painter().galley(title_pos, title, theme.title);
    let year = item
        .year
        .map(|year| year.to_string())
        .unwrap_or_else(|| String::from("—"));

    ui.painter().text(
        title_pos + vec2(0.0, title_h + 2.0),
        egui::Align2::LEFT_TOP,
        year,
        FontId::proportional(12.0),
        theme.muted,
    );

    if response.clicked() {
        return Some(NavAction::OpenMedia {
            kind: item.kind,
            id: item.id,
        });
    }
    None
}

fn wrap_title(ui: &Ui, title: &str, theme: &Theme) -> std::sync::Arc<egui::Galley> {
    let mut job = LayoutJob::simple(
        title.to_owned(),
        FontId::proportional(13.0),
        theme.title,
        theme.tile_w,
    );

    job.wrap.max_rows = 2;
    job.wrap.break_anywhere = false;
    job.wrap.overflow_character = Some('…');
    ui.painter().layout_job(job)
}

pub fn paint_poster(ui: &Ui, rect: Rect, poster: Option<&TextureHandle>, theme: &Theme) {
    let rounding = theme.rounding(theme.radius_poster);
    ui.painter()
        .rect_filled(rect, rounding, theme.poster_placeholder);

    if let Some(texture) = poster {
        let image = Image::new(texture)
            .fit_to_exact_size(rect.size())
            .corner_radius(rounding)
            .maintain_aspect_ratio(false);
        image.paint_at(ui, rect);
    }
}

fn vote_badge(ui: &Ui, poster: Rect, vote: f32, theme: &Theme) {
    let text = format!("{vote:.1}");
    let galley = ui.painter().layout_no_wrap(
        text,
        FontId::proportional(12.0),
        theme.rate,
    );

    let size = galley.size() + vec2(12.0, 4.0);
    let rect = Rect::from_min_size(
        pos2(poster.right() - size.x - 6.0, poster.bottom() - size.y - 6.0),
        size,
    );
    
    ui.painter()
        .rect_filled(rect, theme.rounding(theme.radius_badge), theme.badge_bg);
    ui.painter().galley(rect.min + vec2(6.0, 2.0), galley, theme.rate);
}

pub fn rounded_image(
    ui: &mut Ui,
    texture: Option<&TextureHandle>,
    size: Vec2,
    theme: &Theme,
) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    paint_poster(ui, rect, texture, theme);
}
