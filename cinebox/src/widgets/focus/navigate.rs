//! Moving focus with the D-pad.
//!
//! egui moves focus on the arrows itself, but it also lands on widgets under an
//! open popup: it remembers focusable widgets from before the popup opened.
//! This walks only the current frame's focus stops on layers that take input,
//! scoring them the way egui does.

use egui::{Context, FocusDirection, Id, Rangef, Rect, Vec2, vec2};

use super::{Candidate, Hold, State};

impl State {
    fn areas_of(&self, id: Id) -> &[Id] {
        let candidate = self.candidates.iter().find(|candidate| candidate.id == id);
        candidate.map_or(&[], |candidate| &candidate.areas)
    }

    fn arrival(&self, target: Id, from: Id) -> Id {
        let group = self.groups.iter().find(|group| group.members.contains(&target));
        let Some(group) = group else {
            return target;
        };

        if group.members.contains(&from) {
            return target;
        }

        group.entry
    }
}
use crate::platform::{self, Direction, UiSound};

struct ArrowPress {
    direction: Direction,
    repeat: bool,
}

impl Hold {
    fn keeps(&self, widget: Id, direction: Direction) -> bool {
        if self.id != widget {
            return false;
        }

        match direction {
            Direction::Left | Direction::Right => self.horizontal,
            Direction::Up | Direction::Down => self.vertical,
        }
    }
}

pub(super) fn step(ctx: &Context, state: &State) {
    if ctx.text_edit_focused() {
        return;
    }

    let Some(press) = ctx.input(|i| i.events.iter().find_map(arrow_press)) else {
        return;
    };

    let Some(focused) = ctx.memory(|mem| mem.focused()) else {
        return;
    };

    let held = state.holds.iter().any(|hold| hold.keeps(focused, press.direction));
    if held {
        return;
    }

    let Some(from) = ctx.read_response(focused) else {
        return;
    };

    // Cancels egui's own move for this press.
    ctx.memory_mut(|mem| mem.move_focus(FocusDirection::None));
    ctx.request_repaint();

    let toward = toward(press.direction);
    let areas = state.areas_of(focused);
    let candidates = &state.candidates;
    let Some(nearest) = nearest_staying_in(ctx, from.rect, toward, candidates, areas) else {
        return;
    };

    let target = state.arrival(nearest, focused);
    ctx.memory_mut(|mem| mem.request_focus(target));
    let sound = UiSound::Navigate {
        direction: press.direction,
        repeat: press.repeat,
    };
    platform::device(ctx).play_sound(sound);
}

fn arrow_press(event: &egui::Event) -> Option<ArrowPress> {
    let egui::Event::Key {
        key,
        pressed: true,
        repeat,
        ..
    } = event
    else {
        return None;
    };

    let direction = match key {
        egui::Key::ArrowUp => Direction::Up,
        egui::Key::ArrowDown => Direction::Down,
        egui::Key::ArrowLeft => Direction::Left,
        egui::Key::ArrowRight => Direction::Right,
        _ => return None,
    };

    Some(ArrowPress {
        direction,
        repeat: *repeat,
    })
}

fn toward(direction: Direction) -> Vec2 {
    match direction {
        Direction::Up => Vec2::UP,
        Direction::Down => Vec2::DOWN,
        Direction::Left => Vec2::LEFT,
        Direction::Right => Vec2::RIGHT,
    }
}

/// Inside a scroll area the D-pad stays in it while the area has somewhere
/// to go that way, even when a widget outside (a side menu, the search bar)
/// is nearer on screen: scrolling brings the next item into view. Only then
/// does it leave, one enclosing area at a time.
fn nearest_staying_in(
    ctx: &Context,
    from: Rect,
    toward: Vec2,
    candidates: &[Candidate],
    areas: &[Id],
) -> Option<Id> {
    for area in areas.iter().rev() {
        let inside = candidates.iter().filter(|candidate| candidate.areas.contains(area));
        if let Some(found) = nearest(ctx, from, toward, inside) {
            return Some(found);
        }
    }

    nearest(ctx, from, toward, candidates)
}

/// The candidate closest to `from` within 45° of `toward`. Overlapping spans
/// count as aligned, so the list item below beats a button off to the side.
fn nearest<'a>(
    ctx: &Context,
    from: Rect,
    toward: Vec2,
    candidates: impl IntoIterator<Item = &'a Candidate>,
) -> Option<Id> {
    let mut best: Option<(Id, f32)> = None;

    for candidate in candidates {
        let interactive = ctx.memory(|mem| mem.allows_interaction(candidate.layer));
        if !interactive {
            continue;
        }

        let dx = span_offset(candidate.rect.x_range(), from.x_range());
        let dy = span_offset(candidate.rect.y_range(), from.y_range());
        let offset = vec2(dx, dy);

        // Zero for anything overlapping `from`, the focused widget included.
        let alignment = offset.normalized().dot(toward);
        if alignment < std::f32::consts::FRAC_1_SQRT_2 {
            continue;
        }

        let score = offset.length() / (alignment * alignment);
        let better = best.is_none_or(|(_, best_score)| score < best_score);
        if better {
            best = Some((candidate.id, score));
        }
    }

    best.map(|(id, _)| id)
}

/// Zero when the spans overlap by at least half the shorter one, else the
/// distance between their centers (negative when `a` comes first).
fn span_offset(a: Rangef, b: Rangef) -> f32 {
    let overlap = a.intersection(b).span();
    let shorter = a.span().min(b.span());
    if overlap >= shorter * 0.5 {
        return 0.0;
    }

    a.center() - b.center()
}
