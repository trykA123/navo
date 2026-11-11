//! The folder compare's change map (D36): only the pane that shows the compare's right side draws
//! it, at the trailing edge of its list, and a click on it jumps the rows both panes share.

use gpui_kit::test::{TestQueryExt, TestWindowExt};
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Entity, Modifiers, MouseButton, Pixels, Role,
    TestAppContext, point, px,
};

use navo_l10n::tr;

use super::{Rig, first_labels, frames, rig};
use crate::compare_diff::map::ChangeMap;
use crate::compare_list::BarEvent;
use crate::test_pointer::{dispatch, down, up};

/// The id the folder map draws under.
const MAP: &str = "compare-folder-map";

/// The bounds of every element drawn with this id, window-wide.
fn drawn(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    id: &'static str,
) -> Vec<Bounds<Pixels>> {
    cx.update_window(window, |_, window, _| {
        window
            .find_all(id)
            .iter()
            .map(|item| item.bounds())
            .collect()
    })
    .expect("window")
}

/// The bounds of the compare list that shows the side named `label`.
fn side_bounds(cx: &mut TestAppContext, window: AnyWindowHandle, label: &str) -> Bounds<Pixels> {
    cx.update_window(window, |_, window, _| window.find_by_label(label).bounds())
        .expect("window")
}

/// The row labels the panes draw, split into the window's left and right halves.
fn halves(cx: &mut TestAppContext, window: AnyWindowHandle) -> [Vec<String>; 2] {
    cx.update_window(window, |_, window, _| {
        let edge = window.viewport_size().width / 2.;
        let pick = |right: bool| {
            window
                .find_all_by_role(Role::ListItem)
                .iter()
                .filter(|item| (item.bounds().origin.x >= edge) == right)
                .filter_map(|item| item.label().map(str::to_owned))
                .collect()
        };
        [pick(false), pick(true)]
    })
    .expect("window")
}

/// The centre `y` of the row drawn at this fraction of a strip from `top` to `top + height`.
fn row_y(top: Pixels, height: Pixels, part: f32) -> Pixels {
    top + height * part
}

#[gpui_kit::test]
fn only_the_pane_that_shows_the_compare_right_side_draws_the_map(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, "ctrl-shift-d");
    rig.send_files(cx, &["a.txt", "b.txt", "c.txt", "d.txt"]);
    frames(cx, rig.window);
    let left = side_bounds(cx, rig.window, &tr("compare-list-left"));
    let right = side_bounds(cx, rig.window, &tr("compare-list-right"));
    let maps = drawn(cx, rig.window, MAP);
    assert_eq!(
        maps.len(),
        1,
        "one map in the window, beside the list that shows the compare's right side"
    );
    let map = maps[0];
    assert!(
        map.size.width < px(20.),
        "the folder map is the narrow one, not the file diff's 22 px: {map:?}"
    );
    assert!(
        (f32::from(map.origin.x) - f32::from(right.origin.x + right.size.width)).abs() <= 1.,
        "the map sits at the trailing edge of that list: {map:?} in {right:?}"
    );
    assert!(
        (f32::from(map.size.height) - f32::from(right.size.height)).abs() <= 1.,
        "the map is as tall as its list: {map:?} in {right:?}"
    );
    assert!(
        map.origin.x >= left.origin.x + left.size.width,
        "the pane that shows the compare's left side draws no map: {map:?} vs {left:?}"
    );
}

#[gpui_kit::test]
fn a_click_at_the_far_end_of_the_map_scrolls_both_panes_to_that_row(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, "ctrl-shift-d");
    let names: Vec<String> = (0..400).map(|n| format!("f{n:03}.txt")).collect();
    rig.send_files(cx, &names.iter().map(String::as_str).collect::<Vec<_>>());
    frames(cx, rig.window);
    let maps = drawn(cx, rig.window, MAP);
    assert_eq!(maps.len(), 1, "the map is drawn before the click");
    let map = maps[0];
    let at = point(
        map.origin.x + map.size.width / 2.,
        row_y(map.origin.y, map.size.height, 300.5 / 400.),
    );
    let none = Modifiers::default();
    dispatch(cx, rig.window, down(MouseButton::Left, at, none));
    dispatch(cx, rig.window, up(MouseButton::Left, at, none));
    frames(cx, rig.window);
    let [left, right] = halves(cx, rig.window);
    assert!(
        left.iter().any(|label| label.starts_with("f300")),
        "the pane showing the compare's left side jumped to row 300: {left:?}"
    );
    assert!(
        right.iter().any(|label| label.starts_with("f300")),
        "the pane showing the compare's right side jumped to row 300: {right:?}"
    );
    let [first_left, first_right] = first_labels(cx, rig.window);
    assert_eq!(first_left, first_right, "both panes still draw one row");
}

#[gpui_kit::test]
fn an_empty_compare_draws_no_map(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, "ctrl-shift-d");
    frames(cx, rig.window);
    assert!(
        drawn(cx, rig.window, MAP).is_empty(),
        "a compare with no rows draws no map"
    );
}

impl Rig {
    /// The mounted folder map entity, or `None` when the compare has dropped it.
    fn folder_map(&self, cx: &mut TestAppContext) -> Option<Entity<ChangeMap>> {
        self.view.read_with(cx, |view, _| view.folder_map())
    }
}

#[gpui_kit::test]
fn leaving_the_compare_drops_the_map_entity(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, "ctrl-shift-d");
    rig.send_files(cx, &["a.txt", "b.txt"]);
    frames(cx, rig.window);
    assert!(
        rig.folder_map(cx).is_some(),
        "the map is mounted while comparing"
    );
    rig.bar(cx, BarEvent::Leave);
    frames(cx, rig.window);
    assert!(
        rig.folder_map(cx).is_none(),
        "leaving the compare drops the map entity and its subscription"
    );
}

#[gpui_kit::test]
fn a_jump_to_the_last_row_clips_the_map_viewport_to_the_row_count(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, "ctrl-shift-d");
    let names: Vec<String> = (0..60).map(|n| format!("f{n:02}.txt")).collect();
    rig.send_files(cx, &names.iter().map(String::as_str).collect::<Vec<_>>());
    frames(cx, rig.window);
    let map = drawn(cx, rig.window, MAP)[0];
    let at = point(
        map.origin.x + map.size.width / 2.,
        map.origin.y + map.size.height * 0.99,
    );
    let none = Modifiers::default();
    dispatch(cx, rig.window, down(MouseButton::Left, at, none));
    dispatch(cx, rig.window, up(MouseButton::Left, at, none));
    frames(cx, rig.window);
    let entity = rig.folder_map(cx).expect("the folder map");
    let seen = entity.read_with(cx, |map, _| map.viewport());
    assert_eq!(
        seen.end, 60,
        "the viewport ends at the row count, not past it"
    );
    assert!(seen.start < seen.end, "rows are on screen: {seen:?}");
}
