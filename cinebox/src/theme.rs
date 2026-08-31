//! Brand colors and sizes. The only place `Color32` literals live.

use egui::{
    Color32, CornerRadius, CursorIcon, FontId, Margin, Stroke, Style, Visuals, style::ScrollStyle,
};

pub const PAGE_BG_RGB: [u8; 3] = [0x2B, 0x2D, 0x31];

/// Application theme. Screens and widgets take `&Theme`; they do not pick colors.
#[derive(Clone, Debug)]
pub struct Theme {
    pub page_bg: Color32,
    pub panel: Color32,
    pub panel_elevated: Color32,
    pub overlay: Color32,
    pub muted: Color32,
    pub muted_bright: Color32,
    pub label: Color32,
    pub title: Color32,
    pub body: Color32,
    pub err: Color32,
    pub ok: Color32,
    pub rate: Color32,
    pub ring: Color32,
    pub poster_placeholder: Color32,
    pub card: Color32,
    pub card_selected: Color32,
    pub badge_bg: Color32,
    pub size_pill_bg: Color32,
    pub size_pill_fg: Color32,
    pub progress_fill: Color32,
    pub progress_track: Color32,
    pub toast_info: Color32,
    pub toast_ok: Color32,
    pub toast_err: Color32,
    pub video_bg: Color32,
    pub metric_bg: Color32,
    pub rating_pill: Color32,
    pub chrome_bg: Color32,
    pub chrome_btn_idle: Color32,
    pub chrome_btn_hover: Color32,
    pub chrome_close_hover: Color32,
    pub window_edge: Color32,
    pub btn_primary_bg: Color32,
    pub btn_primary_fg: Color32,
    pub btn_primary_hover: Color32,
    pub widget_hover: Color32,
    pub widget_active: Color32,
    pub selection: Color32,
    pub input_bg: Color32,
    pub toggle_off: Color32,
    pub radius_poster: f32,
    pub radius_card: f32,
    pub radius_badge: f32,
    pub radius_dialog: f32,
    pub tile_w: f32,
    pub tile_h: f32,
    pub ring_w: f32,
    pub ring_gap: f32,
    pub poster_w: f32,
    pub poster_h: f32,
    pub explorer_poster_w: f32,
    pub explorer_poster_h: f32,
    pub explorer_left: f32,
    pub still_w: f32,
    pub still_h: f32,
    pub pad: f32,
    pub title_bar_h: f32,
    pub overview_max_w: f32,
    pub text_micro: f32,
    pub text_caption: f32,
    pub text_small: f32,
    pub text_body: f32,
    pub text_label: f32,
    pub text_section: f32,
    pub text_subtitle: f32,
    pub text_heading: f32,
    pub text_display: f32,
    pub text_hero: f32,
    pub text_person: f32,
    pub text_explorer_from: f32,
    pub text_icon: f32,
    pub text_icon_md: f32,
    pub text_icon_lg: f32,
    pub text_cta_icon: f32,
    pub text_gauge_min: f32,
    pub text_gauge_max: f32,
}

impl Theme {
    #[must_use]
    pub fn dark() -> Self {
        Self {
            page_bg: Color32::from_rgb(PAGE_BG_RGB[0], PAGE_BG_RGB[1], PAGE_BG_RGB[2]),
            panel: Color32::from_rgb(20, 20, 26),
            panel_elevated: Color32::from_rgba_unmultiplied(28, 28, 36, 250),
            overlay: Color32::from_rgba_unmultiplied(0, 0, 0, 140),
            muted: Color32::from_rgb(166, 166, 173),
            muted_bright: Color32::from_rgb(199, 199, 209),
            label: Color32::from_rgb(235, 235, 240),
            title: Color32::from_rgb(245, 245, 247),
            body: Color32::from_rgb(224, 224, 230),
            err: Color32::from_rgb(255, 56, 56),
            ok: Color32::from_rgb(115, 209, 140),
            rate: Color32::from_rgb(255, 217, 64),
            ring: Color32::from_rgb(240, 240, 245),
            poster_placeholder: Color32::from_rgb(41, 41, 46),
            card: Color32::from_rgba_unmultiplied(20, 20, 26, 184),
            card_selected: Color32::from_rgb(54, 54, 54),
            badge_bg: Color32::from_rgba_unmultiplied(0, 0, 0, 191),
            size_pill_bg: Color32::from_rgb(38, 40, 41),
            size_pill_fg: Color32::WHITE,
            progress_fill: Color32::WHITE,
            progress_track: Color32::from_rgba_unmultiplied(255, 255, 255, 77),
            toast_info: Color32::from_rgb(90, 140, 220),
            toast_ok: Color32::from_rgb(115, 209, 140),
            toast_err: Color32::from_rgb(255, 56, 56),
            video_bg: Color32::BLACK,
            metric_bg: Color32::from_white_alpha(28),
            rating_pill: Color32::from_black_alpha(89),
            chrome_bg: Color32::from_rgb(22, 23, 27),
            chrome_btn_idle: Color32::TRANSPARENT,
            chrome_btn_hover: Color32::from_rgba_unmultiplied(255, 255, 255, 22),
            chrome_close_hover: Color32::from_rgb(196, 64, 64),
            window_edge: Color32::from_white_alpha(28),
            btn_primary_bg: Color32::from_rgb(245, 245, 247),
            btn_primary_fg: Color32::from_rgb(0x2B, 0x2D, 0x31),
            btn_primary_hover: Color32::from_rgb(220, 220, 228),
            widget_hover: Color32::from_rgb(40, 40, 50),
            widget_active: Color32::from_rgb(50, 50, 62),
            selection: Color32::from_rgb(70, 90, 140),
            input_bg: Color32::from_rgb(32, 33, 38),
            toggle_off: Color32::from_rgb(58, 59, 66),
            radius_poster: 12.0,
            radius_card: 8.0,
            radius_badge: 4.0,
            radius_dialog: 12.0,
            tile_w: 140.0,
            tile_h: 210.0,
            ring_w: 3.0,
            ring_gap: 4.0,
            poster_w: 200.0,
            poster_h: 300.0,
            explorer_poster_w: 112.0,
            explorer_poster_h: 168.0,
            explorer_left: 340.0,
            still_w: 168.0,
            still_h: 98.0,
            pad: 16.0,
            title_bar_h: 40.0,
            overview_max_w: 640.0,
            text_micro: 11.0,
            text_caption: 12.0,
            text_small: 13.0,
            text_body: 14.0,
            text_label: 15.0,
            text_section: 16.0,
            text_subtitle: 18.0,
            text_heading: 20.0,
            text_display: 22.0,
            text_hero: 36.0,
            text_person: 26.0,
            text_explorer_from: 32.0,
            text_icon: 16.0,
            text_icon_md: 18.0,
            text_icon_lg: 20.0,
            text_cta_icon: 22.0,
            text_gauge_min: 26.0,
            text_gauge_max: 38.0,
        }
    }

    #[must_use]
    pub fn ring_pad(&self) -> f32 {
        self.ring_w + self.ring_gap
    }

    #[must_use]
    pub fn rounding(&self, radius: f32) -> CornerRadius {
        CornerRadius::same(radius.round() as u8)
    }

    #[must_use]
    pub fn overlay_at(&self, t: f32) -> Color32 {
        self.overlay.gamma_multiply(t.clamp(0.0, 1.0))
    }

    /// Arc color for the speed gauge. `t` is 0..=1 along the scale.
    #[must_use]
    pub fn gauge_hot(&self, t: f32) -> Color32 {
        let t = t.clamp(0.0, 1.0);
        let hue = 330.0 + t * 200.0;
        hsl(hue, 0.80, 0.45)
    }

    #[must_use]
    pub fn gauge_track(&self) -> Color32 {
        Color32::from_white_alpha(28)
    }

    #[must_use]
    pub fn ui_font(&self, size: f32) -> FontId {
        FontId::proportional(size)
    }

    /// Movie titles, shelf labels, drawer headings.
    #[must_use]
    pub fn title_font(&self, size: f32) -> FontId {
        crate::fonts::title(size)
    }

    /// Primary action labels.
    #[must_use]
    pub fn emphasis_font(&self, size: f32) -> FontId {
        crate::fonts::emphasis(size)
    }

    /// Apply visuals once at startup.
    pub fn apply(&self, ctx: &egui::Context) {
        let mut visuals = Visuals::dark();

        visuals.override_text_color = Some(self.label);
        visuals.panel_fill = self.page_bg;
        visuals.window_fill = self.panel_elevated;
        visuals.faint_bg_color = self.panel_elevated;
        visuals.extreme_bg_color = self.input_bg;
        visuals.widgets.inactive.bg_fill = self.input_bg;
        visuals.widgets.inactive.weak_bg_fill = self.input_bg;
        visuals.widgets.hovered.bg_fill = self.widget_hover;
        visuals.widgets.hovered.weak_bg_fill = self.widget_hover;
        visuals.widgets.active.bg_fill = self.widget_active;
        visuals.widgets.active.weak_bg_fill = self.widget_active;
        visuals.widgets.open.bg_fill = self.input_bg;
        visuals.widgets.open.weak_bg_fill = self.input_bg;
        visuals.widgets.inactive.expansion = 0.0;
        visuals.widgets.hovered.expansion = 0.0;
        visuals.widgets.active.expansion = 0.0;
        visuals.widgets.open.expansion = 0.0;
        visuals.interact_cursor = Some(CursorIcon::PointingHand);
        visuals.selection.bg_fill = self.selection;
        visuals.window_corner_radius = self.rounding(self.radius_dialog);
        visuals.window_stroke = Stroke::NONE;
        visuals.window_shadow.blur = 16;

        let mut style = Style {
            visuals,
            ..Style::default()
        };

        style.spacing.scroll = ScrollStyle::floating();
        style.spacing.scroll.fade.strength = 0.0;
        style.spacing.window_margin = Margin::same(12);
        style.interaction.selectable_labels = false;

        ctx.set_style_of(egui::Theme::Dark, style);
        ctx.set_theme(egui::ThemePreference::Dark);
    }
}

fn hsl(h: f32, s: f32, l: f32) -> Color32 {
    let h = ((h % 360.0) + 360.0) % 360.0;
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h_sec = h / 60.0;
    let x = c * (1.0 - (h_sec % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = hsl_rgb(h, c, x);

    let to_u8 = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(to_u8(r), to_u8(g), to_u8(b))
}

fn hsl_rgb(h: f32, c: f32, x: f32) -> (f32, f32, f32) {
    if h < 60.0 {
        return (c, x, 0.0);
    }
    if h < 120.0 {
        return (x, c, 0.0);
    }
    if h < 180.0 {
        return (0.0, c, x);
    }
    if h < 240.0 {
        return (0.0, x, c);
    }
    if h < 300.0 {
        return (x, 0.0, c);
    }

    (c, 0.0, x)
}
