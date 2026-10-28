//! Skip-segment banner rendered in the bottom-right corner of the video.

use cinebox_skip::SegmentType;
use egui::{Area, Color32, Frame, Id, Margin, Order, Rect, RichText, Vec2, pos2};
use rust_i18n::t;

use super::skip::{ActiveSegment, SkipState};
use crate::theme::Theme;
use crate::widgets::button::{self, Opts};
use crate::widgets::focus;

const BOTTOM_GAP: f32 = 12.0;
const RIGHT_MARGIN: f32 = 20.0;
const BTN_SIZE: Vec2 = Vec2::new(130.0, 36.0);
const BTN_GAP: f32 = 8.0;
const IDLE_OPACITY: f32 = 0.45;
/// Semi-transparent white countdown fill painted on top of the Skip button.
const COUNTDOWN_TINT: Color32 = Color32::from_rgba_premultiplied(40, 40, 40, 40);

#[derive(Debug, Default)]
pub(super) struct SkipBannerOut {
    pub is_skip_clicked: bool,
    pub is_cancel_clicked: bool,
}

/// Render the Cancel / Skip banner.
///
/// `seek_rect` is `Rect::NOTHING` when the footer overlay is hidden.
pub(super) fn show(
    skip: &SkipState,
    ctx: &egui::Context,
    theme: &Theme,
    video: Rect,
    seek_rect: Rect,
) -> SkipBannerOut {
    let Some(active) = &skip.active else { return SkipBannerOut::default() };

    if active.is_dismissed {
        return SkipBannerOut::default();
    }

    let safe = video - crate::platform::edge_inset(ctx);
    let is_footer_visible = seek_rect != Rect::NOTHING;
    let anchor_bottom = if is_footer_visible { seek_rect.top() - BOTTOM_GAP } else { safe.bottom() - BOTTOM_GAP };

    let anchor = pos2(safe.right() - RIGHT_MARGIN, anchor_bottom);
    let area_id = Id::new("player-skip-banner");

    let is_directional = crate::platform::profile(ctx).is_directional();
    let banner = ctx.read_response(area_id);
    let has_appeared = banner.is_none();
    let is_pointed = banner.as_ref().is_some_and(egui::Response::contains_pointer);
    let focused_layer = focus::focused_layer(ctx);
    let is_focused = is_directional && banner.is_some_and(|response| focused_layer == Some(response.layer_id));
    let is_hovered = is_pointed || is_focused;

    let opacity = if is_hovered { 1.0 } else { IDLE_OPACITY };
    let now = ctx.input(|i| i.time);
    let mut out = SkipBannerOut::default();

    Area::new(area_id)
        .order(Order::Foreground)
        .pivot(egui::Align2::RIGHT_BOTTOM)
        .fixed_pos(anchor)
        .constrain(false)
        .show(ctx, |ui| {
            ui.set_opacity(opacity);

            Frame::new().inner_margin(Margin::ZERO).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = BTN_GAP;

                    let cancel = t!("player.cancel");
                    out.is_cancel_clicked = button::label(ui, theme, cancel.as_ref(), Opts::secondary(BTN_SIZE));

                    let countdown = active.countdown_frac(now);
                    let is_counting = active.countdown_started_at.is_some();
                    // On TV only when it appears: grabbing every frame would trap the D-pad.
                    let can_grab_focus = if is_directional { has_appeared } else { !is_counting };
                    out.is_skip_clicked = skip_btn(ui, theme, active, countdown, can_grab_focus);
                });
            });
        });

    if active.countdown_started_at.is_some() {
        ctx.request_repaint();
    }

    out
}

/// `countdown` is the share of the auto-skip countdown already run, from 0 to 1.
fn skip_btn(ui: &mut egui::Ui, theme: &Theme, active: &ActiveSegment, countdown: f32, can_grab_focus: bool) -> bool {
    let caption: std::borrow::Cow<str> = match active.ty {
        SegmentType::Intro => t!("player.skip_intro"),
        SegmentType::Recap => t!("player.skip_recap"),
        SegmentType::Credits => t!("player.skip_credits"),
        SegmentType::Preview => t!("player.skip_preview"),
    };

    let text = RichText::new(caption.as_ref()).size(theme.text_body).color(theme.btn_primary_fg);

    let response = button::add_named(ui, theme, text, Opts::primary(BTN_SIZE), Some(caption.as_ref()));

    if countdown > 0.0 {
        let rect = response.rect;
        let mut fill_rect = rect;
        fill_rect.max.x = rect.left() + rect.width() * countdown;

        let radius = theme.rounding(theme.radius_card);
        ui.painter().rect_filled(fill_rect, radius, COUNTDOWN_TINT);
    }

    if can_grab_focus {
        response.request_focus();
    }

    // A remote's OK clicks whatever is focused, Cancel included.
    let is_directional = crate::platform::profile(ui.ctx()).is_directional();
    let is_enter_pressed = !is_directional && ui.input(|i| i.key_pressed(egui::Key::Enter));

    response.clicked() || is_enter_pressed
}
