//! The tone edge of a diff text column: a wash across the column and a bar at its left.
//! Shared by the file diff body and the Stream cards; line numbers stay outside it.

use gpui_kit::transparent_black;

use crate::prelude::*;
use crate::tokens::DiffTone;

/// Width of the edge bar, in px.
const EDGE_PX: f32 = 3.;

/// Gives a text column its tone: the wash as background and a bar in the mark colour at its
/// left. Every column keeps the bar's width (transparent without a tone) so text stays aligned.
pub(super) fn edged(cell: Div, tone: Option<DiffTone>) -> Div {
    cell.border_l(px(EDGE_PX))
        .border_color(tone.map_or_else(transparent_black, |tone| tone.mark))
        .when_some(tone, |cell, tone| cell.bg(tone.wash))
}

#[cfg(test)]
mod tests {
    use super::edged;
    use crate::prelude::*;
    use crate::tokens::{Mode, Theme};
    use gpui_kit::transparent_black;

    #[test]
    fn edged_draws_wash_and_mark_bar_for_a_tone_and_neither_without() {
        let t = Theme::new(Mode::Dark);
        for tone in [t.diff.change, t.diff.add, t.diff.delete] {
            let mut cell = edged(div(), Some(tone));
            let style = cell.style();
            assert_eq!(style.background, Some(tone.wash.into()));
            assert_eq!(style.border_color, Some(tone.mark));
        }
        let mut equal = edged(div(), None);
        let style = equal.style();
        assert_eq!(style.background, None);
        assert_eq!(style.border_color, Some(transparent_black()));
    }
}
