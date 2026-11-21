//! Key tests of H4: real keystrokes on a focused compare tab switch its views and the topbar mode.

use std::fs;

use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};

use sidewise_ui::window_view::ScreenEvent;

use crate::chrome::topbar_status::TopbarMode;
use crate::compare_tab_script::scripted_shell;
use crate::compare_tab_tests::{TempDir, submit, wait_for_row};
use crate::screens::compare_tab::{CompareTab, TabView};
use crate::shell::Shell;

type Opened = (AnyWindowHandle, Entity<Shell>);

/// A submitted compare of two folders that differ in `a.txt`, with the cursor on its row.
fn opened(cx: &mut TestAppContext, tag: &str) -> (Opened, Entity<CompareTab>, [TempDir; 2]) {
    let left = TempDir::new(&format!("{tag}-l"));
    let right = TempDir::new(&format!("{tag}-r"));
    fs::write(left.path().join("a.txt"), "one").expect("write left");
    fs::write(right.path().join("a.txt"), "two").expect("write right");
    let opened = scripted_shell(cx);
    submit(cx, &opened, [left.path(), right.path()]);
    assert!(wait_for_row(cx, &opened, "a.txt"), "rows reach the view");
    let tab = opened
        .1
        .read_with(cx, |shell, _| shell.active_compare().cloned());
    (opened, tab.expect("compare tab"), [left, right])
}

/// Types `keys` into the window and lets the tab settle.
fn press(cx: &mut TestAppContext, (window, _): &Opened, keys: &str) {
    cx.simulate_keystrokes(*window, keys);
    cx.run_until_parked();
}

/// The tab's shown view and the topbar's mode.
fn shown(
    cx: &mut TestAppContext,
    (_, view): &Opened,
    tab: &Entity<CompareTab>,
) -> (TabView, TopbarMode) {
    let mode = view.read_with(cx, |shell, cx| shell.topbar_mode(cx));
    (tab.read_with(cx, |tab, _| tab.view()), mode)
}

#[gpui_kit::test]
fn digits_switch_views_and_the_topbar_follows(cx: &mut TestAppContext) {
    let (open, tab, _dirs) = opened(cx, "digits");
    assert_eq!(
        shown(cx, &open, &tab),
        (TabView::Folders, TopbarMode::Folders)
    );
    press(cx, &open, "5");
    assert_eq!(shown(cx, &open, &tab), (TabView::Large, TopbarMode::Large));
    press(cx, &open, "2");
    assert_eq!(
        shown(cx, &open, &tab),
        (TabView::Folders, TopbarMode::Folders)
    );
    press(cx, &open, "4");
    assert_eq!(
        shown(cx, &open, &tab),
        (TabView::Stream, TopbarMode::Stream)
    );
}

#[gpui_kit::test]
fn three_needs_an_open_pair(cx: &mut TestAppContext) {
    let (open, tab, _dirs) = opened(cx, "three");
    press(cx, &open, "3");
    assert_eq!(shown(cx, &open, &tab).0, TabView::Folders, "no pair open");
    press(cx, &open, "enter");
    assert_eq!(shown(cx, &open, &tab), (TabView::File, TopbarMode::File));
    let screen = tab.read_with(cx, |tab, _| tab.screen().clone());
    cx.update_window(open.0, |_, _, cx| {
        screen.update(cx, |_, cx| cx.emit(ScreenEvent::Closed { reveal: None }));
    })
    .expect("window");
    cx.run_until_parked();
    press(cx, &open, "5");
    assert_eq!(shown(cx, &open, &tab).0, TabView::Large);
    press(cx, &open, "3");
    assert_eq!(shown(cx, &open, &tab), (TabView::File, TopbarMode::File));
}

#[gpui_kit::test]
fn one_adds_a_shell_tab(cx: &mut TestAppContext) {
    let (open, _tab, _dirs) = opened(cx, "one");
    press(cx, &open, "1");
    assert_eq!(open.1.read_with(cx, |shell, _| shell.tab_count()), 2);
    assert_eq!(
        open.1.read_with(cx, |shell, cx| shell.topbar_mode(cx)),
        TopbarMode::Start
    );
}

#[gpui_kit::test]
fn escape_on_large_goes_to_folders_and_on_folders_stays(cx: &mut TestAppContext) {
    let (open, tab, _dirs) = opened(cx, "esc");
    press(cx, &open, "5");
    press(cx, &open, "escape");
    assert_eq!(
        shown(cx, &open, &tab),
        (TabView::Folders, TopbarMode::Folders)
    );
    press(cx, &open, "escape");
    assert_eq!(shown(cx, &open, &tab).0, TabView::Folders);
}

#[gpui_kit::test]
fn digits_inside_the_screen_belong_to_navo(cx: &mut TestAppContext) {
    let (open, tab, _dirs) = opened(cx, "inside");
    press(cx, &open, "enter");
    assert_eq!(shown(cx, &open, &tab).0, TabView::File);
    for key in ["2", "4", "5", "1"] {
        press(cx, &open, key);
    }
    assert_eq!(shown(cx, &open, &tab).0, TabView::File);
    assert_eq!(open.1.read_with(cx, |shell, _| shell.tab_count()), 1);
}
