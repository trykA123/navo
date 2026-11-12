//! Sideways scroll of a text body (D39.2–D39.4, PC3, PC4; W8j): one offset in px that both text
//! columns share, the wheel that moves it, and the column cut every row draws from it. The file
//! diff (its `file` module) and the large body (its `large` module) each hold one handle per
//! open pair; the keys that move it are wired in `file/sideways.rs`.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::base::ScrollbarHandle;
use gpui_kit::{ScrollDelta, ScrollEvent, Size, WeakEntity, font};

use crate::compare_diff::body::{RowCut, TextProbe};
use crate::prelude::*;
use crate::tokens::Theme;

/// The sideways state of one open text body.
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
pub(in crate::window_view::compare_screen) struct SidewaysHandle(Rc<RefCell<Sideways>>);

impl SidewaysHandle {
    /// A state for a reloaded pair: the offset and wrap of `old`, kept unclamped until the new
    /// widths are measured.
    pub(in crate::window_view::compare_screen) fn carried(old: &Self) -> Self {
        let old = old.0.borrow();
        Self(Rc::new(RefCell::new(Sideways {
            offset: old.offset,
            advance: old.advance,
            wrap: old.wrap,
            ..Sideways::default()
        })))
    }

    /// Sets the widest line of the pair, in columns.
    pub(in crate::window_view::compare_screen) fn set_widest(&self, widest: u32) {
        self.0.borrow_mut().widest = Some(widest);
    }

    /// The widest line of the pair so far, in columns; `None` before the first measure. The
    /// large body raises it window by window (W8j) and never lowers it.
    pub(in crate::window_view::compare_screen) fn widest(&self) -> Option<u32> {
        self.0.borrow().widest
    }

    /// The width of a wrapped visual line in whole columns (D39.5): the text cell's columns less
    /// one for a wide character that starts last, at least 8. `None` before the first probe.
    pub(in crate::window_view::compare_screen) fn wrap_cols(&self) -> Option<u32> {
        let state = self.0.borrow();
        let width = state.text_width()?;
        let cols = (width / state.advance).floor();
        (cols.is_finite() && cols > 0.).then(|| (cols as u32).saturating_sub(1).max(8))
    }

    /// Turns wrap on or off; the offset stays as it is.
    pub(in crate::window_view::compare_screen) fn set_wrap(&self, wrap: bool) {
        self.0.borrow_mut().wrap = wrap;
    }

    /// The probe the first built row reports its text cells to.
    pub(in crate::window_view::compare_screen) fn probe(&self) -> TextProbe {
        self.0.borrow().probe.clone()
    }

    /// Takes this frame's column `advance` (px), clamps the offset once everything is known,
    /// and returns the cut every row draws (whole lines while wrapping or still measuring).
    pub(in crate::window_view::compare_screen) fn frame(&self, advance: Pixels) -> RowCut {
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
    fn wheel(&self, delta: &ScrollDelta) -> bool {
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
        // One column past the widest line, as `max` counts it, so dragging reaches the end.
        let wide = (state.widest.unwrap_or(0) as f32 + 1.) * state.advance;
        size(px(wide), state.probe.get().left.size.height)
    }
}

/// The width of one monospace column at the body's text size, in px. A text system that
/// measures nothing (tests) gets 0.6 × the size.
pub(in crate::window_view::compare_screen) fn advance(window: &Window, t: &Theme) -> Pixels {
    let system = window.text_system();
    let id = system.resolve_font(&font(t.typography.mono_font.clone()));
    match system.ch_width(id, t.typography.small) {
        Ok(width) if width > px(0.) => width,
        _ => t.typography.small * 0.6,
    }
}

impl SidewaysHandle {
    /// Whether the text is cut: the extent is known and above 0, and lines do not wrap.
    pub(in crate::window_view::compare_screen) fn cut_shown(&self) -> bool {
        let state = self.0.borrow();
        !state.wrap && state.max().is_some_and(|max| max > 0.)
    }

    /// The strip's left edge in window px, as its canvas last saw it.
    pub(in crate::window_view::compare_screen) fn strip_x(&self) -> Pixels {
        self.0.borrow().strip_x
    }

    /// Records the strip's left edge `x` (window px), and asks for one more frame when it moved,
    /// so the bars follow a drawer that opens beside the list.
    pub(in crate::window_view::compare_screen) fn set_strip_x(
        &self,
        x: Pixels,
        window: &mut Window,
    ) {
        let moved = std::mem::replace(&mut self.0.borrow_mut().strip_x, x) != x;
        if moved {
            window.request_animation_frame();
        }
    }

    /// Moves the offset by `cols` columns (negative: left), clamped; true when it moved.
    /// Ignored while wrapping or before the extent is known.
    pub(in crate::window_view::compare_screen) fn step(&self, cols: f32) -> bool {
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

/// A wheel listener for the list that moves `handle` and redraws the window behind `weak`.
pub(in crate::window_view::compare_screen) fn on_wheel<V: 'static>(
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
    pub(in crate::window_view::compare_screen) fn seen(&self) -> SidewaysSeen {
        let advance = px(self.0.borrow().advance);
        self.seen_at(advance)
    }

    /// Like `seen`, with this frame's column `advance` (px) given.
    pub(in crate::window_view::compare_screen) fn seen_at(&self, advance: Pixels) -> SidewaysSeen {
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

#[cfg(test)]
mod tests {
    use gpui_kit::base::ScrollbarHandle;

    use super::SidewaysHandle;
    use crate::compare_diff::body::ProbeSeen;
    use crate::prelude::*;

    #[test]
    fn the_bar_range_reaches_the_same_end_as_the_wheel() {
        let handle = SidewaysHandle::default();
        handle.probe().set(ProbeSeen {
            left: Bounds::new(point(px(0.), px(0.)), size(px(200.), px(20.))),
            ..ProbeSeen::default()
        });
        handle.set_widest(100);
        let seen = handle.seen_at(px(5.));
        let bar_range = f32::from(handle.content_size().width) - 200.;
        assert_eq!(Some(bar_range), seen.max);
    }

    /// `advance` must be the pen advance of one glyph (what the line shaper steps by), not the
    /// ink width of a "0": ten shaped columns are ten advances wide.
    #[gpui_kit::test]
    fn advance_is_the_shaped_column_width_of_the_mono_font(cx: &mut gpui_kit::TestAppContext) {
        let platform = gpui_kit::platform::current_platform(true);
        let mut cx = gpui_kit::TestAppContext::build_with_text_system(
            cx.dispatcher.clone(),
            None,
            platform.text_system(),
        );
        cx.update(|cx| crate::assets::load_fonts(cx).expect("bundled fonts load"));
        let window = cx.add_empty_window();
        window.update(|window, _| {
            let theme = crate::tokens::Theme::new(crate::tokens::Mode::Light);
            let size = theme.typography.small;
            let run = gpui_kit::TextRun {
                len: 10,
                font: gpui_kit::font(theme.typography.mono_font.clone()),
                color: theme.colors.text,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let shaped = window
                .text_system()
                .layout_line("0000000000", size, &[run], None);
            let column = f32::from(shaped.width) / 10.;
            let used = f32::from(super::advance(window, &theme));
            assert!(
                (used - column).abs() < 0.05,
                "advance {used} px, shaped column {column} px"
            );
        });
    }
}
