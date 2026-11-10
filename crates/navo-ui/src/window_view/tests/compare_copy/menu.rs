//! The right-click menu (V33): Shift+F10 and the Menu key open it on the file diff and the
//! Stream, a right-click moves the cursor to the row first, and an entry runs its action.

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext as _, InputEvent as _, Modifiers, MouseButton, MouseDownEvent, TestAppContext,
};

use crate::compare_diff::{CopyDirection, Layout};
use crate::window_view::tests::compare_screen::Rig;
use crate::window_view::tests::compare_screen::rig;

impl Rig {
    /// Whether the compare menu is open.
    fn menu_open(&self, cx: &mut TestAppContext) -> bool {
        self.view
            .read_with(cx, |view, _| view.compare_menu_view().is_some())
    }

    /// Right-clicks the middle of the element `id` and draws.
    fn right_click(&self, cx: &mut TestAppContext, id: (&'static str, usize)) {
        cx.update_window(self.window, |_, window, cx| {
            let position = window.find(id).bounds().center();
            let event = MouseDownEvent {
                button: MouseButton::Right,
                position,
                modifiers: Modifiers::default(),
                click_count: 1,
                first_mouse: false,
            };
            window.dispatch_event(event.to_platform_input(), cx);
        })
        .expect("window");
        self.frame(cx);
    }
}

#[gpui_kit::test]
fn compare_copy_menu_shift_f10_and_the_menu_key_open_it_on_the_file_diff(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_pair(cx);
    assert!(!rig.menu_open(cx));
    rig.press(cx, "shift-f10");
    assert!(rig.menu_open(cx));
    rig.press(cx, "escape");
    assert!(!rig.menu_open(cx), "Esc closes the menu first");
    rig.press(cx, "menu");
    assert!(rig.menu_open(cx));
}

#[gpui_kit::test]
fn compare_copy_menu_right_click_moves_the_cursor_and_an_entry_copies(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_pair(cx);
    rig.right_click(cx, ("compare-file-row", 3));
    assert!(rig.menu_open(cx));
    let current = rig
        .view
        .update(cx, |view, _| view.file_probe(Layout::SideBySide));
    assert_eq!(current.expect("an opened pair").0, Some(0));
    rig.press(cx, "enter");
    assert!(!rig.menu_open(cx), "choosing closes the menu");
    let asked = rig.diff.copies();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].direction, CopyDirection::ToLeft);
}

#[gpui_kit::test]
fn compare_copy_menu_opens_on_the_stream_and_copies_the_current_change(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_stream_card(cx);
    rig.press(cx, "j");
    rig.press(cx, "shift-f10");
    assert!(rig.menu_open(cx));
    rig.press(cx, "down");
    rig.press(cx, "enter");
    let asked = rig.diff.copies();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].direction, CopyDirection::ToRight);
}
