//! The hex body of the file diff at a narrow window (HX1): each half must stay inside its
//! own column, so the left bytes never draw over the right half.

use gpui_kit::AppContext as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{TestAppContext, VisualTestContext, px, size};

use super::compare_hex::{frame, hex_rig, open_hex};
use crate::compare_diff::HexAlign;

/// The right edge of the left half's row 0 stays at or left of the right half's own left edge.
#[gpui_kit::test]
fn compare_screen_hex_halves_stay_in_their_columns_when_narrow(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = hex_rig(cx, vec![0x11; 64], vec![0x22; 64]);
    open_hex(cx, &rig, &[(0..64, 0..64)], HexAlign::Offset);
    VisualTestContext::from_window(rig.window, cx).simulate_resize(size(px(780.), px(560.)));
    cx.run_until_parked();
    frame(cx, &rig);
    let (left, edge) = cx
        .update_window(rig.window, |_, window, cx| {
            let pad = crate::tokens::current(cx).spacing.xl + px(1.);
            let right = window.find(("paged-row", 1usize)).bounds();
            let left = window.find(("paged-row", 0usize)).bounds();
            (left, right.origin.x - pad)
        })
        .expect("window");
    assert!(
        left.right() <= edge,
        "the left half's row reaches {:?}, past the right half's left edge {edge:?}",
        left.right()
    );
}
