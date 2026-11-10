//! The View menu and the drawers of the file diff (V17): each menu entry, drawers that start
//! closed, and Enter in the list of changes jumping to the change.

use gpui_kit::TestAppContext;

use super::body::pair;
use super::rig;
use crate::compare_diff::Layout;
use crate::window_view::compare_screen::menu::{
    ShowSideBySide, ShowUnified, ToggleMap, ToggleWords,
};
use crate::window_view::viewer::{ViewCustom, ViewRich, ViewSimple};

#[gpui_kit::test]
fn compare_screen_drawers_start_closed_and_the_look_starts_rich(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_text(cx, pair(400, &[10, 200]));
    let (look, changes, details) = rig.view.read_with(cx, |view, _| view.file_look()).unwrap();
    assert!(look.words && look.map && look.layout.is_none());
    assert!(!changes && !details);
}

#[gpui_kit::test]
fn compare_screen_enter_in_the_list_of_changes_jumps_and_esc_closes_the_drawer(
    cx: &mut TestAppContext,
) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_text(cx, pair(400, &[10, 200, 390]));
    rig.press(cx, "ctrl-b");
    let open = rig.view.read_with(cx, |view, _| view.file_look()).unwrap();
    assert!(open.1 && !open.2, "Ctrl+B opens the list of changes only");
    rig.press(cx, "down");
    rig.press(cx, "down");
    assert_eq!(
        rig.view.read_with(cx, |v, _| v.file_drawer_cursor()),
        Some(2)
    );
    rig.press(cx, "enter");
    let (current, built, _) = rig.probe(cx, Layout::SideBySide);
    assert_eq!(current, Some(2));
    assert!(built.contains(&390), "the third change is shown: {built:?}");
    rig.press(cx, "escape");
    assert!(
        rig.view.read_with(cx, |v, _| v.file_screen().is_some()),
        "Esc closed the drawer only"
    );
    assert!(!rig.view.read_with(cx, |v, _| v.file_look()).unwrap().1);
    rig.press(cx, "escape");
    assert!(
        rig.view.read_with(cx, |v, _| v.file_screen().is_none()),
        "a second Esc goes back"
    );
}

#[gpui_kit::test]
fn compare_screen_ctrl_i_opens_the_details_drawer(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_text(cx, pair(40, &[10]));
    rig.press(cx, "ctrl-i");
    let open = rig.view.read_with(cx, |view, _| view.file_look()).unwrap();
    assert!(!open.1 && open.2);
    rig.press(cx, "ctrl-i");
    assert!(
        !rig.view
            .read_with(cx, |view, _| view.file_look())
            .unwrap()
            .2
    );
}

#[gpui_kit::test]
fn compare_screen_alt_v_opens_the_menu_and_each_entry_changes_the_look(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_text(cx, pair(40, &[10]));
    let look = |cx: &mut TestAppContext| rig.view.read_with(cx, |v, _| v.file_look()).unwrap().0;
    rig.dispatch(cx, &ViewSimple);
    assert_eq!((look(cx).words, look(cx).map), (false, false));
    rig.dispatch(cx, &ViewRich);
    assert_eq!((look(cx).words, look(cx).map), (true, true));
    rig.dispatch(cx, &ToggleWords);
    assert_eq!((look(cx).words, look(cx).map), (false, true));
    rig.dispatch(cx, &ToggleMap);
    assert_eq!((look(cx).words, look(cx).map), (false, false));
    rig.dispatch(cx, &ViewCustom);
    assert_eq!(
        (look(cx).words, look(cx).map),
        (false, false),
        "Custom keeps the parts"
    );
    rig.dispatch(cx, &ShowUnified);
    assert_eq!(look(cx).layout, Some(Layout::Unified));
    assert_eq!(
        rig.probe(cx, Layout::Unified).2,
        41,
        "unified shows the changed row twice"
    );
    rig.dispatch(cx, &ShowSideBySide);
    assert_eq!(look(cx).layout, Some(Layout::SideBySide));
    rig.press(cx, "alt-v");
    assert!(
        rig.view
            .read_with(cx, |view, _| view.compare_menu_view().is_some())
    );
}
