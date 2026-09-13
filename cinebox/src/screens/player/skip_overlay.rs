//! Skip-segment banner rendered in the bottom-right corner of the video.

use egui::{Area, Color32, Frame, Id, Margin, Order, Rect, RichText, Vec2, pos2};
use rust_i18n::t;

use crate::theme::Theme;
use crate::widgets::button::{self, Opts};

use super::skip::{ActiveSegment, SkipState};
use cinebox_skip::SegmentType;

const BOTTOM_GAP: f32 = 12.0;
const RIGHT_MARGIN: f32 = 20.0;
const BTN_MIN_W: f32 = 130.0;
const BTN_H: f32 = 36.0;
const BTN_GAP: f32 = 8.0;
const IDLE_OPACITY: f32 = 0.45;
/// Semi-transparent white countdown fill painted on top of the Skip button.
const COUNTDOWN_TINT: Color32 = Color32::from_rgba_premultiplied(40, 40, 40, 40);

pub struct SkipBannerOut {
    pub skip_clicked: bool,
    pub cancel_clicked: bool,
}

impl SkipBannerOut {
    fn none() -> Self {
        Self {
            skip_clicked: false,
            cancel_clicked: false,
        }
    }
}

/// Render the Cancel / Skip banner.
///
/// `seek_rect` is `Rect::NOTHING` when the footer overlay is hidden.
/// `now` is `ui.input(|i| i.time)`.
pub fn show(
    skip: &SkipState,
    ctx: &egui::Context,
    theme: &Theme,
    video: Rect,
    seek_rect: Rect,
    now: f64,
) -> SkipBannerOut {
    let Some(active) = &skip.active else {
        return SkipBannerOut::none();
    };

    if active.dismissed {
        return SkipBannerOut::none();
    }

    let footer_visible = seek_rect != Rect::NOTHING;
    let anchor_bottom = if footer_visible {
        seek_rect.top() - BOTTOM_GAP
    } else {
        video.bottom() - BOTTOM_GAP
    };

    let anchor = pos2(video.right() - RIGHT_MARGIN, anchor_bottom);
    let area_id = Id::new("player-skip-banner");

    let hovered = ctx
        .read_response(area_id)
        .is_some_and(|r| r.contains_pointer());

    let opacity = if hovered { 1.0 } else { IDLE_OPACITY };

    let mut skip_clicked = false;
    let mut cancel_clicked = false;
    let btn_size = Vec2::new(BTN_MIN_W, BTN_H);

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

                    cancel_clicked = button::label(
                        ui,
                        theme,
                        t!("player.cancel").as_ref(),
                        Opts::secondary(btn_size),
                    );

                    let frac = active.countdown_frac(now);
                    let is_counting = active.countdown_started_at.is_some();
                    skip_clicked = skip_btn(ui, theme, active, btn_size, frac, is_counting);
                });
            });
        });

    if active.countdown_started_at.is_some() {
        ctx.request_repaint();
    }

    SkipBannerOut {
        skip_clicked,
        cancel_clicked,
    }
}

fn skip_btn(
    ui: &mut egui::Ui,
    theme: &Theme,
    active: &ActiveSegment,
    btn_size: Vec2,
    frac: f32,
    is_counting: bool,
) -> bool {
    let caption: std::borrow::Cow<str> = match active.ty {
        SegmentType::Intro => t!("player.skip_intro"),
        SegmentType::Recap => t!("player.skip_recap"),
        SegmentType::Credits => t!("player.skip_credits"),
        SegmentType::Preview => t!("player.skip_preview"),
    };

    let text = RichText::new(caption.as_ref())
        .size(theme.text_body)
        .color(theme.btn_primary_fg);

    let response = button::add_named(
        ui,
        theme,
        text,
        Opts::primary(btn_size),
        Some(caption.as_ref()),
    );

    // Countdown fill: semi-transparent overlay drawn left-to-right over the button.
    if frac > 0.0 {
        let rect = response.rect;
        let mut fill_rect = rect;
        fill_rect.max.x = rect.left() + rect.width() * frac;

        let radius = theme.rounding(theme.radius_card);
        ui.painter().rect_filled(fill_rect, radius, COUNTDOWN_TINT);
    }

    if !is_counting {
        response.request_focus();
    }

    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
    response.clicked() || enter
}
