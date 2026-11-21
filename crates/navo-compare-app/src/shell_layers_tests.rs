//! Tests of L1: the Keys sheet and the View menu mounted in the shell, opened from the topbar and
//! the keys, and the View action's availability by screen.

use navo_compare_cli::args::SideSpec;
use std::fs;
use std::sync::Arc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, Modifiers, TestAppContext, VisualTestContext,
};
use navo_compare_profile::appearance::{UiPrefs, ViewPresetPref};
use sidewise_core::Location;
use sidewise_ui::browser::{CompareControl, CompareHooks, CompareSink};
use sidewise_ui::compare_diff::CompareOptions;
use sidewise_ui::window_view::LookPreset;

use crate::compare_tab_tests::{TempDir, shell, submit};
use crate::screens::compare_tab::{CompareRequest, TabView, install_test_hooks};
use crate::shell::Shell;
use crate::shell_prefs::{install_prefs, prefs, save_prefs};
use crate::start_tabs::StartChoice;

type Opened = (AnyWindowHandle, Entity<Shell>);

/// A compare engine that starts and then does nothing.
struct Quiet;

impl CompareHooks for Quiet {
    /// Every location can be compared in this fixture.
    fn supports(&self, _location: &Location) -> bool {
        true
    }

    /// Returns an idle control and drops the sink.
    fn start(
        &self,
        _sides: (&Location, &Location),
        _options: &CompareOptions,
        _sink: CompareSink,
    ) -> Box<dyn CompareControl> {
        Box::new(Idle)
    }
}

/// A compare control that ignores every request.
struct Idle;

impl CompareControl for Idle {
    /// Ignores a focus request.
    fn focus(&self, _row_ids: Vec<u64>) {}
    /// Ignores a stop request for one row.
    fn stop_row(&self, _id: u64) {}
    /// Ignores a stop request for the whole walk.
    fn stop_all(&self) {}
    /// Ignores a refresh request.
    fn refresh(&self) {}
    /// Ignores a request to resume a stopped walk.
    fn continue_walk(&self) {}
}

/// Clicks the centre of the element `id` and lets the window settle.
fn click(cx: &mut TestAppContext, window: AnyWindowHandle, id: &'static str) {
    let at = cx
        .update_window(window, |_, window, _| window.find(id).bounds().center())
        .expect("window");
    VisualTestContext::from_window(window, cx).simulate_click(at, Modifiers::none());
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("window");
}

/// Reads the Keys sheet and View menu state: `(keys open, menu open)`.
fn open_state(cx: &mut TestAppContext, (_, view): &Opened) -> (bool, bool) {
    view.read_with(cx, |shell, cx| (shell.keys_open(cx), shell.view_open()))
}

/// The focused element's handle, to compare before and after a layer.
fn focus_id(cx: &mut TestAppContext, window: AnyWindowHandle) -> Option<gpui_kit::FocusHandle> {
    cx.update_window(window, |_, window, cx| window.focused(cx))
        .expect("window")
}

/// A shell on a compare tab of two folders (quiet engine) or two files (opens the File view).
fn on_compare(cx: &mut TestAppContext, tag: &str, files: bool) -> (Opened, [TempDir; 2]) {
    let dirs = [
        TempDir::new(&format!("{tag}-l")),
        TempDir::new(&format!("{tag}-r")),
    ];
    let opened = shell(cx);
    cx.update(|cx| install_test_hooks(Arc::new(Quiet), cx));
    if files {
        let (left, right) = (dirs[0].path().join("a.txt"), dirs[1].path().join("a.txt"));
        fs::write(&left, "one").expect("write");
        fs::write(&right, "two").expect("write");
        let request = CompareRequest {
            left: SideSpec::Path(left),
            right: SideSpec::Path(right),
            choice: StartChoice::default(),
        };
        let (window, view) = &opened;
        cx.update_window(*window, |_, window, cx| {
            view.update(cx, |shell, cx| shell.open_compare((0, request), window, cx))
        })
        .expect("window")
        .expect("open");
    } else {
        submit(cx, &opened, [dirs[0].path(), dirs[1].path()]);
    }
    cx.run_until_parked();
    (opened, dirs)
}

#[gpui_kit::test]
fn on_start_the_keys_action_and_the_key_open_the_sheet_and_escape_closes_it(
    cx: &mut TestAppContext,
) {
    let opened = shell(cx);
    let before = focus_id(cx, opened.0);
    assert!(before.is_some(), "start focuses a field");
    click(cx, opened.0, "top-keys");
    assert_eq!(open_state(cx, &opened), (true, false), "Keys pill opens it");
    cx.simulate_keystrokes(opened.0, "escape");
    assert_eq!(open_state(cx, &opened), (false, false), "Escape closes it");
    assert_eq!(focus_id(cx, opened.0), before, "focus returns");
    cx.simulate_keystrokes(opened.0, "f1");
    assert_eq!(open_state(cx, &opened), (true, false), "F1 opens it");
    cx.simulate_keystrokes(opened.0, "escape");
    assert_eq!(open_state(cx, &opened), (false, false));
    assert_eq!(focus_id(cx, opened.0), before, "focus returns again");
}

#[gpui_kit::test]
fn on_start_alt_v_and_the_pill_open_the_view_menu_and_escape_returns_focus(
    cx: &mut TestAppContext,
) {
    let opened = shell(cx);
    assert!(opened.1.read_with(cx, |s, cx| s.view_action_available(cx)));
    let before = focus_id(cx, opened.0);
    cx.simulate_keystrokes(opened.0, "alt-v");
    assert_eq!(open_state(cx, &opened), (false, true), "alt-v opens it");
    cx.simulate_keystrokes(opened.0, "escape");
    assert_eq!(open_state(cx, &opened), (false, false), "Escape closes it");
    assert_eq!(focus_id(cx, opened.0), before, "focus returns to the tab");
    click(cx, opened.0, "top-view");
    assert_eq!(open_state(cx, &opened), (false, true), "the pill opens it");
    click(cx, opened.0, "top-view");
    assert_eq!(
        open_state(cx, &opened),
        (false, false),
        "the pill closes it"
    );
}

#[gpui_kit::test]
fn picking_simple_saves_the_preset_to_the_profile(cx: &mut TestAppContext) {
    let profile = TempDir::new("view-save");
    let opened = shell(cx);
    cx.update(|cx| install_prefs(Some(profile.path()), cx))
        .expect("install");
    cx.simulate_keystrokes(opened.0, "alt-v");
    cx.simulate_keystrokes(opened.0, "up enter");
    assert_eq!(cx.update(|cx| prefs(cx)).preset, ViewPresetPref::Simple);
    let text = fs::read_to_string(profile.path().join("settings.toml")).expect("settings.toml");
    assert!(text.contains("preset = \"simple\""), "saved: {text}");
    assert_eq!(open_state(cx, &opened), (false, true), "stays open");
}

#[gpui_kit::test]
fn view_is_available_on_folders_only_of_the_compare_screens(cx: &mut TestAppContext) {
    for (tag, files, want, open) in [
        ("folders", false, TabView::Folders, true),
        ("file", true, TabView::File, false),
    ] {
        let (opened, _dirs) = on_compare(cx, tag, files);
        let tab = opened.1.read_with(cx, |s, _| s.active_compare().cloned());
        assert_eq!(tab.expect("tab").read_with(cx, |t, _| t.view()), want);
        assert_eq!(
            opened.1.read_with(cx, |s, cx| s.view_action_available(cx)),
            open
        );
        cx.simulate_keystrokes(opened.0, "alt-v");
        assert_eq!(open_state(cx, &opened), (false, open));
    }
}

#[gpui_kit::test]
fn the_keys_sheet_opens_on_a_compare_tab_too(cx: &mut TestAppContext) {
    let (opened, _dirs) = on_compare(cx, "keys", false);
    cx.simulate_keystrokes(opened.0, "f1");
    assert_eq!(open_state(cx, &opened), (true, false));
    cx.simulate_keystrokes(opened.0, "escape");
    assert_eq!(open_state(cx, &opened), (false, false));
}

/// The look of the active compare tab's screen.
fn tab_look(cx: &mut TestAppContext, opened: &Opened) -> sidewise_ui::window_view::LookDefaults {
    let tab = opened.1.read_with(cx, |s, _| s.active_compare().cloned());
    let screen = tab.expect("tab").read_with(cx, |t, _| t.screen().clone());
    screen.read_with(cx, |s, _| s.look_defaults())
}

#[gpui_kit::test]
fn a_new_compare_tab_opens_with_the_saved_look(cx: &mut TestAppContext) {
    let profile = TempDir::new("look-new");
    cx.update(|cx| install_prefs(Some(profile.path()), cx))
        .expect("install");
    let simple = UiPrefs {
        preset: ViewPresetPref::Simple,
        ..cx.update(|cx| prefs(cx))
    };
    cx.update(|cx| save_prefs(simple, cx)).expect("save");
    let (opened, _dirs) = on_compare(cx, "look-new", false);
    assert_eq!(tab_look(cx, &opened).preset, LookPreset::Simple);
}

#[gpui_kit::test]
fn changing_the_view_updates_an_open_compare_tab(cx: &mut TestAppContext) {
    let profile = TempDir::new("look-open");
    cx.update(|cx| install_prefs(Some(profile.path()), cx))
        .expect("install");
    let (opened, _dirs) = on_compare(cx, "look-open", false);
    assert_eq!(tab_look(cx, &opened).preset, LookPreset::Rich);
    cx.simulate_keystrokes(opened.0, "alt-v");
    cx.simulate_keystrokes(opened.0, "up enter");
    assert_eq!(tab_look(cx, &opened).preset, LookPreset::Simple);
}

/// The bounds of the element `id` in the last frame.
fn bounds_of(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    id: &'static str,
) -> gpui_kit::Bounds<gpui_kit::Pixels> {
    cx.update_window(window, |_, window, _| window.find(id).bounds())
        .expect("window")
}

/// B4: at 1200 x 760 the open View menu floats under the View pill, right-aligned to it, and
/// the main pane keeps the full window height.
#[gpui_kit::test]
fn the_view_menu_drops_under_the_view_pill_and_leaves_the_layout_alone(cx: &mut TestAppContext) {
    let profile = TempDir::new("view-anchor");
    let opened = shell(cx);
    cx.update(|cx| install_prefs(Some(profile.path()), cx))
        .expect("install");
    VisualTestContext::from_window(opened.0, cx)
        .simulate_resize(gpui_kit::size(gpui_kit::px(1200.), gpui_kit::px(760.)));
    cx.run_until_parked();
    let pane = bounds_of(cx, opened.0, "main-pane");
    cx.simulate_keystrokes(opened.0, "alt-v");
    cx.update_window(opened.0, |_, window, cx| window.render_frame(cx))
        .expect("window");
    assert_eq!(open_state(cx, &opened), (false, true), "alt-v opens it");
    let pill = bounds_of(cx, opened.0, "top-view");
    let menu = bounds_of(cx, opened.0, "view-menu");
    let near = |a: gpui_kit::Pixels, b: gpui_kit::Pixels| (a - b).abs() <= gpui_kit::px(8.);
    assert!(
        near(menu.top(), pill.bottom()),
        "menu {menu:?} under pill {pill:?}"
    );
    assert!(
        near(menu.right(), pill.right()),
        "menu {menu:?} right-aligned to {pill:?}"
    );
    assert_eq!(
        bounds_of(cx, opened.0, "main-pane"),
        pane,
        "the pane keeps its bounds"
    );
    assert_eq!(
        pane.bottom(),
        gpui_kit::px(760.),
        "the pane reaches the window bottom"
    );
    let simple = cx
        .update_window(opened.0, |_, window, _| {
            window.find(("view-item", 0usize)).bounds().center()
        })
        .expect("window");
    VisualTestContext::from_window(opened.0, cx).simulate_click(simple, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| prefs(cx)).preset,
        ViewPresetPref::Simple,
        "a click reaches it"
    );
}

/// VM6: a mouse-down outside the open View menu closes it like a context menu: the click does not
/// reach the Keys pill under it, and focus returns to the tab as Escape does.
#[gpui_kit::test]
fn a_click_outside_the_view_menu_closes_it_and_returns_focus(cx: &mut TestAppContext) {
    let opened = shell(cx);
    let before = focus_id(cx, opened.0);
    cx.simulate_keystrokes(opened.0, "alt-v");
    cx.update_window(opened.0, |_, window, cx| window.render_frame(cx))
        .expect("window");
    assert_eq!(open_state(cx, &opened), (false, true), "alt-v opens it");
    click(cx, opened.0, "top-keys");
    assert_eq!(
        open_state(cx, &opened),
        (false, false),
        "the outside click closes the menu and stops there"
    );
    assert_eq!(focus_id(cx, opened.0), before, "focus returns to the tab");
    cx.simulate_keystrokes(opened.0, "alt-v");
    cx.update_window(opened.0, |_, window, cx| window.render_frame(cx))
        .expect("window");
    let pane = bounds_of(cx, opened.0, "main-pane");
    let at = gpui_kit::point(
        pane.left() + gpui_kit::px(8.),
        pane.bottom() - gpui_kit::px(8.),
    );
    VisualTestContext::from_window(opened.0, cx).simulate_mouse_down(
        at,
        gpui_kit::MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    assert_eq!(
        open_state(cx, &opened),
        (false, false),
        "a mouse-down closes it"
    );
}

/// VM6: switching to a tab without the View pill closes the open menu; it stays closed on return.
#[gpui_kit::test]
fn switching_tabs_closes_the_view_menu(cx: &mut TestAppContext) {
    let (opened, _dirs) = on_compare(cx, "view-tabs", true);
    cx.simulate_keystrokes(opened.0, "ctrl-t");
    assert_eq!(opened.1.read_with(cx, |s, _| s.active()), 1, "a start tab");
    cx.simulate_keystrokes(opened.0, "alt-v");
    assert_eq!(open_state(cx, &opened), (false, true), "alt-v opens it");
    cx.simulate_keystrokes(opened.0, "ctrl-tab");
    assert_eq!(opened.1.read_with(cx, |s, _| s.active()), 0, "the file tab");
    assert_eq!(
        open_state(cx, &opened),
        (false, false),
        "the switch closes it"
    );
    cx.simulate_keystrokes(opened.0, "ctrl-tab");
    assert_eq!(opened.1.read_with(cx, |s, _| s.active()), 1);
    assert_eq!(
        open_state(cx, &opened),
        (false, false),
        "it does not come back"
    );
}
