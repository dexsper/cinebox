//! Floating auto-hiding player chrome: header (title) and footer (transport).

use cinebox_player::{SEEK_SECS, format_clock};
use egui::{
    Align, Area, Color32, CornerRadius, Frame, Id, Label, Layout, Margin, Mesh, Order, Pos2, Rect,
    Response, RichText, Sense, Stroke, Ui, UiBuilder, Vec2, epaint::Vertex, pos2, vec2,
};
use egui_material_icons::MaterialIcon;
use egui_material_icons::icons::{
    ICON_FORWARD_10, ICON_FULLSCREEN, ICON_FULLSCREEN_EXIT, ICON_PAUSE, ICON_PLAY_ARROW,
    ICON_PLAYLIST_PLAY, ICON_REPLAY_10, ICON_SETTINGS, ICON_SKIP_NEXT, ICON_SKIP_PREVIOUS,
    ICON_VOLUME_MUTE, ICON_VOLUME_OFF, ICON_VOLUME_UP,
};
use rust_i18n::t;

use crate::theme::Theme;

/// Seconds the overlay stays fully visible after the last input.
pub const HOLD: f64 = 3.0;
/// Seconds the fade-out takes after [`HOLD`] elapses.
pub const FADE: f64 = 0.3;

const FOOTER_H: f32 = 116.0;
const BTN: f32 = 44.0;
const BTN_GAP: f32 = 8.0;
const SEEK_H: f32 = 18.0;
const SEEK_TRACK_H: f32 = 4.0;

/// Auto-hide tracker: any input resets the clock, the overlay fades after
/// [`HOLD`] seconds over [`FADE`] seconds.
pub struct Activity {
    last_at: f64,
}

impl Activity {
    #[must_use]
    pub fn new() -> Self {
        Self { last_at: 0.0 }
    }

    pub fn poke(&mut self, now: f64) {
        self.last_at = now;
    }

    /// Overlay alpha: `1.0` while held, easing to `0.0` during the fade.
    #[must_use]
    pub fn visual_t(&self, now: f64) -> f32 {
        let idle = now - self.last_at;
        if idle <= HOLD {
            return 1.0;
        }

        (1.0 - (idle - HOLD) / FADE).clamp(0.0, 1.0) as f32
    }
}

/// What the footer needs to render.
pub struct FooterView {
    pub time: f64,
    pub duration: f64,
    pub paused: bool,
    pub muted: bool,
    pub volume: f64,
    pub file_count: usize,
    pub file_index: usize,
    pub has_next: bool,
    pub fullscreen: bool,
}

/// Clicks and rects reported by one footer frame.
pub struct FooterOut {
    pub seek_to: Option<f64>,
    pub seek_rel: Option<f64>,
    pub toggle_pause: bool,
    pub prev: bool,
    pub next: bool,
    pub playlist_clicked: bool,
    pub settings_clicked: bool,
    pub volume_clicked: bool,
    pub volume_hovered: bool,
    pub fullscreen_clicked: bool,
    pub rect: Rect,
    pub playlist_rect: Rect,
    pub settings_rect: Rect,
    pub volume_rect: Rect,
}

impl FooterOut {
    fn empty() -> Self {
        Self {
            seek_to: None,
            seek_rel: None,
            toggle_pause: false,
            prev: false,
            next: false,
            playlist_clicked: false,
            settings_clicked: false,
            volume_clicked: false,
            volume_hovered: false,
            fullscreen_clicked: false,
            rect: Rect::NOTHING,
            playlist_rect: Rect::NOTHING,
            settings_rect: Rect::NOTHING,
            volume_rect: Rect::NOTHING,
        }
    }
}

/// Title over a scrim, top-left of the video. Returns the painted rect.
pub fn header(
    ctx: &egui::Context,
    theme: &Theme,
    video: Rect,
    title: &str,
    alpha: f32,
) -> Option<Rect> {
    if alpha <= 0.01 {
        return None;
    }

    let max_w = (video.width() - 48.0).max(280.0);
    let response = Area::new(Id::new("player-header"))
        .order(Order::Foreground)
        .fixed_pos(video.left_top() + vec2(16.0, 16.0))
        .constrain(false)
        .show(ctx, |ui| {
            ui.set_opacity(alpha);
            ui.set_max_width(max_w);
            Frame::new()
                .fill(theme.badge_bg)
                .corner_radius(theme.rounding(theme.radius_card))
                .inner_margin(Margin::symmetric(14, 10))
                .show(ui, |ui| {
                    ui.set_max_width(max_w - 28.0);
                    ui.add(
                        Label::new(
                            RichText::new(title)
                                .font(theme.title_font(theme.text_subtitle))
                                .color(theme.title),
                        )
                        .wrap(),
                    );
                });
        })
        .response;

    Some(response.rect)
}

/// Transport bar over the bottom of the video.
pub fn footer(ctx: &egui::Context, theme: &Theme, video: Rect, view: &FooterView, alpha: f32) -> FooterOut {
    let mut out = FooterOut::empty();
    if alpha <= 0.01 {
        return out;
    }

    let rect = Rect::from_min_max(pos2(video.left(), video.bottom() - FOOTER_H), video.right_bottom());
    out.rect = rect;

    Area::new(Id::new("player-footer"))
        .order(Order::Foreground)
        .fixed_pos(rect.min)
        .constrain(false)
        .show(ctx, |ui| {
            ui.set_opacity(alpha);
            scrim(ui, rect, theme);

            let inner = rect.shrink2(vec2(20.0, 0.0));
            let mut body = ui.new_child(
                UiBuilder::new()
                    .max_rect(inner)
                    .layout(Layout::top_down(Align::Min)),
            );

            body.add_space(14.0);
            clock_row(&mut body, theme, view);

            out.seek_to = seek_bar(&mut body, theme, view);

            body.add_space(6.0);
            buttons_row(&mut body, theme, view, &mut out);
        });

    out
}

fn clock_row(ui: &mut Ui, theme: &Theme, view: &FooterView) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format_clock(view.time))
                .size(theme.text_small)
                .color(theme.muted_bright),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(format_clock(view.duration))
                    .size(theme.text_small)
                    .color(theme.muted_bright),
            );
        });
    });
}

fn seek_bar(ui: &mut Ui, theme: &Theme, view: &FooterView) -> Option<f64> {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, SEEK_H), Sense::click_and_drag());
    let response = crate::widgets::button::pointing(response);

    let fraction = if view.duration > 0.0 {
        (view.time / view.duration).clamp(0.0, 1.0) as f32
    } else {
        0.0
    };

    let track = Rect::from_center_size(rect.center(), vec2(rect.width(), SEEK_TRACK_H));
    ui.painter()
        .rect_filled(track, CornerRadius::same(2), theme.progress_track);

    let mut fill = track;
    fill.max.x = track.left() + track.width() * fraction;
    ui.painter()
        .rect_filled(fill, CornerRadius::same(2), theme.progress_fill);

    let engaged = response.hovered() || response.dragged();
    if engaged {
        let thumb = pos2(fill.max.x, track.center().y);
        ui.painter().circle_filled(thumb, 6.0, theme.progress_fill);
    }

    let scrubbing = response.clicked() || response.dragged();
    if !scrubbing || view.duration <= 0.0 {
        return None;
    }

    let pos = response.interact_pointer_pos()?;
    let fraction = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);

    Some(f64::from(fraction) * view.duration)
}

fn buttons_row(ui: &mut Ui, theme: &Theme, view: &FooterView, out: &mut FooterOut) {
    let row_h = BTN + 4.0;
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), row_h), Sense::hover());

    right_cluster(ui, theme, view, row, out);
    center_cluster(ui, theme, view, row, out);
}

fn center_cluster(ui: &mut Ui, theme: &Theme, view: &FooterView, row: Rect, out: &mut FooterOut) {
    let show_skip = view.file_count > 1;
    let count = if show_skip { 5 } else { 3 };
    let width = count as f32 * BTN + (count - 1) as f32 * BTN_GAP;
    let cluster = Rect::from_center_size(row.center(), vec2(width, row.height()));

    let mut center = ui.new_child(
        UiBuilder::new()
            .max_rect(cluster)
            .layout(Layout::left_to_right(Align::Center)),
    );
    center.spacing_mut().item_spacing.x = BTN_GAP;

    if show_skip {
        let prev = icon_btn(
            &mut center,
            theme,
            ICON_SKIP_PREVIOUS,
            t!("nav.back").as_ref(),
            view.file_index > 0,
        );
        out.prev = prev.clicked();
    }

    let rewind = icon_btn(&mut center, theme, ICON_REPLAY_10, "−10s", true);
    if rewind.clicked() {
        out.seek_rel = Some(-SEEK_SECS);
    }

    let (icon, hint) = if view.paused {
        (ICON_PLAY_ARROW, t!("player.play"))
    } else {
        (ICON_PAUSE, t!("player.pause"))
    };
    out.toggle_pause = icon_btn(&mut center, theme, icon, hint.as_ref(), true).clicked();

    let forward = icon_btn(&mut center, theme, ICON_FORWARD_10, "+10s", true);
    if forward.clicked() {
        out.seek_rel = Some(SEEK_SECS);
    }

    if show_skip {
        let next = icon_btn(
            &mut center,
            theme,
            ICON_SKIP_NEXT,
            t!("player.next").as_ref(),
            view.has_next,
        );
        out.next = next.clicked();
    }
}

fn right_cluster(ui: &mut Ui, theme: &Theme, view: &FooterView, row: Rect, out: &mut FooterOut) {
    let mut right = ui.new_child(
        UiBuilder::new()
            .max_rect(row)
            .layout(Layout::right_to_left(Align::Center)),
    );
    right.spacing_mut().item_spacing.x = BTN_GAP;

    let (fs_icon, fs_hint) = if view.fullscreen {
        (ICON_FULLSCREEN_EXIT, t!("player.exit_fullscreen"))
    } else {
        (ICON_FULLSCREEN, t!("player.fullscreen"))
    };
    out.fullscreen_clicked = icon_btn(&mut right, theme, fs_icon, fs_hint.as_ref(), true).clicked();

    let volume = icon_btn(&mut right, theme, volume_icon(view), t!("player.volume").as_ref(), true);
    out.volume_clicked = volume.clicked();
    out.volume_hovered = volume.hovered();
    out.volume_rect = volume.rect;

    let settings = icon_btn(&mut right, theme, ICON_SETTINGS, t!("nav.settings").as_ref(), true);
    out.settings_clicked = settings.clicked();
    out.settings_rect = settings.rect;

    let playlist = if view.file_count > 1 {
        Some(icon_btn(&mut right, theme, ICON_PLAYLIST_PLAY, t!("player.playlist").as_ref(), true))
    } else {
        None
    };

    let Some(playlist) = playlist else {
        return;
    };

    out.playlist_clicked = playlist.clicked();
    out.playlist_rect = playlist.rect;
}

fn volume_icon(view: &FooterView) -> MaterialIcon {
    if view.muted {
        return ICON_VOLUME_OFF;
    }

    if view.volume <= 0.5 {
        return ICON_VOLUME_MUTE;
    }

    ICON_VOLUME_UP
}

fn icon_btn(ui: &mut Ui, theme: &Theme, icon: MaterialIcon, hint: &str, enabled: bool) -> Response {
    let response = ui
        .scope(|ui| {
            let hover = theme.chrome_btn_hover;
            let widgets = &mut ui.visuals_mut().widgets;
            widgets.inactive.bg_fill = Color32::TRANSPARENT;
            widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
            widgets.hovered.bg_fill = hover;
            widgets.hovered.weak_bg_fill = hover;
            widgets.active.bg_fill = hover;
            widgets.active.weak_bg_fill = hover;

            ui.add_enabled(
                enabled,
                egui::Button::new(icon.rich_text().size(theme.text_cta_icon).color(theme.title))
                    .stroke(Stroke::NONE)
                    .corner_radius(8)
                    .min_size(Vec2::splat(BTN)),
            )
        })
        .inner;

    let is_enabled = response.enabled();
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, is_enabled, hint)
    });

    crate::widgets::button::pointing(response).on_hover_text(hint)
}

/// Bottom scrim: transparent at the top edge, dark at the bottom.
fn scrim(ui: &Ui, rect: Rect, theme: &Theme) {
    let mut mesh = Mesh::default();
    let top = Color32::TRANSPARENT;
    let bottom = theme.badge_bg;
    let i = mesh.vertices.len() as u32;

    mesh.vertices.push(vert(rect.left_top(), top));
    mesh.vertices.push(vert(rect.right_top(), top));
    mesh.vertices.push(vert(rect.right_bottom(), bottom));
    mesh.vertices.push(vert(rect.left_bottom(), bottom));
    mesh.indices
        .extend_from_slice(&[i, i + 1, i + 2, i, i + 2, i + 3]);

    ui.painter().add(egui::Shape::mesh(mesh));
}

fn vert(pos: Pos2, color: Color32) -> Vertex {
    Vertex {
        pos,
        uv: Pos2::ZERO,
        color,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_holds_then_fades() {
        let mut activity = Activity::new();
        activity.poke(10.0);

        assert!((activity.visual_t(10.0) - 1.0).abs() < f32::EPSILON);
        assert!((activity.visual_t(10.0 + HOLD) - 1.0).abs() < f32::EPSILON);

        let mid = activity.visual_t(10.0 + HOLD + FADE / 2.0);
        assert!(mid > 0.0 && mid < 1.0, "{mid}");

        assert!((activity.visual_t(10.0 + HOLD + FADE) - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn poke_restores_visibility() {
        let mut activity = Activity::new();
        activity.poke(0.0);
        assert!(activity.visual_t(100.0) < f32::EPSILON);

        activity.poke(100.0);
        assert!((activity.visual_t(100.0) - 1.0).abs() < f32::EPSILON);
    }
}
