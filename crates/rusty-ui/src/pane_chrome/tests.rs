use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::AppContext as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    Context, IntoElement, ParentElement, Render, Role, Styled, TestAppContext, Window, div,
};
use rusty_core::{Command, Location};

use super::{PaneTabs, PathBar, path_segments};
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
