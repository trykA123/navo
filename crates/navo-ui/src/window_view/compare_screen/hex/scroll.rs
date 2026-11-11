//! Wheel, drag and scrollbar state of the hex body (V39), kept out of render.

#[cfg(test)]
use gpui_kit::base::ScrollbarHandle;
#[cfg(test)]
use gpui_kit::point;
use gpui_kit::{Pixels, ScrollEvent, px};

use super::{HexView, WHEEL_ROWS};

impl HexView {
    /// Scrolls by `rows` rows, negative upward, inside the list's bounds.
    pub(in crate::window_view::compare_screen) fn scroll_rows(
        &mut self,
        rows: i64,
        visible: usize,
    ) {
        let last = self.row_count().saturating_sub(visible.max(1));
        let rows = isize::try_from(rows).unwrap_or(isize::MAX);
        self.top = self.top.saturating_add_signed(rows).min(last);
    }

    /// Updates the scrollbar geometry after a scroll or a fill, never inside render.
    pub(in crate::window_view::compare_screen) fn refresh_scroll(&self, visible: usize) {
        let total = self.row_count();
        let top = self.top.min(total.saturating_sub(visible.max(1)));
        let pages = (total as f64 / visible.max(1) as f64).max(1.0);
        let max = total.saturating_sub(visible.max(1));
        let fraction = top as f64 / max.max(1) as f64;
        self.scroll.show(fraction, pages);
    }

    /// Handles a wheel event, keeping whole-row precision past f32 limits.
    pub(in crate::window_view::compare_screen) fn wheel(
        &mut self,
        event: &ScrollEvent,
        visible: usize,
        row_height: Pixels,
    ) {
        if row_height <= px(0.) {
            return;
        }
        let rows = match event.delta {
            gpui_kit::ScrollDelta::Lines(delta) => -f64::from(delta.y) * WHEEL_ROWS as f64,
            gpui_kit::ScrollDelta::Pixels(delta) => -f64::from(delta.y / row_height),
        };
        self.wheel_rows(rows, visible);
    }

    /// Adds fractional wheel rows and scrolls by the whole rows they make.
    pub(in crate::window_view::compare_screen) fn wheel_rows(&mut self, rows: f64, visible: usize) {
        self.wheel_remainder += rows;
        let whole = self.wheel_remainder.trunc();
        if whole != 0. {
            self.wheel_remainder -= whole;
            self.scroll_rows(whole as i64, visible);
        }
    }

    /// Moves to `fraction` of the scrollbar's travel; 1.0 reaches the last page.
    pub(in crate::window_view::compare_screen) fn drag_to(
        &mut self,
        fraction: f64,
        visible: usize,
    ) {
        let max = self.row_count().saturating_sub(visible.max(1));
        self.top = (fraction.clamp(0., 1.) * max as f64).round() as usize;
        self.top = self.top.min(max);
    }

    /// Moves the thumb to `fraction`, as a drag would, for tests.
    #[cfg(test)]
    pub(in crate::window_view::compare_screen) fn set_scroll_fraction(&self, fraction: f64) {
        let viewport = self.scroll.viewport_bounds().size.height;
        let content = self.scroll.content_size().height;
        let travel = (content - viewport).max(px(0.));
        self.scroll
            .set_offset(point(px(0.), -travel * fraction.clamp(0., 1.) as f32));
    }

    /// Polls a scrollbar drag; returns whether the top moved.
    pub(in crate::window_view::compare_screen) fn take_drag(&mut self, visible: usize) -> bool {
        let Some(fraction) = self.scroll.take_drag() else {
            return false;
        };
        self.drag_to(fraction, visible);
        true
    }

    /// The first visible row, for tests.
    #[cfg(test)]
    pub(in crate::window_view::compare_screen) fn top(&self) -> usize {
        self.top
    }

    /// Drops fetched rows, for a probe that draws before the background read lands.
    #[cfg(test)]
    pub(in crate::window_view::compare_screen) fn forget_rows(&mut self) {
        self.loaded.clear();
    }
}
