//! The change map: a narrow strip at the right edge of the file diff.
//! It shows one tick per hunk, the viewport box and the current hunk; click or drag jumps.

mod geometry;

#[cfg(test)]
mod tests;

use std::ops::Range;
use std::sync::Arc;

use gpui_kit::{
    Bounds, EventEmitter, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, canvas,
};

pub use self::geometry::{HunkKind, MapHunk, Tick, row_at, ticks, viewport_box};
use crate::prelude::*;
use crate::tokens::{self, DiffColors, Theme};

/// Width of the strip, in px.
const WIDTH: f32 = 22.;
/// Font size of the number on the current hunk, in px.
const NUMBER_SIZE: f32 = 10.;
/// What the current hunk's outline leaves of a wide strip, in px.
const CURRENT_INSET: f32 = 8.;
/// The narrowest drawn outline of the current hunk, in px.
const MARK_MIN: f32 = 4.;
/// A strip narrower than this has no room for the current hunk's number, in px.
const NUMBER_MIN: f32 = 16.;

/// What the map reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapEvent {
    /// The user asked to show this row.
    Jump(u32),
}

/// The change map element.
pub struct ChangeMap {
    hunks: Arc<[MapHunk]>,
    /// The hunks placed along `height`: kept until the hunks or the height change, so a scroll
    /// moves the box without re-placing a million ticks.
    marks: Arc<[Tick]>,
    total_rows: u32,
    viewport: Range<u32>,
    current: Option<usize>,
    top: Pixels,
    height: f32,
    width: Pixels,
    dragging: bool,
}

impl EventEmitter<MapEvent> for ChangeMap {}

impl ChangeMap {
    /// Builds a map for `hunks` over `total_rows` rows; the viewport starts empty.
    pub fn new(hunks: Arc<[MapHunk]>, total_rows: u32) -> Self {
        Self {
            marks: place(&hunks, total_rows, 0.),
            hunks,
            total_rows,
            viewport: 0..0,
            current: None,
            top: px(0.),
            height: 0.,
            width: px(WIDTH),
            dragging: false,
        }
    }

    /// Makes the strip `width` px wide; the file diff keeps the 22 px default.
    #[must_use]
    pub fn with_width(mut self, width: Pixels) -> Self {
        self.width = width;
        self
    }

    /// Replaces the hunks and the row count.
    pub fn set_hunks(&mut self, hunks: Arc<[MapHunk]>, total_rows: u32, cx: &mut Context<Self>) {
        self.hunks = hunks;
        self.total_rows = total_rows;
        self.current = self.current.filter(|at| *at < self.hunks.len());
        self.marks = place(&self.hunks, self.total_rows, self.height);
        cx.notify();
    }

    /// Sets the rows the file diff shows now.
    pub fn set_viewport(&mut self, rows: Range<u32>, cx: &mut Context<Self>) {
        if self.viewport != rows {
            self.viewport = rows;
            cx.notify();
        }
    }

    /// Sets the current hunk (outlined and numbered), or none.
    pub fn set_current(&mut self, hunk: Option<usize>, cx: &mut Context<Self>) {
        let hunk = hunk.filter(|at| *at < self.hunks.len());
        if self.current != hunk {
            self.current = hunk;
            cx.notify();
        }
    }

    /// The current hunk, or none.
    #[cfg(test)]
    pub(crate) fn current(&self) -> Option<usize> {
        self.current
    }

    /// The rows on screen, for tests.
    #[cfg(test)]
    pub(crate) fn viewport(&self) -> Range<u32> {
        self.viewport.clone()
    }

    /// The placed ticks, for tests.
    #[cfg(test)]
    pub(crate) fn ticks(&self) -> Arc<[Tick]> {
        Arc::clone(&self.marks)
    }

    /// Emits a jump to the row under window position `y`.
    fn jump(&self, y: Pixels, cx: &mut Context<Self>) {
        let row = row_at(f32::from(y - self.top), self.total_rows, self.height);
        cx.emit(MapEvent::Jump(row));
    }

    /// Stores the measured strip height.
    fn measure(&mut self, bounds: Bounds<Pixels>, cx: &mut Context<Self>) {
        let height = f32::from(bounds.size.height);
        self.top = bounds.origin.y;
        if (self.height - height).abs() > f32::EPSILON {
            self.height = height;
            self.marks = place(&self.hunks, self.total_rows, height);
            cx.notify();
        }
    }
}

/// Places every hunk along a strip `height` px high.
fn place(hunks: &[MapHunk], total_rows: u32, height: f32) -> Arc<[Tick]> {
    Arc::from(ticks(hunks, total_rows, height))
}

/// Returns the tick colour for a hunk kind from the diff tokens.
fn tint(colors: &DiffColors, kind: HunkKind) -> Hsla {
    match kind {
        HunkKind::Changed => colors.change.mark,
        HunkKind::Added => colors.add.mark,
        HunkKind::Deleted => colors.delete.mark,
    }
}

/// An invisible layer that reports the strip's bounds.
fn sensor(cx: &mut Context<ChangeMap>) -> impl IntoElement + use<> {
    let entity = cx.entity().downgrade();
    canvas(
        move |bounds, _, cx| {
            entity.update(cx, |map, cx| map.measure(bounds, cx)).ok();
        },
        |_, (), _, _| {},
    )
    .absolute()
    .size_full()
}

/// One tick; the current one gets an outline, and its number on a strip wide enough for it.
fn tick_view(t: &Theme, tick: &Tick, number: Option<usize>, width: f32) -> Div {
    let base = div()
        .absolute()
        .left_0()
        .w(px(3.))
        .top(px(tick.top))
        .h(px(tick.height));
    let mark = base.bg(tint(&t.diff, tick.kind)).rounded(px(1.));
    let wide = width >= NUMBER_MIN;
    match number {
        None => mark,
        Some(n) => mark
            .w(px((width - CURRENT_INSET).max(MARK_MIN)))
            .border_2()
            .border_color(t.colors.text)
            .when(wide, |mark| {
                mark.child(
                    div()
                        .absolute()
                        .right_0()
                        .text_size(px(NUMBER_SIZE))
                        .text_color(t.colors.background)
                        .bg(t.colors.text)
                        .rounded(t.radii.xs)
                        .px(px(2.))
                        .child(format!("{}", n + 1)),
                )
            }),
    }
}

impl Render for ChangeMap {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let sensor = sensor(cx);
        let t = tokens::current(cx);
        let marks = Arc::clone(&self.marks);
        let current = self.current.and_then(|at| geometry::tick_of(&marks, at));
        let (vp_top, vp_height) = viewport_box(&self.viewport, self.total_rows, self.height);
        let width = f32::from(self.width);
        let ticks_view = marks.iter().enumerate().map(|(i, tick)| {
            let number = (current == Some(i)).then(|| self.current.unwrap_or(0));
            tick_view(t, tick, number, width)
        });
        div()
            .id("change-map")
            .relative()
            .flex_none()
            .w(self.width)
            .h_full()
            .bg(t.colors.rail)
            .border_l_1()
            .border_color(t.colors.line)
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|map, event: &MouseDownEvent, _, cx| {
                    map.dragging = true;
                    map.jump(event.position.y, cx);
                }),
            )
            .on_mouse_move(cx.listener(|map, event: &MouseMoveEvent, _, cx| {
                if map.dragging && event.dragging() {
                    map.jump(event.position.y, cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|map, _: &MouseUpEvent, _, _| map.dragging = false),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|map, _: &MouseUpEvent, _, _| map.dragging = false),
            )
            .child(sensor)
            .children(ticks_view)
            .child(viewport_view(t, vp_top, vp_height))
    }
}

/// The box that shows which rows are on screen, `top` and `height` in pixels.
fn viewport_view(t: &Theme, top: f32, height: f32) -> Div {
    div()
        .absolute()
        .left_0()
        .right_0()
        .top(px(top))
        .h(px(height))
        .border_1()
        .border_color(t.colors.muted)
        .bg(t.colors.text.opacity(0.08))
        .rounded(px(2.))
}
