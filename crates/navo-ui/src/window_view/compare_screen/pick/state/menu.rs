//! What the right-click menu asks of the picks (V33): the side picked from, whether the line
//! under the cursor can be picked or is, and moving the cursor onto a clicked line.

use super::{Pick, Spot, find, spots};
use crate::compare_diff::{PairText, PickSide};

impl Pick {
    /// The side the picked lines are on, if any line is picked.
    pub(in crate::window_view::compare_screen) fn picked_side(&self) -> Option<PickSide> {
        self.side.filter(|_| self.has_lines())
    }

    /// The side a clicked row is read on: the picks' side, else the left (else the right).
    pub(in crate::window_view::compare_screen) fn side_for(&self, has_left: bool) -> PickSide {
        match (self.side, has_left) {
            (Some(side), _) => side,
            (None, true) => PickSide::Left,
            (None, false) => PickSide::Right,
        }
    }

    /// Whether the keys' line (else the first of change `from`) can be picked, and if it
    /// can, whether it is picked now. `None` when the pair has no pickable line.
    pub(in crate::window_view::compare_screen) fn line_state(
        &self,
        text: &PairText,
        from: Option<usize>,
    ) -> Option<bool> {
        let spots = spots(text, self.side.unwrap_or(PickSide::Left));
        let spot = spots.get(self.here(&spots, from)?)?;
        Some(self.lines.contains(&spot.line))
    }

    /// The pickable line the cursor is on, if the keys have started.
    pub(in crate::window_view::compare_screen) fn cursor_spot(
        &self,
        text: &PairText,
    ) -> Option<Spot> {
        let spots = spots(text, self.side?);
        spots.get(find(&spots, self.cursor?)?).copied()
    }

    /// Puts the cursor on `line` of `side` without picking it. A line that cannot be picked
    /// leaves everything as it was. Returns whether the cursor moved.
    pub(in crate::window_view::compare_screen) fn point(
        &mut self,
        text: &PairText,
        (side, line): (PickSide, u32),
    ) -> bool {
        let spots = spots(text, side);
        if find(&spots, line).is_none() {
            return false;
        }
        self.begin(side);
        self.rebase(line);
        true
    }
}
