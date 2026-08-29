//! Overlay scrollbars, inertial panning, click-drag, and Shift+wheel.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer};
use iced::border;
use iced::widget::container;
use iced::widget::scrollable::{self, Scrollbar, Status};
use iced::{
    Background, Color, Element, Event, Fill, Length, Point, Rectangle, Size, Theme, Vector,
    keyboard,
};

use crate::app::Message;

const FLASH: Duration = Duration::from_millis(700);
const DRAG_THRESHOLD: f32 = 8.0;
const FRICTION: f32 = 4.2;
const MIN_SPEED: f32 = 22.0;
const WHEEL_GAIN: f32 = 10.0;
const PIXEL_GAIN: f32 = 6.0;
const MAX_SPEED: f32 = 4200.0;

/// Identifies a scrollable that can flash its overlay scrollbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScrollPane {
    Page,
    Row(u8),
}

impl ScrollPane {
    fn widget_id(self) -> iced::widget::Id {
        match self {
            Self::Page => iced::widget::Id::new("cinebox-scroll-page"),
            Self::Row(index) => iced::widget::Id::from(format!("cinebox-scroll-row-{index}")),
        }
    }
}

/// Overlay visibility, click-vs-drag, and coasting velocity.
#[derive(Debug, Default)]
pub struct ScrollFlash {
    until: HashMap<ScrollPane, Instant>,
    velocity: HashMap<ScrollPane, Vector>,
    last_tick: Option<Instant>,
    pub suppress_click: bool,
}

impl ScrollFlash {
    pub fn is_active(&self, pane: ScrollPane) -> bool {
        self.moving(pane)
            || self
                .until
                .get(&pane)
                .is_some_and(|until| Instant::now() < *until)
    }

    pub fn page(&self) -> bool {
        self.is_active(ScrollPane::Page)
    }

    pub fn row(&self, index: u8) -> bool {
        self.is_active(ScrollPane::Row(index))
    }

    pub fn touch(&mut self, pane: ScrollPane) {
        self.until.insert(pane, Instant::now() + FLASH);
    }

    pub fn prune(&mut self) {
        let now = Instant::now();
        self.until.retain(|_, until| *until > now);
    }

    pub fn needs_tick(&self) -> bool {
        !self.until.is_empty() || self.velocity.values().any(|vel| speed(vel) >= MIN_SPEED)
    }

    pub fn reset(&mut self) {
        self.until.clear();
        self.velocity.clear();
        self.last_tick = None;
        self.suppress_click = false;
    }

    pub fn stop(&mut self, pane: ScrollPane) {
        self.velocity.remove(&pane);
    }

    pub fn impulse(&mut self, pane: ScrollPane, dx: f32, dy: f32, gain: f32) {
        let vel = self.velocity.entry(pane).or_insert(Vector::ZERO);
        vel.x = (vel.x + dx * gain).clamp(-MAX_SPEED, MAX_SPEED);
        vel.y = (vel.y + dy * gain).clamp(-MAX_SPEED, MAX_SPEED);
        self.touch(pane);
    }

    pub fn flick(&mut self, pane: ScrollPane, vx: f32, vy: f32) {
        self.velocity.insert(
            pane,
            Vector::new(
                vx.clamp(-MAX_SPEED, MAX_SPEED),
                vy.clamp(-MAX_SPEED, MAX_SPEED),
            ),
        );
        self.touch(pane);
    }

    pub fn step(&mut self, now: Instant) -> Vec<(ScrollPane, f32, f32)> {
        let dt = self
            .last_tick
            .map(|tick| now.saturating_duration_since(tick).as_secs_f32())
            .unwrap_or(1.0 / 60.0)
            .clamp(0.001, 0.05);
        self.last_tick = Some(now);
        let decay = (-FRICTION * dt).exp();
        let mut moved = Vec::new();
        self.velocity.retain(|pane, vel| {
            let dx = vel.x * dt;
            let dy = vel.y * dt;
            if dx.abs() > 0.15 || dy.abs() > 0.15 {
                moved.push((*pane, dx, dy));
            }
            vel.x *= decay;
            vel.y *= decay;
            if vel.x.abs() < MIN_SPEED {
                vel.x = 0.0;
            }
            if vel.y.abs() < MIN_SPEED {
                vel.y = 0.0;
            }
            speed(vel) >= MIN_SPEED
        });
        self.prune();
        moved
    }

    fn moving(&self, pane: ScrollPane) -> bool {
        self.velocity
            .get(&pane)
            .is_some_and(|vel| speed(vel) >= MIN_SPEED)
    }
}

fn speed(vel: &Vector) -> f32 {
    vel.x.abs().max(vel.y.abs())
}

pub fn scroll_by(pane: ScrollPane, dx: f32, dy: f32) -> iced::Task<Message> {
    iced::widget::operation::scroll_by(
        pane.widget_id(),
        scrollable::AbsoluteOffset { x: dx, y: dy },
    )
}

/// Vertical page scroller with an overlay bar and wheel inertia.
pub fn vertical<'a>(
    flashing: bool,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    let inner = Element::new(Smooth {
        pane: ScrollPane::Page,
        drag: false,
        content: content.into(),
    });
    iced::widget::scrollable(inner)
        .id(ScrollPane::Page.widget_id())
        .direction(scrollable::Direction::Vertical(overlay_bar(flashing)))
        .style(overlay_style(flashing))
        .width(Fill)
        .height(Fill)
        .into()
}

/// Horizontal shelf: overlay bar, click-drag, Shift+wheel, inertia.
pub fn horizontal<'a>(
    pane: ScrollPane,
    flashing: bool,
    height: impl Into<Length>,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    let scroller = iced::widget::scrollable(content)
        .id(pane.widget_id())
        .direction(scrollable::Direction::Horizontal(overlay_bar(flashing)))
        .style(overlay_style(flashing))
        .width(Fill)
        .height(height);
    Element::new(Smooth {
        pane,
        drag: true,
        content: scroller.into(),
    })
}

fn overlay_bar(flashing: bool) -> Scrollbar {
    if flashing {
        Scrollbar::new().width(7).scroller_width(5).margin(3)
    } else {
        Scrollbar::hidden()
    }
}

fn overlay_style(scrolling: bool) -> impl Fn(&Theme, Status) -> scrollable::Style {
    move |theme, status| {
        let mut style = scrollable::default(theme, status);
        style.container = container::Style::default();
        style.gap = None;
        let (h_show, h_hot, v_show, v_hot) = match status {
            Status::Dragged {
                is_horizontal_scrollbar_dragged,
                is_vertical_scrollbar_dragged,
                ..
            } => (
                scrolling || is_horizontal_scrollbar_dragged,
                is_horizontal_scrollbar_dragged,
                scrolling || is_vertical_scrollbar_dragged,
                is_vertical_scrollbar_dragged,
            ),
            Status::Hovered {
                is_horizontal_scrollbar_hovered,
                is_vertical_scrollbar_hovered,
                ..
            } => (
                scrolling || is_horizontal_scrollbar_hovered,
                is_horizontal_scrollbar_hovered,
                scrolling || is_vertical_scrollbar_hovered,
                is_vertical_scrollbar_hovered,
            ),
            Status::Active { .. } => (scrolling, false, scrolling, false),
        };
        style.horizontal_rail = overlay_rail(h_show, h_hot, 0.30);
        style.vertical_rail = overlay_rail(v_show, v_hot, 0.22);
        style
    }
}

fn overlay_rail(show: bool, hot: bool, base: f32) -> scrollable::Rail {
    let alpha = if !show {
        0.0
    } else if hot {
        (base + 0.14).min(0.5)
    } else {
        base
    };
    let fill = if alpha <= 0.001 {
        Background::Color(Color::TRANSPARENT)
    } else {
        Background::Color(Color::from_rgba(1.0, 1.0, 1.0, alpha))
    };
    scrollable::Rail {
        background: None,
        border: border::rounded(0),
        scroller: scrollable::Scroller {
            background: fill,
            border: border::rounded(8),
        },
    }
}

fn wheel_dx(delta: mouse::ScrollDelta, shift: bool) -> f32 {
    match delta {
        mouse::ScrollDelta::Lines { x, y } => {
            let x = if shift { y } else { x };
            -x * 60.0
        }
        mouse::ScrollDelta::Pixels { x, y } => {
            if shift && x.abs() <= y.abs() {
                -y
            } else {
                -x
            }
        }
    }
}

fn wheel_dy(delta: mouse::ScrollDelta, shift: bool) -> f32 {
    if shift {
        return 0.0;
    }
    match delta {
        mouse::ScrollDelta::Lines { y, .. } => -y * 60.0,
        mouse::ScrollDelta::Pixels { y, .. } => -y,
    }
}

fn wheel_gain(delta: mouse::ScrollDelta) -> f32 {
    match delta {
        mouse::ScrollDelta::Lines { .. } => WHEEL_GAIN,
        mouse::ScrollDelta::Pixels { .. } => PIXEL_GAIN,
    }
}

struct Smooth<'a> {
    pane: ScrollPane,
    drag: bool,
    content: Element<'a, Message>,
}

struct PanState {
    last: Option<Point>,
    last_at: Option<Instant>,
    vx: f32,
    dragging: bool,
    modifiers: keyboard::Modifiers,
}

impl Default for PanState {
    fn default() -> Self {
        Self {
            last: None,
            last_at: None,
            vx: 0.0,
            dragging: false,
            modifiers: keyboard::Modifiers::default(),
        }
    }
}

impl Widget<Message, Theme, iced::Renderer> for Smooth<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<PanState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(PanState::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        if let Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) = event {
            tree.state.downcast_mut::<PanState>().modifiers = *modifiers;
        }

        let over = cursor.is_over(layout.bounds());
        let position = cursor.position();
        let shift = tree.state.downcast_mut::<PanState>().modifiers.shift();

        if self.drag
            && over
            && matches!(
                event,
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                    | Event::Touch(iced::touch::Event::FingerPressed { .. })
            )
        {
            let state = tree.state.downcast_mut::<PanState>();
            state.last = position;
            state.last_at = Some(Instant::now());
            state.vx = 0.0;
            state.dragging = false;
        }

        if self.drag
            && over
            && let Event::Mouse(mouse::Event::WheelScrolled { delta }) = event
        {
            let dx = wheel_dx(*delta, shift);
            if dx.abs() > 0.5 {
                shell.publish(Message::ScrollImpulse {
                    pane: self.pane,
                    dx,
                    dy: 0.0,
                    gain: wheel_gain(*delta),
                });
                shell.capture_event();
                return;
            }
        }

        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );

        let state = tree.state.downcast_mut::<PanState>();

        if self.drag {
            match event {
                Event::Mouse(mouse::Event::CursorMoved { position })
                | Event::Touch(iced::touch::Event::FingerMoved { position, .. })
                    if state.last.is_some() =>
                {
                    let Some(last) = state.last else {
                        return;
                    };
                    let position = *position;
                    if !state.dragging && last.distance(position) >= DRAG_THRESHOLD {
                        state.dragging = true;
                        shell.publish(Message::ScrollDragging(true));
                    }
                    if state.dragging {
                        let dx = last.x - position.x;
                        let now = Instant::now();
                        if let Some(prev) = state.last_at {
                            let dt = now.saturating_duration_since(prev).as_secs_f32().max(0.001);
                            state.vx = dx / dt;
                        }
                        state.last_at = Some(now);
                        if dx.abs() > 0.2 {
                            shell.publish(Message::ScrollPan {
                                pane: self.pane,
                                dx,
                            });
                            shell.capture_event();
                        }
                    }
                    state.last = Some(position);
                }
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                | Event::Touch(
                    iced::touch::Event::FingerLifted { .. } | iced::touch::Event::FingerLost { .. },
                ) => {
                    if state.dragging {
                        shell.publish(Message::ScrollFlick {
                            pane: self.pane,
                            vx: state.vx,
                            vy: 0.0,
                        });
                        shell.publish(Message::ScrollDragging(false));
                    }
                    state.last = None;
                    state.last_at = None;
                    state.vx = 0.0;
                    state.dragging = false;
                }
                _ => {}
            }
        } else if !shell.is_event_captured()
            && over
            && let Event::Mouse(mouse::Event::WheelScrolled { delta }) = event
        {
            let dy = wheel_dy(*delta, shift);
            if dy.abs() > 0.5 {
                shell.publish(Message::ScrollImpulse {
                    pane: self.pane,
                    dx: 0.0,
                    dy,
                    gain: wheel_gain(*delta),
                });
                shell.capture_event();
            }
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: iced::Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a> From<Smooth<'a>> for Element<'a, Message> {
    fn from(row: Smooth<'a>) -> Self {
        Self::new(row)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_converts_vertical_wheel_lines_to_horizontal() {
        let dx = wheel_dx(mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 }, true);
        assert!(dx < 0.0, "scroll up + shift should pan left, got {dx}");
        let none = wheel_dx(mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 }, false);
        assert_eq!(none, 0.0);
    }

    #[test]
    fn shift_converts_vertical_pixel_wheel_to_horizontal() {
        let dx = wheel_dx(mouse::ScrollDelta::Pixels { x: 0.0, y: 40.0 }, true);
        assert_eq!(dx, -40.0);
    }

    #[test]
    fn shift_disables_vertical_wheel_on_page() {
        assert_eq!(
            wheel_dy(mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 }, true),
            0.0
        );
        assert!(wheel_dy(mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 }, false) < 0.0);
    }

    #[test]
    fn inertia_decays_and_stops() {
        let mut flash = ScrollFlash::default();
        flash.flick(ScrollPane::Page, 0.0, 800.0);
        assert!(flash.page());
        let now = Instant::now();
        let _ = flash.step(now);
        let later = now + Duration::from_millis(16);
        let moved = flash.step(later);
        assert!(!moved.is_empty());
        let mut t = later;
        for _ in 0..180 {
            t += Duration::from_millis(16);
            let _ = flash.step(t);
        }
        assert!(!flash.moving(ScrollPane::Page));
    }
}
