//! Left navigation rail: icons only, widening over the content with labels while hovered.

use cinebox_core::Section;
use egui::{
    Area, Color32, CornerRadius, Id, Order, Rect, Sense, Stroke, Ui, WidgetInfo, WidgetType, pos2,
    vec2,
};
use egui_material_icons::MaterialIcon;
use egui_material_icons::icons::{
    ICON_AUTO_AWESOME, ICON_BOOKMARKS, ICON_HOME, ICON_LIVE_TV, ICON_MOVIE, ICON_TOYS,
};
use rust_i18n::t;

use crate::nav::RailEntry;
use crate::theme::Theme;
use crate::widgets::button::pointing;

/// Collapsed width; content is laid out against this.
pub const WIDTH: f32 = 60.0;
const EXPANDED_W: f32 = 216.0;
const ITEM_H: f32 = 44.0;
const ITEM_GAP: f32 = 4.0;
const ITEM_INSET: f32 = 8.0;
const ACCENT_W: f32 = 3.0;
const EXPAND_SECS: f32 = 0.16;
/// The title bar's 1px bottom rule spans `bottom - 1.5 ..= bottom - 0.5`. The rail covers it
/// and starts its own edge at the rule's top, so bar and rail meet in one clean corner.
const BAR_RULE_TOP: f32 = 1.5;
const BAR_RULE_BOTTOM: f32 = 0.5;

const ENTRIES: [RailEntry; 6] = [
    RailEntry::Home,
    RailEntry::Section(Section::Movies),
    RailEntry::Section(Section::Cartoons),
    RailEntry::Section(Section::Tv),
    RailEntry::Section(Section::Anime),
    RailEntry::Library,
];

fn icon(entry: RailEntry) -> MaterialIcon {
    match entry {
        RailEntry::Home => ICON_HOME,
        RailEntry::Section(Section::Movies) => ICON_MOVIE,
        RailEntry::Section(Section::Cartoons) => ICON_TOYS,
        RailEntry::Section(Section::Tv) => ICON_LIVE_TV,
        RailEntry::Section(Section::Anime) => ICON_AUTO_AWESOME,
        RailEntry::Library => ICON_BOOKMARKS,
    }
}

fn label(entry: RailEntry) -> std::borrow::Cow<'static, str> {
    match entry {
        RailEntry::Home => t!("rail.home"),
        RailEntry::Section(section) => crate::i18n::section_title(section),
        RailEntry::Library => t!("rail.library"),
    }
}

/// X of the rail's icon column for a rail whose left edge is `left`, so the title bar's
/// Back button can line up with the rail icons.
#[must_use]
pub fn column_center(left: f32) -> f32 {
    left + WIDTH * 0.5
}

/// Paint the rail along the left edge of `body`, which starts at the title bar's bottom edge.
/// Returns the entry clicked this frame.
pub fn show(ui: &Ui, body: Rect, theme: &Theme, active: Option<RailEntry>) -> Option<RailEntry> {
    let ctx = ui.ctx();
    let id = Id::new("cinebox-rail");
    let hovered = ctx
        .read_response(id)
        .is_some_and(|response| response.contains_pointer());
    let t = ctx.animate_bool_with_time(id.with("expand"), hovered, EXPAND_SECS);
    let width = egui::lerp(WIDTH..=EXPANDED_W, t);
    let top = body.top() - BAR_RULE_TOP - BAR_RULE_BOTTOM;
    let rect = Rect::from_min_max(pos2(body.left(), top), pos2(body.left() + width, body.bottom()));
    let edge_top = body.top() - BAR_RULE_TOP;
    let mut clicked = None;

    Area::new(id.with("area"))
        .order(Order::Middle)
        .fixed_pos(rect.min)
        .constrain(false)
        .show(ctx, |ui| {
            // Area hit-testing uses its content size, so claim the whole strip.
            ui.set_min_size(rect.size());
            let _ = ui.interact(rect, id, Sense::hover());

            if t > 0.0 {
                let shadow = egui::epaint::Shadow {
                    offset: [0, 0],
                    blur: 24,
                    spread: 0,
                    color: theme.overlay_at(t),
                };
                // Only over the content: never onto the title bar or back over the rail.
                let content = Rect::from_min_max(
                    pos2(rect.right(), body.top() - BAR_RULE_BOTTOM),
                    body.right_bottom(),
                );
                ui.painter()
                    .with_clip_rect(content)
                    .add(shadow.as_shape(rect, CornerRadius::ZERO));
            }

            ui.set_clip_rect(rect);
            ui.painter().rect_filled(rect, 0.0, theme.chrome_bg);
            ui.painter().vline(
                rect.right() - 0.5,
                edge_top..=rect.bottom(),
                Stroke::new(1.0, theme.window_edge),
            );

            let mut top = body.top() + ITEM_INSET;
            for (index, entry) in ENTRIES.into_iter().enumerate() {
                if entry == RailEntry::Library {
                    let y = top + ITEM_GAP;
                    let x_range = (rect.left() + ITEM_INSET)..=(rect.right() - ITEM_INSET);
                    ui.painter()
                        .hline(x_range, y, Stroke::new(1.0, theme.window_edge));
                    top += ITEM_GAP * 3.0;
                }

                let item = Rect::from_min_size(
                    pos2(rect.left() + ITEM_INSET, top),
                    vec2(width - ITEM_INSET * 2.0, ITEM_H),
                );
                if item_ui(ui, item, entry, active == Some(entry), t, theme, index) {
                    clicked = Some(entry);
                }
                top += ITEM_H + ITEM_GAP;
            }
        });

    clicked
}

fn item_ui(
    ui: &Ui,
    rect: Rect,
    entry: RailEntry,
    active: bool,
    t: f32,
    theme: &Theme,
    index: usize,
) -> bool {
    let text = label(entry);
    let response = pointing(ui.interact(rect, Id::new(("cinebox-rail-item", index)), Sense::click()));
    response.widget_info(|| WidgetInfo::selected(WidgetType::Button, true, active, text.as_ref()));

    let fill = if response.hovered() {
        theme.widget_hover
    } else if active {
        theme.widget_active
    } else {
        Color32::TRANSPARENT
    };
    let painter = ui.painter();
    painter.rect_filled(rect, theme.rounding(theme.radius_card), fill);

    if active {
        let accent = Rect::from_min_size(
            pos2(rect.left(), rect.center().y - ITEM_H * 0.25),
            vec2(ACCENT_W, ITEM_H * 0.5),
        );
        painter.rect_filled(accent, theme.rounding(ACCENT_W), theme.title);
    }

    let fg = if active { theme.title } else { theme.muted_bright };
    let icon_center = pos2(rect.left() + (WIDTH - ITEM_INSET * 2.0) * 0.5, rect.center().y);
    let glyph = icon(entry);
    painter.text(
        icon_center,
        egui::Align2::CENTER_CENTER,
        glyph.codepoint,
        egui::FontId::new(theme.text_icon_lg, glyph.font_family()),
        fg,
    );

    if t > 0.0 {
        let text_pos = pos2(rect.left() + WIDTH - ITEM_INSET, rect.center().y);
        let color = fg.gamma_multiply(t);
        painter.with_clip_rect(rect).text(
            text_pos,
            egui::Align2::LEFT_CENTER,
            text.as_ref(),
            theme.ui_font(theme.text_label),
            color,
        );
    }

    response.clicked()
}
