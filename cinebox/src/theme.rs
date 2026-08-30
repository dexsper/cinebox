//! Brand colors and sizes. The only place `Color32` literals live.

use egui::{Color32, CornerRadius, Margin, Stroke, Style, Visuals, style::ScrollStyle};

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
}

impl Theme {
    #[must_use]
    pub fn dark() -> Self {
        Self {
            page_bg: Color32::from_rgb(0x2B, 0x2D, 0x31),
            panel: Color32::from_rgb(20, 20, 26),
            panel_elevated: Color32::from_rgba_unmultiplied(28, 28, 36, 250),
            overlay: Color32::from_rgba_unmultiplied(0, 0, 0, 140),
            muted: Color32::from_rgb(166, 166, 173),
            muted_bright: Color32::from_rgb(199, 199, 209),
            label: Color32::from_rgb(235, 235, 240),
            title: Color32::from_rgb(245, 245, 247),
            body: Color32::from_rgb(224, 224, 230),
            err: Color32::from_rgb(235, 97, 97),
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
            toast_err: Color32::from_rgb(235, 97, 97),
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
        }
    }

    #[must_use]
    pub fn ring_pad(&self) -> f32 {
        self.ring_w + self.ring_gap
    }

    #[must_use]
    pub fn catalog_shelf_height(&self) -> f32 {
        self.tile_h + self.ring_pad() * 2.0 + 72.0
    }

    #[must_use]
    pub fn rounding(&self, radius: f32) -> CornerRadius {
        CornerRadius::same(radius.round() as u8)
    }

    /// Apply visuals once at startup.
    pub fn apply(&self, ctx: &egui::Context) {
        let mut visuals = Visuals::dark();
        
        visuals.override_text_color = Some(self.label);
        visuals.panel_fill = self.page_bg;
        visuals.window_fill = self.panel_elevated;
        visuals.extreme_bg_color = self.panel;
        visuals.faint_bg_color = self.panel;
        visuals.widgets.inactive.bg_fill = self.panel;
        visuals.widgets.hovered.bg_fill = self.widget_hover;
        visuals.widgets.active.bg_fill = self.widget_active;
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

        ctx.set_style_of(egui::Theme::Dark, style);
        ctx.set_theme(egui::ThemePreference::Dark);
    }
}
