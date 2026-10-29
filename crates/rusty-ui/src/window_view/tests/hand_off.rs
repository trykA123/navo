use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext, VisualTestContext};
use rusty_core::Location;
use rusty_fs::FakeFs;

use super::{fake, open_options, options_for, settle, two_panes};
use crate::window_view::{FerriteWindow, OpenHooks, OpenRequest, OpenSender, open_requests};

struct Rig {
    window: AnyWindowHandle,
    view: Entity<FerriteWindow>,
    fs: Arc<FakeFs>,
    sender: OpenSender,
}

fn rig_starting_at(cx: &mut TestAppContext, path: Option<&str>) -> Rig {
    let fs = fake();
    fs.add_file("home/docs/zeta.txt", 5);
    let (sender, inbox) = open_requests();
    let open = OpenHooks {
        path: path.map(|path| fs.location(path)),
        inbox: Some(inbox),
        on_shown: None,
        window_shown: None,
    };
    let options = crate::window_view::StartOptions {
        open,
        ..options_for(fs.clone(), two_panes(&fs))
    };
    let (window, view) = open_options(cx, options);
    settle(cx, &view);
    Rig {
        window,
        view,
        fs,
        sender,
    }
}

fn rig(cx: &mut TestAppContext) -> Rig {
    rig_starting_at(cx, None)
}

impl Rig {
    fn at(&self, path: &str) -> Location {
        self.fs.location(path)
    }

    fn send(&self, request: OpenRequest) {
        assert!(self.sender.send(request), "the window takes requests");
    }

    fn open(&self, cx: &mut TestAppContext, path: &str) {
        self.send(OpenRequest {
            path: Some(self.at(path)),
            ..OpenRequest::default()
        });
        settle(cx, &self.view);
    }

    fn active_tabs(&self, cx: &mut TestAppContext) -> (usize, Location) {
        self.view.read_with(cx, |view, _| {
            let state = view.browser.state();
            let pane = state.pane(state.focused_pane()).expect("focused pane");
            (pane.tabs().len(), pane.active().location().clone())
        })
    }

    fn pane_folders(&self, cx: &mut TestAppContext) -> Vec<Location> {
        self.view.read_with(cx, |view, _| {
            let state = view.browser.state();
            view.panes()
                .into_iter()
                .filter_map(|pane| state.pane(pane))
                .map(|pane| pane.active().location().clone())
                .collect()
        })
    }

    fn cursor_name(&self, cx: &mut TestAppContext) -> Option<String> {
        self.view.read_with(cx, |view, _| {
            let tab = view.browser.state().active_tab();
            tab.listing()
                .and_then(|l| l.get(tab.view().cursor))
                .map(|e| e.name.clone())
        })
    }

    fn notice(&self, cx: &mut TestAppContext) -> Option<String> {
        self.view.read_with(cx, |view, _| {
            view.status_notice().map(|notice| notice.text.to_string())
        })
    }

    fn not_found(&self, path: &str) -> String {
        format!("“{}” was not found", self.at(path).path().display())
    }
}

#[gpui_kit::test]
fn a_folder_request_opens_a_new_tab_there_in_the_active_pane(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let (tabs, _) = rig.active_tabs(cx);
    rig.open(cx, "home/docs");
    assert_eq!(rig.active_tabs(cx), (tabs + 1, rig.at("home/docs")));
}

#[gpui_kit::test]
fn a_file_request_opens_a_tab_on_its_folder_with_the_cursor_on_the_file(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let (tabs, _) = rig.active_tabs(cx);
    rig.open(cx, "home/docs/zeta.txt");
    assert_eq!(rig.active_tabs(cx), (tabs + 1, rig.at("home/docs")));
    assert_eq!(rig.cursor_name(cx).as_deref(), Some("zeta.txt"));
}

#[gpui_kit::test]
fn a_missing_path_shows_the_notice_and_changes_no_tab(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let before = rig.active_tabs(cx);
    rig.open(cx, "home/nowhere");
    assert_eq!(rig.active_tabs(cx), before);
    assert_eq!(rig.notice(cx), Some(rig.not_found("home/nowhere")));
}

#[gpui_kit::test]
fn left_and_right_requests_navigate_those_panes(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.send(OpenRequest {
        left: Some(rig.at("dest")),
        right: Some(rig.at("home/docs")),
        ..OpenRequest::default()
    });
    settle(cx, &rig.view);
    assert_eq!(
        rig.pane_folders(cx),
        vec![rig.at("dest"), rig.at("home/docs")]
    );
}

#[gpui_kit::test]
fn two_requests_in_a_row_both_apply(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let (tabs, _) = rig.active_tabs(cx);
    rig.send(OpenRequest {
        path: Some(rig.at("home/docs")),
        ..OpenRequest::default()
    });
    rig.open(cx, "dest");
    assert_eq!(rig.active_tabs(cx), (tabs + 2, rig.at("dest")));
}

#[gpui_kit::test]
fn a_view_request_opens_the_viewer(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.send(OpenRequest {
        view: Some(rig.at("home/a.txt")),
        ..OpenRequest::default()
    });
    settle(cx, &rig.view);
    assert!(rig.view.read_with(cx, |view, _| view.viewer.is_some()));
}

#[gpui_kit::test]
fn a_request_brings_the_window_forward(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let mut visual = VisualTestContext::from_window(rig.window, cx);
    visual.deactivate_window();
    assert!(!visual.update(|window, _| window.is_window_active()));
    rig.send(OpenRequest::default());
    settle(cx, &rig.view);
    let mut visual = VisualTestContext::from_window(rig.window, cx);
    assert!(visual.update(|window, _| window.is_window_active()));
}

#[gpui_kit::test]
fn a_start_path_on_a_file_opens_a_tab_with_the_cursor_on_it(cx: &mut TestAppContext) {
    let rig = rig_starting_at(cx, Some("home/docs/zeta.txt"));
    assert_eq!(rig.active_tabs(cx), (2, rig.at("home/docs")));
    assert_eq!(rig.cursor_name(cx).as_deref(), Some("zeta.txt"));
}

#[gpui_kit::test]
fn a_missing_start_path_opens_the_window_with_the_notice(cx: &mut TestAppContext) {
    let rig = rig_starting_at(cx, Some("home/nowhere"));
    assert_eq!(rig.active_tabs(cx), (1, rig.at("home")));
    assert_eq!(rig.notice(cx), Some(rig.not_found("home/nowhere")));
}

fn next_frame(cx: &mut TestAppContext, window: AnyWindowHandle) {
    cx.update_window(window, |_, window, cx| {
        window.simulate_next_frame(cx);
    })
    .expect("window");
    cx.run_until_parked();
}

#[gpui_kit::test]
fn the_shown_hook_runs_once_after_the_first_frame_is_drawn(cx: &mut TestAppContext) {
    let fs = fake();
    let shown = Rc::new(Cell::new(0));
    let count = shown.clone();
    let options = crate::window_view::StartOptions {
        open: OpenHooks {
            on_shown: Some(Box::new(move || count.set(count.get() + 1))),
            ..OpenHooks::none()
        },
        ..options_for(fs.clone(), two_panes(&fs))
    };
    let (window, _view) = open_options(cx, options);
    next_frame(cx, window);
    assert_eq!(shown.get(), 0, "the first frame is still being drawn");
    next_frame(cx, window);
    next_frame(cx, window);
    assert_eq!(shown.get(), 1);
}
