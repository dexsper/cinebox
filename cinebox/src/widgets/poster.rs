//! Catalog poster tiles.

use cinebox_core::{CatalogItem, CreditPerson, LibraryMark, MediaKind, PosterSize};
use egui::{
    Align2, CornerRadius, FontId, Image, Rect, Sense, Stroke, Ui, Vec2, pos2, text::LayoutJob, vec2,
};
use egui_material_icons::MaterialIcon;
use egui_material_icons::icons::{ICON_BROKEN_IMAGE, ICON_FAVORITE, ICON_HIDE_IMAGE, ICON_PLAY_ARROW};

use crate::images::{ImageCache, ImageSlot};
use crate::nav::NavAction;
use crate::theme::Theme;
use crate::widgets::button::pointing;

const CAPTION_GAP: f32 = 4.0;
const LINE_GAP: f32 = 2.0;
const TITLE_ROWS: f32 = 2.0;

/// Extra clip margin so a shelf that is about to enter view starts decoding.
const LOAD_MARGIN: f32 = 280.0;

/// Cache key for [`caption_h`]: every input that changes the measured
/// caption height, as bit patterns so the key can derive `Eq`.
#[derive(Clone, Copy, PartialEq, Eq)]
struct CaptionKey {
    text_small: u32,
    text_caption: u32,
    pixels_per_point: u32,
    scale: u32,
}

thread_local! {
    /// Caption height for the key above, so a grid of tiles does not lock
    /// the font atlas twice per tile per frame.
    static CAPTION_H: std::cell::Cell<Option<(CaptionKey, f32)>> =
        const { std::cell::Cell::new(None) };
}

fn caption_h(ui: &Ui, theme: &Theme, scale: f32) -> f32 {
    let key = CaptionKey {
        text_small: theme.text_small.to_bits(),
        text_caption: theme.text_caption.to_bits(),
        pixels_per_point: ui.ctx().pixels_per_point().to_bits(),
        scale: scale.to_bits(),
    };

    let cached = CAPTION_H.get().filter(|(cached_key, _)| *cached_key == key);
    if let Some((_, height)) = cached {
        return height;
    }

    let (title_rows_h, year_h) = ui.ctx().fonts_mut(|f| {
        (
            f.row_height(&theme.ui_font(theme.text_small * scale)) * TITLE_ROWS,
            f.row_height(&theme.ui_font(theme.text_caption * scale)),
        )
    });

    let height = CAPTION_GAP * scale + title_rows_h + LINE_GAP * scale + year_h;
    CAPTION_H.set(Some((key, height)));

    height
}

pub fn in_load_window(ui: &Ui, rect: Rect) -> bool {
    ui.is_rect_visible(rect.expand(LOAD_MARGIN))
}

/// Pass to [`watched_badge`] (and the `scale` param of tile-sizing helpers)
/// for posters that are not catalog tiles, e.g. the hero poster on the media
/// detail page or the torrents explorer's left-pane poster — those keep
/// their existing fixed size and are not part of the responsive grid/shelf
/// scaling driven by [`card_scale`].
pub const UNSCALED: f32 = 1.0;

/// Multiplier for card sizes, driven by the width available to the current
/// row/grid. Today's tile size is the floor: cards never shrink below it,
/// and grow up to `MAX_SCALE` as the window widens. Aspect ratio is kept
/// because callers scale width and height by the same factor.
///
/// Callers must sample `ui.available_width()` from the row/grid's *outer*
/// `Ui`, before entering a `horizontal_wrapped` or `scroll::horizontal` —
/// a horizontal `ScrollArea`'s inner `Ui` reports an effectively unbounded
/// width, not the visible viewport width.
pub fn card_scale(available_w: f32) -> f32 {
    const FLOOR_W: f32 = 750.0;
    const CEIL_W: f32 = 1900.0;
    const MAX_SCALE: f32 = 1.45;

    if available_w <= FLOOR_W {
        return 1.0;
    }

    let t = ((available_w - FLOOR_W) / (CEIL_W - FLOOR_W)).min(1.0);
    1.0 + t * (MAX_SCALE - 1.0)
}

/// Local state painted over a catalog tile.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TileMarks {
    /// Playback was started at least once.
    pub watched: bool,
    pub library: LibraryMark,
}

pub fn catalog_tile(
    ui: &mut Ui,
    item: &CatalogItem,
    images: &ImageCache,
    size: PosterSize,
    theme: &Theme,
    marks: TileMarks,
    scale: f32,
) -> Option<NavAction> {
    let pad = theme.ring_pad();
    let tile_w = theme.tile_w * scale;
    let tile_h = theme.tile_h * scale;
    let well = vec2(
        tile_w + pad * 2.0,
        pad * 2.0 + tile_h + caption_h(ui, theme, scale),
    );

    let (rect, response) = ui.allocate_exact_size(well, Sense::click());
    let response = pointing(response);

    if !in_load_window(ui, rect) {
        if response.clicked() {
            return Some(nav_for_item(item));
        }

        return None;
    }

    let poster_rect = Rect::from_min_size(rect.min + vec2(pad, pad), vec2(tile_w, tile_h));
    let poster = images.poster(item, size);

    paint_poster(ui, poster_rect, poster, theme);
    paint_marks(ui, poster_rect, marks, theme, scale);

    if let Some(vote) = item.vote.filter(|v| *v > 0.0) {
        vote_badge(ui, poster_rect, vote, theme, scale);
    }

    if response.hovered() {
        hover_ring(ui, poster_rect, theme);
    }

    let title_pos = pos2(
        poster_rect.left(),
        poster_rect.bottom() + CAPTION_GAP * scale,
    );

    let title = wrap_title(ui, &item.title, theme, tile_w, scale);
    let title_h = title.size().y;

    ui.painter().galley(title_pos, title, theme.title);
    let year = item
        .year
        .map(|year| year.to_string())
        .unwrap_or_else(|| String::from("—"));

    ui.painter().text(
        title_pos + vec2(0.0, title_h + LINE_GAP * scale),
        egui::Align2::LEFT_TOP,
        year,
        theme.ui_font(theme.text_caption * scale),
        theme.muted,
    );

    if response.clicked() {
        return Some(nav_for_item(item));
    }

    None
}

fn nav_for_item(item: &CatalogItem) -> NavAction {
    if item.kind != MediaKind::Person {
        return NavAction::OpenMedia { item: item.clone() };
    }

    NavAction::OpenPerson {
        person: CreditPerson {
            id: item.id,
            name: item.title.clone(),
            role: String::new(),
            profile_path: item.poster_path.clone(),
        },
    }
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

fn wrap_title(
    ui: &Ui,
    title: &str,
    theme: &Theme,
    width: f32,
    scale: f32,
) -> std::sync::Arc<egui::Galley> {
    wrap_lines(
        ui,
        title,
        theme.title,
        theme.text_small * scale,
        width,
        2,
        theme,
    )
}

/// Wrap `text` to `max_rows` at `width`, ellipsizing overflow.
///
/// The actual layout is memoized by egui's galley cache (keyed by the job),
/// so repeated frames only pay for building the `LayoutJob`.
pub fn wrap_lines(
    ui: &Ui,
    text: &str,
    color: egui::Color32,
    font_size: f32,
    width: f32,
    max_rows: usize,
    theme: &Theme,
) -> std::sync::Arc<egui::Galley> {
    let mut job = LayoutJob::simple(text.to_owned(), theme.ui_font(font_size), color, width);
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

/// Status badge top-left, liked heart top-right. The "started" badge only shows
/// when no status is set, since every status already implies the user knows the title.
pub fn paint_marks(ui: &Ui, poster: Rect, marks: TileMarks, theme: &Theme, scale: f32) {
    let radius = 13.0 * scale;
    let inset = radius + 8.0 * scale;

    if let Some(status) = marks.library.status {
        let center = pos2(poster.left() + inset, poster.top() + inset);
        icon_badge(ui, center, radius, super::lists::status_icon(status), theme.title, theme, scale);
    } else if marks.watched {
        watched_badge(ui, poster, theme, scale);
    }

    if marks.library.liked {
        let center = pos2(poster.right() - inset, poster.top() + inset);
        icon_badge(ui, center, radius, ICON_FAVORITE, theme.liked, theme, scale);
    }
}

fn icon_badge(
    ui: &Ui,
    center: egui::Pos2,
    radius: f32,
    icon: MaterialIcon,
    color: egui::Color32,
    theme: &Theme,
    scale: f32,
) {
    ui.painter().circle_filled(center, radius, theme.badge_bg);

    let font = FontId::new(16.0 * scale, icon.font_family());
    let galley = ui
        .painter()
        .layout_no_wrap(icon.codepoint.to_owned(), font, color);
    let pos = Align2::CENTER_CENTER.anchor_size(center, galley.size()).min;
    ui.painter().galley(pos, galley, color);
}

/// Play-icon badge, top-center: this media has been started at least once.
///
/// `scale` is [`UNSCALED`] for non-tile posters (e.g. the hero poster on the
/// media detail page), and the row/grid's [`card_scale`] for catalog tiles.
pub fn watched_badge(ui: &Ui, poster: Rect, theme: &Theme, scale: f32) {
    let radius = 13.0 * scale;
    let center = pos2(poster.center().x, poster.top() + radius + 8.0 * scale);
    ui.painter().circle_filled(center, radius, theme.badge_bg);

    let icon = ICON_PLAY_ARROW;
    let font = FontId::new(16.0 * scale, icon.font_family());
    let galley = ui
        .painter()
        .layout_no_wrap(icon.codepoint.to_owned(), font, theme.title);

    let pos = Align2::CENTER_CENTER.anchor_size(center, galley.size()).min;
    ui.painter().galley(pos, galley, theme.title);
}

fn vote_badge(ui: &Ui, poster: Rect, vote: f32, theme: &Theme, scale: f32) {
    let text = format!("{vote:.1}");
    let font = theme.ui_font(theme.text_caption * scale);
    let galley = ui.painter().layout_no_wrap(text, font, theme.rate);

    let pad = vec2(12.0, 4.0) * scale;
    let margin = 6.0 * scale;
    let size = galley.size() + pad;
    let rect = Rect::from_min_size(
        pos2(
            poster.right() - size.x - margin,
            poster.bottom() - size.y - margin,
        ),
        size,
    );

    ui.painter()
        .rect_filled(rect, theme.rounding(theme.radius_badge), theme.badge_bg);
    ui.painter()
        .galley(rect.min + pad * 0.5, galley, theme.rate);
}

pub fn rounded_image<'a>(
    ui: &mut Ui,
    size: Vec2,
    theme: &Theme,
    texture: impl FnOnce() -> ImageSlot<'a>,
) -> Rect {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    if !in_load_window(ui, rect) {
        return rect;
    }

    paint_poster(ui, rect, texture(), theme);
    rect
}

#[cfg(test)]
mod tests {
    use super::*;
    use cinebox_core::TmdbId;

    fn item(kind: MediaKind, title: &str) -> CatalogItem {
        CatalogItem {
            id: TmdbId::new(7),
            kind,
            title: title.to_owned(),
            year: None,
            vote: None,
            poster_path: Some(String::from("/p.jpg")),
        }
    }

    #[test]
    fn person_opens_person_screen() {
        let NavAction::OpenPerson { person } = nav_for_item(&item(MediaKind::Person, "Tim")) else {
            panic!("expected OpenPerson");
        };

        assert_eq!(person.id, TmdbId::new(7));
        assert_eq!(person.name, "Tim");
        assert_eq!(person.profile_path.as_deref(), Some("/p.jpg"));
    }

    #[test]
    fn movie_opens_media() {
        assert!(matches!(
            nav_for_item(&item(MediaKind::Movie, "Dune")),
            NavAction::OpenMedia { .. }
        ));
    }

    #[test]
    fn card_scale_floors_at_todays_size_on_narrow_windows() {
        assert_eq!(card_scale(600.0), 1.0);
        assert_eq!(card_scale(750.0), 1.0);
    }

    #[test]
    fn card_scale_grows_between_floor_and_ceiling() {
        let narrow = card_scale(900.0);
        let default_window = card_scale(1248.0);
        let wide = card_scale(1900.0);

        assert!(narrow > 1.0, "narrow={narrow}");
        assert!(default_window > narrow, "default={default_window}");
        assert!(wide > default_window, "wide={wide}");
    }

    #[test]
    fn card_scale_caps_beyond_ceiling() {
        assert_eq!(card_scale(1900.0), card_scale(4000.0));
    }
}
