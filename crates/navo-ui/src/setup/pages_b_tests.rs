//! Window tests for the Shortcuts, Installing, Finish and Failed pages: names, states and actions.

use std::{cell::RefCell, path::PathBuf, rc::Rc, time::Duration};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Context, IntoElement, Point, Render, Role,
    TestAppContext, Window, WindowBounds, WindowOptions, px, size,
};

use super::{
    OnAction, SetupAction, SetupCheck, SetupPage, SetupProgress, SetupView, Strings, render_page,
};
use crate::tokens::{self, Mode};

type Sent = Rc<RefCell<Vec<SetupAction>>>;

struct Host {
    view: SetupView,
    sent: Sent,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let sent = self.sent.clone();
        let on_action: OnAction = Rc::new(move |action, _, _| sent.borrow_mut().push(action));
        render_page(&self.view, &Strings::english(), on_action)
    }
}

fn open(cx: &mut TestAppContext, view: SetupView) -> (AnyWindowHandle, Sent) {
    let sent = Sent::default();
    let host = Host {
        view,
        sent: sent.clone(),
    };
    let window: (AnyWindowHandle, _) = cx.update(|cx| {
        gpui_kit::init(cx);
        tokens::apply(Mode::Light, cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: Point::default(),
                size: size(px(640.), px(480.)),
            })),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |_, cx| cx.new(|_| host)).expect("open")
    });
    (window.0, sent)
}

fn installing(seconds: u64, still_working: bool) -> SetupView {
    SetupView {
        page: SetupPage::Installing,
        progress: Some(SetupProgress {
            elapsed: Duration::from_secs(seconds),
            still_working,
        }),
        log_paths: vec![PathBuf::from("C:\\Logs\\setup.log")],
        ..SetupView::default()
    }
}

fn failed(can_retry: bool, leftover: bool) -> SetupView {
    SetupView {
        page: SetupPage::Failed,
        problem: Some("The installer stopped.".into()),
        can_retry,
        leftover_root: leftover.then(|| PathBuf::from("C:\\Navo\\Root")),
        notice: Some("general advice".into()),
        folder_text: "C:\\Navo".into(),
        log_paths: vec![PathBuf::from("C:\\Logs\\setup.log")],
        details: "exit 7".into(),
        ..SetupView::default()
    }
}

#[gpui_kit::test]
fn shortcuts_boxes_show_their_state_and_send_toggles(cx: &mut TestAppContext) {
    let view = SetupView {
        page: SetupPage::Shortcuts,
        desktop: false,
        start_menu: true,
        ..SetupView::default()
    };
    let (window, sent) = open(cx, view);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let menu = window.find("shortcut-start-menu");
        assert_eq!(menu.role(), Some(Role::CheckBox));
        assert_eq!(menu.label(), Some("Start menu"));
        assert_eq!(menu.checked(), Some(true));
        let desktop = window.find("shortcut-desktop");
        assert_eq!(desktop.label(), Some("Desktop"));
        assert_eq!(desktop.checked(), Some(false));
        window.click("shortcut-desktop", cx);
    })
    .expect("window");
    assert_eq!(*sent.borrow(), [SetupAction::Toggle(SetupCheck::Desktop)]);
}

#[gpui_kit::test]
fn installing_shows_elapsed_time_tips_and_no_button(cx: &mut TestAppContext) {
    let (window, _) = open(cx, installing(42, false));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("installing-elapsed").label(),
            Some("Elapsed 0:42")
        );
        let bar = window.find("installing-bar");
        assert_eq!(bar.role(), Some(Role::ProgressIndicator));
        assert_eq!(bar.label(), Some("Installing"));
        assert!(window.try_find("installing-still").is_none());
        assert!(window.try_find("installing-tip-0").is_some());
        assert!(window.try_find("installing-tip-3").is_some());
    })
    .expect("window");
}

#[gpui_kit::test]
fn installing_after_three_minutes_names_the_log_and_drops_the_tips(cx: &mut TestAppContext) {
    let (window, _) = open(cx, installing(192, true));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("installing-elapsed").label(),
            Some("Elapsed 3:12")
        );
        let still = window.find("installing-still");
        assert_eq!(still.role(), Some(Role::Status));
        let label = still.label().expect("label");
        assert!(label.starts_with("Setup is still working"), "{label}");
        assert!(label.contains("C:\\Logs\\setup.log"), "{label}");
        assert!(window.try_find("installing-tip-0").is_none());
    })
    .expect("window");
}

#[gpui_kit::test]
fn finish_names_the_folder_lists_keys_and_sends_actions(cx: &mut TestAppContext) {
    let view = SetupView {
        page: SetupPage::Finish,
        folder_text: "C:\\Navo".into(),
        start_navo: true,
        ..SetupView::default()
    };
    let (window, sent) = open(cx, view);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let heading = window.find("setup-heading");
        assert_eq!(heading.label(), Some("Navo is installed in C:\\Navo."));
        for (index, key) in ["Ctrl+K", "F1", "Shift+F1"].into_iter().enumerate() {
            let line = window.find(format!("finish-key-{index}"));
            assert!(line.label().expect("label").starts_with(key));
        }
        let start = window.find("finish-start");
        assert_eq!(start.role(), Some(Role::CheckBox));
        assert_eq!(start.label(), Some("Start Navo"));
        assert_eq!(start.checked(), Some(true));
        window.click("finish-start", cx);
        assert_eq!(window.find("finish-open-folder").role(), Some(Role::Link));
        window.click("finish-open-folder", cx);
    })
    .expect("window");
    assert_eq!(
        *sent.borrow(),
        [
            SetupAction::Toggle(SetupCheck::StartNavo),
            SetupAction::OpenFolder
        ]
    );
}

#[gpui_kit::test]
fn failed_shows_problem_log_and_copy_details(cx: &mut TestAppContext) {
    let (window, sent) = open(cx, failed(true, false));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("failed-problem").label(),
            Some("The installer stopped.")
        );
        assert_eq!(
            window.find("failed-log").value(),
            Some("C:\\Logs\\setup.log")
        );
        assert_eq!(window.find("failed-copy").label(), Some("Copy details"));
        assert!(window.try_find("failed-leftover").is_none());
        window.click("failed-copy", cx);
        assert_eq!(window.find("failed-retry").label(), Some("Retry"));
        window.click("failed-retry", cx);
    })
    .expect("window");
    assert_eq!(
        *sent.borrow(),
        [SetupAction::CopyDetails, SetupAction::TryAgain]
    );
}

#[gpui_kit::test]
fn failed_with_leftover_files_names_the_root_and_turns_retry_off(cx: &mut TestAppContext) {
    let (window, sent) = open(cx, failed(true, true));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let leftover = window.find("failed-leftover");
        assert_eq!(leftover.role(), Some(Role::Alert));
        let label = leftover.label().expect("label");
        assert!(label.contains("C:\\Navo\\Root"), "{label}");
        assert!(!label.contains("general advice"), "{label}");
        assert!(
            label.ends_with("Delete this folder before you run Setup again."),
            "{label}"
        );
        window.click("failed-retry", cx);
    })
    .expect("window");
    assert!(sent.borrow().is_empty());
}
