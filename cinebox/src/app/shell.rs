//! The window around the screens: title bar, side menu, backdrop and margins.

use cinebox_core::tmdb_image_url;
use cinebox_player::VideoOutput;
use egui::{CentralPanel, Color32, Context, Frame, Margin, Rect, Ui, pos2};

use super::App;
use super::nav::{NavAction, Screen};
use crate::platform;
use crate::services::Services;
use crate::services::images::ImageSlot;
use crate::theme::Theme;
use crate::widgets::{backdrop, chrome, focus, rail};

/// Navigation the window asked for this frame.
pub(super) struct WindowActions {
    /// From the title bar, the side menu or the settings panel.
    pub chrome: Option<NavAction>,
    pub screen: Option<NavAction>,
}

impl App {
    pub(super) fn show_window(&mut self, ui: &mut Ui, theme: &Theme) -> WindowActions {
        let screen = self.nav.current();
        let on_player = matches!(screen, Screen::Player { .. });
        let profile = platform::profile(ui.ctx());
        let player_fullscreen = on_player && self.player.fills_screen(profile);
        let outline = if profile.is_desktop_window() {
            1.0
        } else {
            0.0
        };

        let mut actions = WindowActions {
            chrome: None,
            screen: None,
        };

        let fill = self.page_fill(on_player);
        CentralPanel::default()
            .frame(Frame::new().fill(fill))
            .show(ui, |ui| {
                self.paint_backdrop(ui);
                let with_rail = screen.shows_rail() && !player_fullscreen;

                // Inside the window outline, starting at the title bar's bottom edge.
                let window = ui.max_rect();
                let bar_h = chrome::bar_height(ui.ctx(), theme);
                let rail_body = Rect::from_min_max(
                    pos2(window.left() + outline, window.top() + bar_h),
                    pos2(window.right(), window.bottom() - outline),
                );

                let rail_column = rail::column_center(ui.ctx(), rail_body.left());
                let back_x = with_rail.then_some(rail_column);

                let header = if player_fullscreen {
                    None
                } else {
                    chrome::header(
                        ui,
                        screen,
                        theme,
                        self.settings_screen.is_open(),
                        &mut self.search_bar,
                        back_x,
                    )
                };

                let rail = if with_rail {
                    rail::show(ui, rail_body, theme, screen.rail_entry())
                } else {
                    None
                };

                let margin = content_margin(ui.ctx(), screen, theme, with_rail);
                let content = Frame::new()
                    .inner_margin(margin)
                    .show(ui, |ui| screen_ui(self, ui, screen, theme));

                focus::set_content(ui.ctx(), content.response.rect - margin);
                if !player_fullscreen && profile.is_desktop_window() {
                    chrome::resize_edges(ui, theme);
                    chrome::window_outline(ui, theme);
                }

                let settings = self.settings_screen.ui(ui, &mut self.services, theme);
                let rail = rail.map(NavAction::OpenRail);

                actions = WindowActions {
                    chrome: settings.or(rail).or(header),
                    screen: content.inner,
                };
            });

        actions
    }

    /// Over a video layer the player paints its own background around the picture.
    fn page_fill(&self, on_player: bool) -> Color32 {
        if !on_player {
            return self.theme.page_bg;
        }

        if video_underlay(&self.services) {
            return Color32::TRANSPARENT;
        }

        self.theme.video_bg
    }

    fn paint_backdrop(&mut self, ui: &mut Ui) {
        let url = match self.nav.current() {
            Screen::Media { .. } | Screen::Torrents { .. } => self
                .media
                .ready()
                .and_then(|d| tmdb_image_url(d.backdrop_path.as_deref(), "w1280")),
            _ => None,
        };

        let Some(url) = url else {
            return;
        };

        if let ImageSlot::Ready(tex) = self.services.images.backdrop(Some(&url)) {
            backdrop::paint(ui, tex, &self.theme);
        }
    }
}

fn content_margin(ctx: &Context, screen: Screen, theme: &Theme, with_rail: bool) -> Margin {
    let pad = theme.pad.round() as i8;
    let edge = platform::edge_inset(ctx);
    let left = if with_rail {
        (rail::collapsed_width(ctx) + theme.pad).round() as i8
    } else {
        pad + edge.left
    };

    let right = pad + edge.right;

    match screen {
        Screen::Player { .. } => Margin::ZERO,
        Screen::Home => Margin {
            left,
            right,
            top: 0,
            bottom: pad + edge.bottom,
        },
        _ => Margin {
            left,
            right,
            top: 0,
            bottom: edge.bottom,
        },
    }
}

fn screen_ui(app: &mut App, ui: &mut Ui, screen: Screen, theme: &Theme) -> Option<NavAction> {
    // Under the player the page stays as it was, open file list included.
    let keeps_torrents = matches!(screen, Screen::Torrents { .. } | Screen::Player { .. });
    if !keeps_torrents {
        app.torrents.hide();
    }

    match screen {
        Screen::Home => app.home.ui(ui, &mut app.services, theme),
        Screen::Section { section } => app.section.ui(ui, &mut app.services, theme, section),
        Screen::Category { id } => app.category.ui(ui, &mut app.services, theme, id),
        Screen::Discover { section } => app.discover.ui(ui, &mut app.services, theme, section),
        Screen::Library => app.library.ui(ui, &app.services, theme),
        Screen::Search => app.search.ui(ui, &mut app.services, theme),
        Screen::Media { kind, id } => app.media.ui(ui, &mut app.services, theme, kind, id),
        Screen::Person { id } => app.person.ui(ui, &mut app.services, theme, id),
        Screen::Torrents { kind, id } => {
            let nav = app.torrents.ui(ui, &mut app.services, theme, kind, id);
            if app.torrents.intro_animating(ui.input(|i| i.time)) {
                ui.ctx().request_repaint();
            }
            nav
        }
        Screen::Player { .. } => app.player.ui(ui, &mut app.services, theme),
    }
}

fn video_underlay(svc: &Services) -> bool {
    let output = svc.player.as_ref().map(|player| player.output());
    output == Some(VideoOutput::Underlay)
}
