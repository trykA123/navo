//! A map jump in the wrapped file diff (P42 W8p, follow-up of W8o): the same pair as W8o's
//! Alt+↓ test, jumped through the change map instead of through the key.

use gpui_kit::TestAppContext;

use super::super::compare_switch::open;
use super::super::screen_access::screen_read;
use super::body::pair;
use super::rig;
use super::wrap::{centred, frames, in_view, index, opened, seen, set_wrap, wrapped_pair};
use crate::compare_diff::PairKind;

/// W8o's shape: once change 1 centres at the top of the short list, change 2's first item is
/// already fully visible; a map jump to change 2 must centre it strictly, not do nothing.
#[gpui_kit::test]
fn a_map_jump_centres_a_change_whose_first_item_is_already_visible(cx: &mut TestAppContext) {
    use crate::compare_diff::map::MapEvent;

    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    opened(&rig, cx, wrapped_pair(6, &[1, 4], 10));
    set_wrap(&rig, cx, true);
    let (_, cols) = index(&rig, cx);
    let room = seen(&rig, cx).3;
    assert!(room >= 10, "the rig shows {room} items");
    let wide = cols as usize * (room - 5);
    let mut text = pair(6, &[1, 4]);
    for at in [1, 4] {
        text.left[at] = "l".repeat(wide);
        text.right[at] = "r".repeat(wide);
    }
    open(cx, &rig, (2, PairKind::Text), Some(text));
    frames(&rig, cx);
    let (index, _) = index(&rig, cx);
    let lines = index.lines_of(1) as usize;
    assert_eq!(lines, room - 5, "the long rows wrap to {lines} items");
    rig.press(cx, "f7");
    frames(&rig, cx);
    let first = index.first_of(4);
    let (_, _, top, room) = seen(&rig, cx);
    assert_eq!(top, 0, "change 1 centres at the top of the short list");
    assert!(
        in_view(top, room, first),
        "change 2's first item is on screen"
    );
    assert!(
        first + lines > top + room,
        "the rest of change 2 is below it"
    );
    let map = screen_read(&rig.view, cx, |view, _| view.file_map()).expect("a mounted change map");
    map.update(cx, |_, cx| cx.emit(MapEvent::Jump(4)));
    frames(&rig, cx);
    let (_, _, top, room) = seen(&rig, cx);
    assert!(
        centred(top, room, first),
        "the map jump centres change 2: item {first} at top {top}, room {room}"
    );
}
