//! Whole-row scrolling and sticky folder band for the find results list.

use super::rows::Row;
use super::store::HitStore;

/// Rows scrolled per mouse-wheel notch.
pub const WHEEL_ROWS: usize = 3;

/// Whole-row viewport state of the results list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Scroll {
    /// First row index shown in the viewport.
    pub top: usize,
    /// Full rows that fit in the viewport height.
    pub visible: usize,
}

impl Scroll {
    /// Scrolls by `notches` wheel notches (three rows each), clamped to the list bounds.
    pub fn wheel(&mut self, notches: i32, rows: usize) {
        let max_top = self.max_top(rows);
        let steps = usize::try_from(notches.unsigned_abs()).unwrap_or(usize::MAX);
        let delta = steps.saturating_mul(WHEEL_ROWS);
        let moved = if notches >= 0 {
            self.top.saturating_add(delta)
        } else {
            self.top.saturating_sub(delta)
        };
        self.top = moved.min(max_top);
    }

    /// Folder id shown in the sticky band at `top`, or `None` when `top` is already a folder row.
    #[must_use]
    pub fn band(&self, rows: &[Row], store: &HitStore) -> Option<u32> {
        let file = match rows.get(self.top)? {
            Row::Folder(_) => return None,
            Row::File(file) | Row::Line { file, .. } | Row::More { file, .. } => *file,
        };
        store.file(file).map(|view| view.folder_id)
    }

    /// Moves `top` so `cursor` is neither behind the sticky band nor below the last full row.
    pub fn keep_visible(&mut self, cursor: usize, rows: &[Row], store: &HitStore) {
        if rows.is_empty() {
            self.top = 0;
            return;
        }
        let max_top = self.max_top(rows.len());
        self.top = self.top.min(max_top);
        if self.visible == 0 {
            return;
        }
        let cursor = cursor.min(rows.len() - 1);
        if cursor < self.top {
            self.top = cursor;
        } else if cursor >= self.top.saturating_add(self.visible) {
            self.top = cursor.saturating_add(1).saturating_sub(self.visible);
        }
        if self.visible > 1 && cursor == self.top && self.band(rows, store).is_some() {
            self.top = self.top.saturating_sub(1);
        }
        self.top = self.top.min(max_top);
    }

    /// Snaps a scrollbar fraction in `0.0..=1.0` to a whole row index in `0..rows`.
    #[must_use]
    pub fn snap(fraction: f32, rows: usize) -> usize {
        if rows <= 1 || fraction.is_nan() {
            return 0;
        }
        let max = rows - 1;
        let clamped = f64::from(fraction.clamp(0.0, 1.0));
        let snapped = (clamped * max as f64).round();
        if snapped <= 0.0 {
            0
        } else {
            (snapped as usize).min(max)
        }
    }

    fn max_top(&self, rows: usize) -> usize {
        rows.saturating_sub(self.visible.max(1))
    }
}
