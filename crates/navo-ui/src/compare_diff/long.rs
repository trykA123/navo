//! Lines over 1 MiB in the file diff (V41d, D18): the length test and the "Show bytes here"
//! button every body draws at the end of such a line. A click opens the pair in Bytes there.

use std::rc::Rc;

use gpui_kit::Role;
use gpui_kit::base::TestSupportExt;
use navo_l10n::tr;

use crate::prelude::*;
use crate::tokens::Theme;

/// Bytes of display text above which a line counts as too long to read here.
pub const LONG_LINE: usize = 1 << 20;

/// Whether `text` is a line over 1 MiB.
#[must_use]
pub fn is_long(text: &str) -> bool {
    text.len() > LONG_LINE
}

/// The lines a drawn row holds, for the click on its button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LongHit {
    pub left: Option<u32>,
    pub right: Option<u32>,
}

/// Called with the lines of the row whose "Show bytes here" button was clicked.
pub type OnBytesHere = Rc<dyn Fn(LongHit, &mut Window, &mut App)>;

/// The button for a long line on `side` of list row `row`; it keeps its width at the end of the line.
pub fn bytes_here(
    t: &Theme,
    (row, side): (u32, &'static str),
    hit: LongHit,
    on: &OnBytesHere,
) -> impl IntoElement {
    let on = Rc::clone(on);
    let label = SharedString::from(tr("cmd-compare-show-bytes-here"));
    div()
        .id((side, row as usize))
        .role(Role::Button)
        .aria_label(label.clone())
        .test_support()
        .flex_none()
        .ml(t.spacing.sm)
        .px(t.spacing.sm)
        .rounded(t.radii.sm)
        .border_1()
        .border_color(t.colors.line)
        .bg(t.colors.rail)
        .text_color(t.colors.text)
        .cursor_pointer()
        .hover(|button| button.bg(t.colors.hover))
        .child(label)
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            on(hit, window, cx);
        })
}

#[cfg(test)]
mod tests {
    use super::{LONG_LINE, is_long};

    #[test]
    fn a_line_is_long_only_past_one_mebibyte() {
        assert!(!is_long(&"a".repeat(LONG_LINE)));
        assert!(is_long(&"a".repeat(LONG_LINE + 1)));
    }
}
