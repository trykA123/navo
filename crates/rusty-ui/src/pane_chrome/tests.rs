use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::AppContext as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    Context, IntoElement, ParentElement, Render, Role, Styled, TestAppContext, Window, div,
};
use rusty_core::{Command, Location};

use super::{DriveIcon, DriveInfo, PaneTabs, PathBar, current_drive, drive_for_key, path_segments};
use crate::test_window::open;

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
                    .active(true)
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
