//! Sideways scroll of the file diff (D39.2–D39.4, PC3, PC4): one offset in px that both text
//! columns share, the wheel that moves it, and the column cut every row draws from it.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::base::ScrollbarHandle;
use gpui_kit::{ScrollDelta, ScrollEvent, Size, Stateful, WeakEntity, font};

use crate::compare_diff::body::{RowCut, TextProbe};
use crate::prelude::*;
use crate::tokens::Theme;
use crate::window_view::NavoWindow;
use crate::window_view::viewer::{ScrollLeft, ScrollRight};

/// The sideways state of one open text view.
#[derive(Default)]
struct Sideways {
    /// How far the text is scrolled right, in px; never below 0.
    offset: f32,
    /// The widest line of the pair in columns; `None` until it is measured.
    widest: Option<u32>,
    /// The width of one column of the monospace font, in px, read each frame.
    advance: f32,
    /// Whether lines wrap: sideways input is then ignored and the offset kept.
    wrap: bool,
    /// The text cells' bounds, written by the first built row's canvas.
    probe: TextProbe,
    /// The left edge of the strip under the list, in window px, written by its canvas.
    strip_x: Pixels,
}

impl Sideways {
    /// The text cell width in px, or `None` before the first probe landed.
    fn text_width(&self) -> Option<f32> {
        let seen = self.probe.get();
        let width = f32::from(seen.left.size.width).max(f32::from(seen.right.size.width));
        (width > 0.).then_some(width)
    }

    /// The largest offset in px, or `None` until both the widths and the text cell are known.
    fn max(&self) -> Option<f32> {
        let width = self.text_width()?;
        let widest = self.widest? as f32;
        Some(((widest + 1.) * self.advance - width).max(0.))
    }
}

/// A shared handle on the sideways state: the view, the wheel listener and the bar hold clones.
#[derive(Clone, Default)]
pub(super) struct SidewaysHandle(Rc<RefCell<Sideways>>);

impl SidewaysHandle {
    /// A state for a reloaded pair: the offset and wrap of `old`, kept unclamped until the new
    /// widths are measured.
    pub(super) fn carried(old: &Self) -> Self {
        let old = old.0.borrow();
        Self(Rc::new(RefCell::new(Sideways {
            offset: old.offset,
            advance: old.advance,
            wrap: old.wrap,
            ..Sideways::default()
        })))
    }

    /// Sets the widest line of the pair, in columns.
    pub(super) fn set_widest(&self, widest: u32) {
        self.0.borrow_mut().widest = Some(widest);
    }

    /// Turns wrap on or off; the offset stays as it is.
    pub(super) fn set_wrap(&self, wrap: bool) {
        self.0.borrow_mut().wrap = wrap;
    }

    /// The probe the first built row reports its text cells to.
    pub(super) fn probe(&self) -> TextProbe {
        self.0.borrow().probe.clone()
    }

    /// Takes this frame's column `advance` (px), clamps the offset once everything is known,
    /// and returns the cut every row draws (whole lines while wrapping or still measuring).
    pub(super) fn frame(&self, advance: Pixels) -> RowCut {
        let mut state = self.0.borrow_mut();
        state.advance = f32::from(advance);
        let (Some(max), Some(width), false) = (state.max(), state.text_width(), state.wrap) else {
            return RowCut::default();
        };
        state.offset = state.offset.min(max);
        let advance = state.advance;
        let from = (state.offset / advance).floor();
        let span = (width / advance).ceil() + 2.;
        RowCut {
            cols: Some(from as u32..(from + span) as u32),
            nudge: px(state.offset - from * advance),
            ..RowCut::default()
        }
    }

    /// Moves the offset by a wheel `delta`; true when it moved. A precise (touchpad) delta
    /// counts only when it is more sideways than vertical, so a vertical swipe never drifts.
    pub(super) fn wheel(&self, delta: &ScrollDelta) -> bool {
        let mut state = self.0.borrow_mut();
        let (Some(max), false) = (state.max(), state.wrap) else {
            return false;
        };
        let dx = match delta {
            ScrollDelta::Lines(d) => d.x * state.advance,
            ScrollDelta::Pixels(d) if d.x.abs() > d.y.abs() => f32::from(d.x),
            ScrollDelta::Pixels(_) => 0.,
        };
        let next = (state.offset - dx).clamp(0., max);
        let moved = next != state.offset;
        state.offset = next;
        moved
    }
}

impl ScrollbarHandle for SidewaysHandle {
    /// The probed left (or only) text cell.
    fn viewport_bounds(&self) -> Bounds<Pixels> {
        self.0.borrow().probe.get().left
    }

    fn offset(&self) -> Point<Pixels> {
        point(px(-self.0.borrow().offset), px(0.))
    }

    /// Moves to the thumb's offset, clamped; ignored until the extent is known.
    fn set_offset(&self, offset: Point<Pixels>) {
        let mut state = self.0.borrow_mut();
        if let Some(max) = state.max() {
            state.offset = (-f32::from(offset.x)).clamp(0., max);
        }
    }

    fn content_size(&self) -> Size<Pixels> {
        let state = self.0.borrow();
        let wide = state.widest.unwrap_or(0) as f32 * state.advance;
        size(px(wide), state.probe.get().left.size.height)
    }
}

/// The width of one monospace column at the body's text size, in px. A text system that
/// measures nothing (tests) gets 0.6 × the size.
pub(super) fn advance(window: &Window, t: &Theme) -> Pixels {
    let system = window.text_system();
    let id = system.resolve_font(&font(t.typography.mono_font.clone()));
    match system.ch_width(id, t.typography.small) {
        Ok(width) if width > px(0.) => width,
        _ => t.typography.small * 0.6,
    }
}

/// How many columns Left or Right moves the text, as in the viewer (F3).
const KEY_COLS: f32 = 8.;

impl SidewaysHandle {
    /// Whether the text is cut: the extent is known and above 0, and lines do not wrap.
    pub(super) fn cut_shown(&self) -> bool {
        let state = self.0.borrow();
        !state.wrap && state.max().is_some_and(|max| max > 0.)
    }

    /// The strip's left edge in window px, as its canvas last saw it.
    pub(super) fn strip_x(&self) -> Pixels {
        self.0.borrow().strip_x
    }

    /// Records the strip's left edge `x` (window px), and asks for one more frame when it moved,
    /// so the bars follow a drawer that opens beside the list.
    pub(super) fn set_strip_x(&self, x: Pixels, window: &mut Window) {
        let moved = std::mem::replace(&mut self.0.borrow_mut().strip_x, x) != x;
        if moved {
            window.request_animation_frame();
        }
    }

    /// Moves the offset by `cols` columns (negative: left), clamped; true when it moved.
    /// Ignored while wrapping or before the extent is known.
    fn step(&self, cols: f32) -> bool {
        let mut state = self.0.borrow_mut();
        let (Some(max), false) = (state.max(), state.wrap) else {
            return false;
        };
        let next = (state.offset + cols * state.advance).clamp(0., max);
        let moved = next != state.offset;
        state.offset = next;
        moved
    }
}

impl NavoWindow {
    /// Moves the open text view sideways by `cols` columns and redraws; with no text view
    /// the key goes on to the next handler.
    fn sideways_step(&mut self, cols: f32, cx: &mut Context<Self>) {
        let file = self.compare.screen.as_mut().and_then(|s| s.file.as_mut());
        let Some(view) = file.and_then(|file| file.view.as_ref()) else {
            cx.propagate();
            return;
        };
        if view.sideways.step(cols) {
            cx.notify();
        }
    }

    /// Binds Left and Right (`viewer::ScrollLeft/ScrollRight`) on the file diff's `root`: they
    /// move the open text view 8 columns sideways (D39.3d).
    pub(in crate::window_view::compare_screen) fn sideways_keys(
        root: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &ScrollLeft, _, cx| {
            view.sideways_step(-KEY_COLS, cx);
        }))
        .on_action(cx.listener(|view, _: &ScrollRight, _, cx| {
            view.sideways_step(KEY_COLS, cx);
        }))
    }
}

/// A wheel listener for the list that moves `handle` and redraws the window behind `weak`.
pub(super) fn on_wheel<V: 'static>(
    handle: SidewaysHandle,
    weak: WeakEntity<V>,
) -> impl Fn(&ScrollEvent, &mut Window, &mut App) + 'static {
    move |event, _, cx| {
        if handle.wheel(&event.delta) {
            weak.update(cx, |_, cx| cx.notify()).ok();
        }
    }
}

/// What the sideways state shows, for tests.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
pub(in crate::window_view) struct SidewaysSeen {
    pub(in crate::window_view) offset: f32,
    pub(in crate::window_view) cols: Option<std::ops::Range<u32>>,
    pub(in crate::window_view) nudge: f32,
    pub(in crate::window_view) advance: f32,
    pub(in crate::window_view) max: Option<f32>,
}

#[cfg(test)]
impl SidewaysHandle {
    /// The offset, this frame's cut and the extent, for tests.
    pub(super) fn seen(&self) -> SidewaysSeen {
        let advance = px(self.0.borrow().advance);
        let cut = self.frame(advance);
        let state = self.0.borrow();
        SidewaysSeen {
            offset: state.offset,
            cols: cut.cols,
            nudge: f32::from(cut.nudge),
            advance: state.advance,
            max: state.max(),
        }
    }
}
