//! Bytes per hex row (B2): the widest of 16, 8 or 4 bytes whose offset, hex and ASCII columns
//! fit one half of the body. Both halves share it, so their rows line up; below 4 rows clip.

use super::HexView;
use crate::prelude::*;
use crate::tokens::Theme;

/// The row widths tried, widest first; the last one is the floor.
const WIDTHS: [u64; 3] = [16, 8, 4];

/// The text columns of a hex row of `bytes` bytes: "XX" per byte, a space between, and one
/// more space after the eighth byte.
pub(super) fn hex_columns(bytes: u64) -> usize {
    let columns = (bytes * 3).saturating_sub(1) + u64::from(bytes > 8);
    usize::try_from(columns).unwrap_or(usize::MAX)
}

/// The px a row of `bytes` bytes needs: the paged row's padding, gutter and three gaps, then
/// the offset (`digits` columns), hex and ASCII text at `advance` px per column.
fn row_needs(t: &Theme, (bytes, digits): (u64, u64), advance: Pixels) -> Pixels {
    let columns = digits + hex_columns(bytes) as u64 + bytes;
    t.spacing.xs + t.spacing.sm + t.spacing.xl * 4. + advance * columns as f32
}

/// The bytes per row for a hex body `body` px wide, offsets `digits` columns long.
fn fit(t: &Theme, body: Pixels, digits: u64, advance: Pixels) -> u64 {
    // Flex shares what is left after the row gap and the right half's border and padding.
    let half = (body - t.spacing.md - t.spacing.xl - px(1.)) / 2.;
    let fits = |bytes: &u64| row_needs(t, (*bytes, digits), advance) <= half;
    WIDTHS.into_iter().find(fits).unwrap_or(WIDTHS[2])
}

impl HexView {
    /// Fits the bytes per row to the body width the last frame measured, `advance` px per
    /// text column; true when it changed and the rows were rebuilt. Unmeasured, it keeps 16.
    pub(in super::super) fn fit_width(&mut self, t: &Theme, advance: Pixels) -> bool {
        let body = self.body_width.get();
        if body <= px(0.) {
            return false;
        }
        let digits = match self.left_wide || self.right_wide {
            true => 16,
            false => 8,
        };
        let width = fit(t, body, digits, advance);
        if width == self.width {
            return false;
        }
        self.width = width;
        self.rebuild();
        true
    }
}
