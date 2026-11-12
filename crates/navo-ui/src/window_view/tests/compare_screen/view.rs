//! The View menu and the drawers of the file diff (V17): each menu entry, drawers that start
//! closed, and Enter in the list of changes jumping to the change. W8i adds the Wrap lines entry
//! and Alt+Z; W8n the title-bar View button opening this menu on the compare screen.

use gpui_kit::AppContext as _;
use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt;
use navo_core::shell_menu::ItemId;

use super::super::compare_hex::hex_rig;
use super::super::compare_switch::open;
use super::super::screen_access::screen_read;
use super::super::settle;
use super::body::pair;
use super::{Rig, rig};
use crate::compare_diff::{Layout, PairKind};
use crate::context_menu::ContextMenuEvent;
use crate::window_view::compare_screen::look::Preset;
use crate::window_view::compare_screen::menu::{
    ShowSideBySide, ShowUnified, ToggleMap, ToggleWords,
};
use crate::window_view::viewer::{ViewCustom, ViewRich, ViewSimple};

/// The labels and checked states of the entries of the open View menu.
fn menu_entries(rig: &Rig, cx: &mut TestAppContext) -> Vec<(String, bool)> {
    let menu =
        screen_read(&rig.view, cx, |view, _| view.compare_menu_view()).expect("an open View menu");
    menu.read_with(cx, |menu, _| {
        menu.menu()
            .items
            .iter()
            .map(|item| (item.text.clone(), item.checked))
            .collect()
    })
}

/// Whether the open View menu offers "Wrap lines".
fn offers_wrap(rig: &Rig, cx: &mut TestAppContext) -> bool {
    menu_entries(rig, cx)
        .iter()
        .any(|(text, ..)| text == "Wrap lines")
}

/// Closes the open View menu choosing the entry that runs `compare::ToggleWrap`, as a click
/// would; the menu's own event path then dispatches it.
fn choose_wrap(rig: &Rig, cx: &mut TestAppContext) {
    let menu =
        screen_read(&rig.view, cx, |view, _| view.compare_menu_view()).expect("an open View menu");
    menu.update(cx, |_, cx| {
        cx.emit(ContextMenuEvent::Chosen(ItemId::Action(
            "compare::ToggleWrap",
        )));
    });
    cx.run_until_parked();
}

/// The wrap setting of the open file diff's look.
fn wrap(rig: &Rig, cx: &mut TestAppContext) -> bool {
    screen_read(&rig.view, cx, |view, _| view.file_look())
        .expect("an open file diff")
        .0
        .wrap
}

#[gpui_kit::test]
fn compare_screen_drawers_start_closed_and_the_look_starts_rich(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_text(cx, pair(400, &[10, 200]));
    let (look, changes, details) = screen_read(&rig.view, cx, |view, _| view.file_look()).unwrap();
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
    let open = screen_read(&rig.view, cx, |view, _| view.file_look()).unwrap();
    assert!(open.1 && !open.2, "Ctrl+B opens the list of changes only");
    rig.press(cx, "down");
    rig.press(cx, "down");
    assert_eq!(
        screen_read(&rig.view, cx, |v, _| v.file_drawer_cursor()),
        Some(2)
    );
    rig.press(cx, "enter");
    let (current, built, _) = rig.probe(cx, Layout::SideBySide);
    assert_eq!(current, Some(2));
    assert!(built.contains(&390), "the third change is shown: {built:?}");
    rig.press(cx, "escape");
    assert!(
        screen_read(&rig.view, cx, |v, _| v.file_screen().is_some()),
        "Esc closed the drawer only"
    );
    assert!(!screen_read(&rig.view, cx, |v, _| v.file_look()).unwrap().1);
    rig.press(cx, "escape");
    assert!(
        screen_read(&rig.view, cx, |v, _| v.file_screen().is_none()),
        "a second Esc goes back"
    );
}

#[gpui_kit::test]
fn compare_screen_ctrl_i_opens_the_details_drawer(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_text(cx, pair(40, &[10]));
    rig.press(cx, "ctrl-i");
    let open = screen_read(&rig.view, cx, |view, _| view.file_look()).unwrap();
    assert!(!open.1 && open.2);
    rig.press(cx, "ctrl-i");
    assert!(
        !screen_read(&rig.view, cx, |view, _| view.file_look())
            .unwrap()
            .2
    );
}

#[gpui_kit::test]
fn compare_screen_alt_v_opens_the_menu_and_each_entry_changes_the_look(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_text(cx, pair(40, &[10]));
    let look =
        |cx: &mut TestAppContext| screen_read(&rig.view, cx, |v, _| v.file_look()).unwrap().0;
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
    assert!(screen_read(&rig.view, cx, |view, _| view
        .compare_menu_view()
        .is_some()));
}

#[gpui_kit::test]
fn alt_z_flips_wrap_on_an_open_text_pair(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_text(cx, pair(40, &[10]));
    assert!(!wrap(&rig, cx), "wrap starts off");
    rig.press(cx, "alt-z");
    assert!(wrap(&rig, cx), "Alt+Z turns wrap on");
    rig.press(cx, "alt-z");
    assert!(!wrap(&rig, cx), "Alt+Z again turns wrap off");
}

#[gpui_kit::test]
fn the_wrap_menu_entry_flips_wrap_and_shows_checked(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_text(cx, pair(40, &[10]));
    rig.press(cx, "alt-v");
    let entries = menu_entries(&rig, cx);
    let entry = entries
        .iter()
        .find(|(text, ..)| text == "Wrap lines")
        .unwrap_or_else(|| panic!("the menu offers Wrap lines: {entries:?}"));
    assert!(!entry.1, "unchecked while wrap is off");
    choose_wrap(&rig, cx);
    assert!(wrap(&rig, cx), "choosing the entry turns wrap on");
    rig.press(cx, "alt-v");
    assert!(
        menu_entries(&rig, cx)
            .iter()
            .any(|(text, on)| text == "Wrap lines" && *on),
        "the entry shows checked while wrap is on"
    );
}

#[gpui_kit::test]
fn the_wrap_entry_shows_on_text_pairs_only(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let text = rig(cx);
    text.open_text(cx, pair(40, &[10]));
    text.press(cx, "alt-v");
    assert!(offers_wrap(&text, cx), "a text pair offers the entry");
    let hex = hex_rig(cx, vec![0x11; 0x2000], vec![0x11; 0x2000]);
    hex.press(cx, "enter");
    open(cx, &hex, (1, PairKind::Binary), None);
    hex.press(cx, "alt-v");
    assert!(!offers_wrap(&hex, cx), "a hex pair does not");
    let large = rig(cx);
    large.press(cx, "enter");
    open(
        cx,
        &large,
        (
            1,
            PairKind::Large {
                left: 40,
                right: 40,
            },
        ),
        None,
    );
    large.press(cx, "alt-v");
    assert!(!offers_wrap(&large, cx), "a large pair does not");
}

/// Clicks the title-bar "View" button, as a tester's pointer would.
fn click_view(cx: &mut TestAppContext, rig: &Rig) {
    cx.update_window(rig.window, |_, window, cx| window.click("window-view", cx))
        .expect("window");
    settle(cx, &rig.view);
}

/// The texts of the open window View menu; empty when it is closed.
fn window_menu_texts(cx: &mut TestAppContext, rig: &Rig) -> Vec<String> {
    rig.view.read_with(cx, |view, cx| {
        view.window_menu_view()
            .map(|menu| {
                menu.read(cx)
                    .menu()
                    .items
                    .iter()
                    .map(|item| item.text.clone())
                    .collect()
            })
            .unwrap_or_default()
    })
}

#[gpui_kit::test]
fn the_title_view_button_opens_the_compare_menu_on_the_compare_screen(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_text(cx, pair(40, &[10]));
    click_view(cx, &rig);
    assert!(
        offers_wrap(&rig, cx),
        "the title-bar View button opens the compare menu, which offers Wrap lines"
    );
}

#[gpui_kit::test]
fn the_title_view_button_opens_the_compare_menu_while_the_command_centre_is_focused(
    cx: &mut TestAppContext,
) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_text(cx, pair(40, &[10]));
    rig.press(cx, "ctrl-k");
    let centre_open = cx
        .update_window(rig.window, |_, window, cx| {
            window.render_frame(cx);
            window.try_find("command-center").is_some()
        })
        .expect("window");
    assert!(centre_open, "Ctrl+K opens the command centre");
    assert!(
        !rig.focused(cx, true),
        "the file diff no longer holds focus"
    );
    click_view(cx, &rig);
    assert!(
        offers_wrap(&rig, cx),
        "the title-bar View button opens the compare menu even with focus elsewhere"
    );
}

#[gpui_kit::test]
fn the_title_view_button_opens_the_window_menu_without_the_compare_screen(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    click_view(cx, &rig);
    let texts = window_menu_texts(cx, &rig);
    assert!(
        texts.iter().any(|text| text == "Path bar"),
        "the window View menu opens: {texts:?}"
    );
    assert!(
        screen_read(&rig.view, cx, |view, _| view.compare_menu_view().is_none()),
        "the compare menu stays closed while the screen is"
    );
}

#[gpui_kit::test]
fn choosing_the_simple_preset_keeps_wrap(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_text(cx, pair(40, &[10]));
    rig.press(cx, "alt-z");
    assert!(wrap(&rig, cx), "wrap is on");
    rig.dispatch(cx, &ViewSimple);
    let look = screen_read(&rig.view, cx, |view, _| view.file_look())
        .expect("an open file diff")
        .0;
    assert_eq!(look.preset, Preset::Simple);
    assert!(look.wrap, "Simple keeps wrap as it was (D39.8)");
}
