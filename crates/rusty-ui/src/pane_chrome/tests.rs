use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::AppContext as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    Context, IntoElement, ParentElement, Render, Role, Styled, TestAppContext, Window, div, px,
};
use rusty_core::{Command, Location};

use super::collapse::{Widths, hidden_segments};
use super::{
    DriveIcon, DriveInfo, PaneTabs, PathBar, card_shadow, current_drive, drive_for_key,
    path_segments,
};
use crate::test_window::open;
use crate::tokens::{Mode, Theme};

fn location(text: &str) -> Location {
    Location::new(text).expect("valid location")
}

fn labels(text: &str) -> Vec<String> {
    path_segments(&location(text))
        .into_iter()
        .map(|segment| segment.label.to_string())
        .collect()
}

#[test]
fn unix_paths_split_into_root_and_folders() {
    assert_eq!(labels("/home/admin/work"), ["/", "home", "admin", "work"]);
    assert_eq!(labels("/"), ["/"]);
}

#[test]
fn windows_paths_start_at_the_drive() {
    assert_eq!(labels(r"D:\work\commander"), ["D:", "work", "commander"]);
}

#[test]
fn each_segment_points_at_its_own_folder() {
    let segments = path_segments(&location("/home/admin/work"));
    let targets: Vec<&str> = segments.iter().map(|s| s.location.as_str()).collect();
    assert_eq!(targets, ["/", "/home", "/home/admin", "/home/admin/work"]);
}

#[derive(Default)]
struct Chrome {
    commands: Rc<RefCell<Vec<Command>>>,
    visited: Rc<RefCell<Vec<Location>>>,
}

impl Render for Chrome {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let commands = self.commands.clone();
        let visited = self.visited.clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                PaneTabs::new("tabs", ["commander", "core", "Downloads"], 1)
                    .on_command(move |command, _, _| commands.borrow_mut().push(command.clone())),
            )
            .child(
                PathBar::new("path", location("/home/admin/work"))
                    .on_navigate(move |target, _, _| visited.borrow_mut().push(target.clone())),
            )
    }
}

#[gpui_kit::test]
fn a_tab_click_activates_that_tab(cx: &mut TestAppContext) {
    let (window, chrome) = open(cx, |_, _| Chrome::default());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.within("tabs").find(1usize).selected(), Some(true));
        window.within("tabs").click(2usize, cx);
    })
    .expect("window");
    let commands = chrome.read_with(cx, |chrome, _| chrome.commands.borrow().clone());
    assert_eq!(commands, [Command::PaneActivateTab { tab: 2 }]);
}

#[gpui_kit::test]
fn a_path_segment_click_navigates_to_its_folder(cx: &mut TestAppContext) {
    let (window, chrome) = open(cx, |_, _| Chrome::default());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("path").role(), Some(Role::Navigation));
        let path = window.within("path");
        assert_eq!(path.find(("segment", 2usize)).label(), Some("admin"));
        window.within("path").click(("segment", 1usize), cx);
    })
    .expect("window");
    let visited = chrome.read_with(cx, |chrome, _| chrome.visited.borrow().clone());
    assert_eq!(visited, [location("/home")]);
}

fn drive(letter: Option<char>, root: &str, space: Option<(u64, u64)>) -> DriveInfo {
    DriveInfo {
        root: location(root),
        letter,
        icon: DriveIcon::Fixed,
        label: None,
        remote: None,
        connected: true,
        space,
    }
}

#[test]
fn drives_are_named_by_letter_then_label_then_root() {
    let mount = DriveInfo {
        label: Some("data".to_owned()),
        ..drive(None, "/mnt/data", None)
    };
    assert_eq!(drive(Some('C'), r"C:\", None).name(), "C");
    assert_eq!(mount.name(), "data");
    assert_eq!(drive(None, "/", None).name(), "/");
}

#[test]
fn usage_turns_to_a_warning_above_ninety_percent() {
    let gib = 1 << 30;
    let roomy = drive(Some('C'), r"C:\", Some((100 * gib, 65 * gib)));
    let full = drive(Some('D'), r"D:\", Some((100 * gib, 7 * gib)));
    assert!((roomy.used().expect("space") - 0.35).abs() < 0.001);
    assert!(!roomy.is_nearly_full());
    assert!(full.is_nearly_full());
    assert_eq!(drive(Some('E'), r"E:\", None).used(), None);
    assert_eq!(
        roomy.free_text().as_deref(),
        Some("65.0 GB free of 100.0 GB")
    );
}

#[test]
fn a_disconnected_drive_names_its_share() {
    let share = DriveInfo {
        icon: DriveIcon::Network,
        remote: Some(r"\\nas\share".to_owned()),
        connected: false,
        ..drive(Some('Z'), r"Z:\", None)
    };
    assert_eq!(share.status(), r"Disconnected · \\nas\share");
    assert_eq!(
        share.tooltip(),
        r"Network drive · \\nas\share · Disconnected"
    );
}

#[test]
fn the_current_drive_holds_the_location_and_letters_pick_drives() {
    let drives = [
        drive(Some('C'), r"C:\", None),
        drive(Some('D'), r"D:\", None),
    ];
    assert_eq!(current_drive(&drives, &location(r"d:\work\x")), Some(1));
    assert_eq!(current_drive(&drives, &location(r"E:\")), None);
    assert_eq!(drive_for_key(&drives, "d"), Some(1));
    assert_eq!(drive_for_key(&drives, "x"), None);
    assert_eq!(drive_for_key(&drives, "f1"), None);
}

#[test]
fn the_deepest_mount_holding_the_location_is_the_current_drive() {
    let drives = [
        drive(None, "/", None),
        drive(None, "/mnt/data", None),
        drive(None, "/mnt/dat", None),
    ];
    assert_eq!(
        current_drive(&drives, &location("/mnt/data/films")),
        Some(1)
    );
    assert_eq!(current_drive(&drives, &location("/mnt/data")), Some(1));
    assert_eq!(current_drive(&drives, &location("/mnt/database")), Some(0));
    assert_eq!(current_drive(&drives, &location("/home/admin")), Some(0));
}

#[test]
fn only_the_active_card_wears_the_accent_ring() {
    for mode in [Mode::Light, Mode::Dark] {
        let t = Theme::new(mode);
        let ring = t.surfaces.card_ring;
        let active = card_shadow(&t, true);
        let idle = card_shadow(&t, false);
        assert_eq!(active.last().map(|layer| layer.color), Some(ring));
        assert!(idle.iter().all(|layer| layer.color != ring), "{mode:?}");
        assert_eq!(
            idle.last().map(|layer| layer.color),
            Some(t.surfaces.card_edge)
        );
    }
}

fn hidden(crumbs: &[f32], available: f32) -> std::ops::Range<usize> {
    let widths = Widths {
        crumbs,
        separator: 10.,
        ellipsis: 20.,
    };
    hidden_segments(&widths, available)
}

#[test]
fn a_path_that_fits_collapses_nothing() {
    assert_eq!(hidden(&[20., 40., 40., 60.], 400.), 0..0);
    assert_eq!(hidden(&[20., 40.], 10.), 0..0);
}

#[test]
fn middle_segments_collapse_until_the_rest_fits() {
    let crumbs = [20., 50., 50., 50., 60.];
    assert_eq!(hidden(&crumbs, 250.), 1..2);
    assert_eq!(hidden(&crumbs, 200.), 1..3);
    assert_eq!(hidden(&crumbs, 150.), 1..4);
}

#[test]
fn the_root_and_the_last_segment_stay_even_when_nothing_fits() {
    let crumbs = [20., 50., 50., 50., 500.];
    let range = hidden(&crumbs, 100.);
    assert_eq!(range, 1..4);
    assert!(!range.contains(&0) && !range.contains(&4));
}

struct Narrow {
    visited: Rc<RefCell<Vec<Location>>>,
    path: &'static str,
}

impl Render for Narrow {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let visited = self.visited.clone();
        div().w(px(260.)).child(
            PathBar::new("path", location(self.path))
                .on_navigate(move |target, _, _| visited.borrow_mut().push(target.clone())),
        )
    }
}

const DEEP: &str =
    "/home/admin/vehicle-calibration/powertrain-control/endurance-campaign/thermal-soak";

fn frames(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle) {
    for _ in 0..3 {
        cx.update_window(window, |_, window, cx| window.render_frame(cx))
            .expect("window");
    }
}

#[gpui_kit::test]
fn a_long_path_shows_the_root_a_menu_crumb_and_the_last_segment(cx: &mut TestAppContext) {
    let visited = Rc::default();
    let (window, _) = open(cx, |_, _| Narrow {
        visited,
        path: DEEP,
    });
    frames(cx, window);
    let last = path_segments(&location(DEEP)).len() - 1;
    cx.update_window(window, |_, window, _| {
        let path = window.within("path");
        assert!(path.try_find(("segment", 0usize)).is_some());
        assert!(path.try_find("path-more").is_some());
        assert!(path.try_find(("segment", 1usize)).is_none());
        assert!(path.try_find(("segment", last)).is_some());
    })
    .expect("window");
}

#[gpui_kit::test]
fn choosing_a_hidden_folder_in_the_menu_navigates_there(cx: &mut TestAppContext) {
    let visited: Rc<RefCell<Vec<Location>>> = Rc::default();
    let (window, _) = open(cx, |_, _| Narrow {
        visited: visited.clone(),
        path: DEEP,
    });
    frames(cx, window);
    cx.update_window(window, |_, window, cx| {
        window.within("path").click("path-more", cx);
    })
    .expect("window");
    frames(cx, window);
    cx.update_window(window, |_, window, cx| {
        window.within("path").click(("path-hidden", 1usize), cx);
    })
    .expect("window");
    assert_eq!(*visited.borrow(), [location("/home")]);
    frames(cx, window);
    cx.update_window(window, |_, window, _| {
        assert!(
            window
                .within("path")
                .try_find(("path-hidden", 1usize))
                .is_none()
        );
    })
    .expect("window");
}

#[gpui_kit::test]
fn a_short_path_has_no_menu_crumb(cx: &mut TestAppContext) {
    let (window, _) = open(cx, |_, _| Narrow {
        visited: Rc::default(),
        path: "/home/admin",
    });
    frames(cx, window);
    cx.update_window(window, |_, window, _| {
        let path = window.within("path");
        assert!(path.try_find("path-more").is_none());
        assert!(path.try_find(("segment", 1usize)).is_some());
    })
    .expect("window");
}

struct Keyed {
    visited: Rc<RefCell<Vec<Location>>>,
    path: &'static str,
    has_bar: bool,
    list: gpui_kit::FocusHandle,
}

impl Render for Keyed {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        use gpui_kit::InteractiveElement as _;
        use gpui_kit::base::TestSupportExt as _;
        let visited = self.visited.clone();
        let bar = self.has_bar.then(|| {
            PathBar::new("path", location(self.path))
                .on_navigate(move |target, _, _| visited.borrow_mut().push(target.clone()))
        });
        div().w(px(260.)).children(bar).child(
            div()
                .id("list")
                .test_support()
                .track_focus(&self.list)
                .size(px(100.)),
        )
    }
}

fn open_keyed(
    cx: &mut TestAppContext,
    has_bar: bool,
) -> (
    gpui_kit::AnyWindowHandle,
    gpui_kit::Entity<Keyed>,
    Rc<RefCell<Vec<Location>>>,
) {
    cx.update(crate::primitives::popup_menu::bind_keys);
    let visited: Rc<RefCell<Vec<Location>>> = Rc::default();
    let shared = visited.clone();
    let (window, view) = open(cx, move |_, cx| Keyed {
        visited: shared,
        path: DEEP,
        has_bar,
        list: cx.focus_handle(),
    });
    cx.update_window(window, |_, window, cx| {
        let list = view.read(cx).list.clone();
        window.focus(&list, cx);
    })
    .expect("window");
    frames(cx, window);
    (window, view, visited)
}

fn is_menu_open(window: &mut Window) -> bool {
    window.within("path").try_find("path-hidden-menu").is_some()
}

#[gpui_kit::test]
fn the_path_menu_opens_focused_and_arrows_and_enter_navigate(cx: &mut TestAppContext) {
    let (window, _, visited) = open_keyed(cx, true);
    cx.update_window(window, |_, window, cx| {
        window.within("path").click("path-more", cx);
        let path = window.within("path");
        assert_eq!(path.find("path-hidden-menu").focused(), Some(true));
        assert_eq!(path.find(("path-hidden", 1usize)).selected(), Some(true));
        window.press("down", cx);
        window.press("down", cx);
        window.press("up", cx);
        assert_eq!(
            window
                .within("path")
                .find(("path-hidden", 2usize))
                .selected(),
            Some(true)
        );
        window.press("enter", cx);
        assert!(!is_menu_open(window));
        assert_eq!(window.find("list").focused(), Some(true));
    })
    .expect("window");
    assert_eq!(*visited.borrow(), [location("/home/admin")]);
}

#[gpui_kit::test]
fn escape_closes_the_path_menu_and_returns_focus(cx: &mut TestAppContext) {
    let (window, _, visited) = open_keyed(cx, true);
    cx.update_window(window, |_, window, cx| {
        window.within("path").click("path-more", cx);
        window.press("escape", cx);
        assert!(!is_menu_open(window));
        assert_eq!(window.find("list").focused(), Some(true));
    })
    .expect("window");
    assert!(visited.borrow().is_empty());
}

#[gpui_kit::test]
fn clicking_the_ellipsis_again_closes_the_path_menu(cx: &mut TestAppContext) {
    let (window, _, _) = open_keyed(cx, true);
    cx.update_window(window, |_, window, cx| {
        window.within("path").click("path-more", cx);
        assert!(is_menu_open(window));
        window.within("path").click("path-more", cx);
        window.render_frame(cx);
        assert!(!is_menu_open(window));
        assert_eq!(window.find("list").focused(), Some(true));
        window.within("path").click("path-more", cx);
        assert!(is_menu_open(window));
    })
    .expect("window");
}

#[gpui_kit::test]
fn a_new_path_bar_never_shows_the_long_path_uncollapsed(cx: &mut TestAppContext) {
    let (window, view, _) = open_keyed(cx, false);
    view.update(cx, |view, cx| {
        view.has_bar = true;
        cx.notify();
    });
    for _ in 0..3 {
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert!(
                window
                    .within("path")
                    .try_find(("segment", 1usize))
                    .is_none()
            );
        })
        .expect("window");
    }
    cx.update_window(window, |_, window, _| {
        assert!(window.within("path").try_find("path-more").is_some());
    })
    .expect("window");
}

#[gpui_kit::test]
fn a_path_change_is_collapsed_in_its_first_frame(cx: &mut TestAppContext) {
    let (window, view, _) = open_keyed(cx, true);
    view.update(cx, |view, cx| {
        view.path = "/home/admin";
        cx.notify();
    });
    frames(cx, window);
    view.update(cx, |view, cx| {
        view.path = DEEP;
        cx.notify();
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let path = window.within("path");
        assert!(path.try_find(("segment", 1usize)).is_none());
        assert!(path.try_find("path-more").is_some());
    })
    .expect("window");
}

#[gpui_kit::test]
fn the_path_menu_opens_just_below_the_ellipsis(cx: &mut TestAppContext) {
    let (window, _, _) = open_keyed(cx, true);
    cx.update_window(window, |_, window, cx| {
        window.within("path").click("path-more", cx);
        let path = window.within("path");
        let crumb = path.find("path-more").bounds();
        let menu = path.find("path-hidden-menu").bounds();
        assert!(
            menu.top() >= crumb.bottom() && menu.top() <= crumb.bottom() + px(12.),
            "menu top {:?}, crumb bottom {:?}",
            menu.top(),
            crumb.bottom()
        );
        assert!((menu.left() - crumb.left()).abs() <= px(8.));
    })
    .expect("window");
}

mod markers;
