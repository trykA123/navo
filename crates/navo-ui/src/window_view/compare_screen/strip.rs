//! The sideways strip under a text body's list (D39.3c, W8j): a row with one horizontal bar per
//! text column, each placed at that column's probed text cell, shown only when lines are cut.
//! The file diff and the large body each mount it under their list.

use gpui_kit::base::{Scrollbar, ScrollbarMode, TestSupportExt};
use gpui_kit::canvas;

use super::sideways::SidewaysHandle;
use crate::compare_diff::Layout;
use crate::prelude::*;

/// One bar named `name` over the text cell `cell` (window px), in a strip whose left edge is
/// at `strip_x`.
fn bar(
    handle: &SidewaysHandle,
    name: &'static str,
    cell: Bounds<Pixels>,
    strip_x: Pixels,
) -> impl IntoElement {
    div()
        .id(name)
        .test_support()
        .absolute()
        .top_0()
        .h_full()
        .left((cell.origin.x - strip_x).max(px(0.)))
        .w(cell.size.width)
        .child(
            Scrollbar::horizontal(handle)
                .id(SharedString::from(format!("{name}-bar")))
                .viewport_from_layout()
                .mode(ScrollbarMode::Always),
        )
}

/// The strip under the list in `layout`: `Scrollbar::width()` tall with its bars while the text
/// is cut, else empty and 0 tall. A canvas records its left edge for the bars' places.
pub(in crate::window_view::compare_screen) fn strip(
    handle: &SidewaysHandle,
    layout: Layout,
) -> AnyElement {
    let shown = handle.cut_shown();
    let seen = handle.probe().get();
    let strip_x = handle.strip_x();
    let cells = [
        ("compare-sideways-left", Some(seen.left)),
        (
            "compare-sideways-right",
            (layout == Layout::SideBySide).then_some(seen.right),
        ),
    ];
    let bars = cells
        .into_iter()
        .filter_map(|(name, cell)| Some((name, cell?)))
        .filter(|(_, cell)| shown && cell.size.width > px(0.))
        .map(|(name, cell)| bar(handle, name, cell, strip_x));
    let probe = handle.clone();
    div()
        .relative()
        .flex_none()
        .w_full()
        .h(if shown { Scrollbar::width() } else { px(0.) })
        .child(
            canvas(
                move |bounds, window, _| probe.set_strip_x(bounds.origin.x, window),
                |_, (), _, _| {},
            )
            .absolute()
            .size_full(),
        )
        .children(bars)
        .into_any_element()
}
