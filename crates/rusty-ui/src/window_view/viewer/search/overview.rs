use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::base::{Scrollbar, TestSupportExt};
use gpui_kit::{Bounds, EntityId, MouseButton, MouseDownEvent, Role, canvas};
use rusty_core::marks::{MARK_COLOURS, ticks};

use super::Marking;
use crate::prelude::*;
use crate::tokens::Theme;

pub(in crate::window_view::viewer) const HIT_TICK: u8 = MARK_COLOURS;
pub(in crate::window_view::viewer) const BOOKMARK_TICK: u8 = MARK_COLOURS + 1;
const NEAR_ROWS: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TickKey {
    rev: u64,
    len: u64,
    rows: u32,
}

#[derive(Debug, Default)]
pub(in crate::window_view::viewer) struct Ticks {
    key: Option<TickKey>,
    pub shown: Vec<(u32, u8)>,
    targets: Vec<(u32, u64)>,
}

#[derive(Clone, Copy)]
pub(in crate::window_view::viewer) struct TickScale<'a> {
    pub len: u64,
    pub rows: u32,
    pub place: &'a dyn Fn(u64) -> u64,
}

fn row_of(position: u64, scale: &TickScale<'_>) -> u32 {
    let row = u128::from(position) * u128::from(scale.rows) / u128::from(scale.len.max(1));
    u32::try_from(row)
        .unwrap_or(u32::MAX)
        .min(scale.rows.saturating_sub(1))
}

impl Marking {
    fn tick_spans(&self) -> Vec<(u64, u8)> {
        let hits = self
            .counted()
            .into_iter()
            .flat_map(|counted| counted.starts.iter().map(|start| (*start, HIT_TICK)));
        let bookmarks = self.bookmarks.iter().map(|at| (at, BOOKMARK_TICK));
        self.marks
            .spans()
            .into_iter()
            .chain(hits)
            .chain(bookmarks)
            .collect()
    }

    pub(in crate::window_view::viewer) fn update_ticks(&mut self, scale: &TickScale<'_>) {
        let key = TickKey {
            rev: self.rev,
            len: scale.len,
            rows: scale.rows,
        };
        if self.ticks.key == Some(key) {
            return;
        }
        let spans = self.tick_spans();
        let placed: Vec<(u64, u8)> = spans
            .iter()
            .map(|(offset, colour)| ((scale.place)(*offset), *colour))
            .collect();
        let mut targets: Vec<(u32, u64)> = spans
            .iter()
            .zip(&placed)
            .map(|((offset, _), (position, _))| (row_of(*position, scale), *offset))
            .collect();
        targets.sort_unstable();
        targets.dedup_by_key(|(row, _)| *row);
        self.ticks = Ticks {
            key: Some(key),
            shown: ticks(&placed, scale.len, scale.rows),
            targets,
        };
    }

    #[cfg(test)]
    pub(in crate::window_view::viewer) fn tick_rows(&self) -> u32 {
        self.ticks.key.map_or(0, |key| key.rows)
    }

    pub(in crate::window_view::viewer) fn tick_target(&self, row: u32) -> Option<u64> {
        self.ticks
            .targets
            .iter()
            .filter(|(at, _)| at.abs_diff(row) <= NEAR_ROWS)
            .min_by_key(|(at, _)| at.abs_diff(row))
            .map(|(_, offset)| *offset)
    }
}

pub(in crate::window_view::viewer) struct StripLook<'a> {
    pub t: &'a Theme,
    pub shown: &'a [(u32, u8)],
    pub entity: EntityId,
}

#[derive(Clone, Default)]
pub(in crate::window_view::viewer) struct Overview(Rc<Cell<Bounds<Pixels>>>);

fn tick_colour(t: &Theme, colour: u8) -> Hsla {
    match colour {
        HIT_TICK => t.colors.accent,
        BOOKMARK_TICK => t.colors.muted,
        mark => t.marks.ticks[usize::from(mark.min(MARK_COLOURS - 1))],
    }
}

impl Overview {
    pub(in crate::window_view::viewer) fn rows(&self, t: &Theme) -> u32 {
        let rows = (self.0.get().size.height / t.spacing.xxs).floor();
        if rows.is_finite() && rows >= 1. {
            rows as u32
        } else {
            0
        }
    }

    pub(in crate::window_view::viewer) fn row_at(&self, t: &Theme, y: Pixels) -> u32 {
        let bounds = self.0.get();
        let row = ((y - bounds.origin.y) / t.spacing.xxs).floor().max(0.);
        (row as u32).min(self.rows(t).saturating_sub(1))
    }

    pub(in crate::window_view::viewer) fn element(
        &self,
        look: &StripLook<'_>,
        on_click: impl Fn(Pixels, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let t = look.t;
        let shown = look.shown;
        let entity = look.entity;
        let measured = self.0.clone();
        let measure = canvas(
            move |bounds, window, _| {
                if measured.replace(bounds).size != bounds.size {
                    window.on_next_frame(move |_, cx| cx.notify(entity));
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        let tick = t.spacing.xxs;
        div()
            .id("overview")
            .role(Role::Group)
            .aria_label("Marks overview")
            .test_support()
            .absolute()
            .top_0()
            .bottom_0()
            .right(Scrollbar::width())
            .w(t.spacing.md)
            .on_mouse_down(
                MouseButton::Left,
                move |event: &MouseDownEvent, window, cx| {
                    on_click(event.position.y, window, cx);
                },
            )
            .child(measure)
            .children(shown.iter().map(|(row, colour)| {
                div()
                    .absolute()
                    .when(*colour == HIT_TICK, |lane| lane.right_0())
                    .when(*colour != HIT_TICK, |lane| lane.left_0())
                    .w(t.spacing.xs)
                    .top(tick * *row as f32)
                    .h(tick)
                    .bg(tick_colour(t, *colour))
            }))
            .into_any_element()
    }
}
