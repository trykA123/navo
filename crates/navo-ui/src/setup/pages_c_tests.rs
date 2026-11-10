//! Window tests for the Blocked, Same, Newer, Ambiguous, Elevated and NoPayload pages.

use std::{cell::RefCell, rc::Rc};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Context, IntoElement, Point, Render, Role,
    TestAppContext, Window, WindowBounds, WindowOptions, px, size,
};

use super::{OnAction, SetupAction, SetupPage, SetupView, Strings, access_keys, render_page};
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

fn view(page: SetupPage) -> SetupView {
    SetupView {
        page,
        installed_version: Some("0.4.1".into()),
        folder_text: "C:\\Users\\admin\\Navo".into(),
        ..SetupView::default()
    }
}

#[gpui_kit::test]
fn same_offers_open_navo_and_close(cx: &mut TestAppContext) {
    let (window, sent) = open(cx, view(SetupPage::Same));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let node = window.find("same-text");
        let text = node.label().expect("text");
        assert!(
            text.contains("0.4.1") && text.contains("C:\\Users\\admin\\Navo"),
            "{text}"
        );
        assert_eq!(window.find("same-open").label(), Some("Open Navo"));
        assert_eq!(window.find("same-close").label(), Some("Close"));
        window.click("same-open", cx);
        window.click("same-close", cx);
    })
    .expect("window");
    assert_eq!(*sent.borrow(), [SetupAction::Primary, SetupAction::Close]);
}

#[gpui_kit::test]
fn newer_names_the_version_and_path_and_offers_only_close(cx: &mut TestAppContext) {
    let (window, sent) = open(cx, view(SetupPage::Newer));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("newer-text").label(),
            Some("A newer Navo 0.4.1 is installed in C:\\Users\\admin\\Navo.")
        );
        assert!(window.try_find("same-open").is_none());
        assert!(window.try_find("newer-open").is_none());
        window.click("newer-close", cx);
    })
    .expect("window");
    assert_eq!(*sent.borrow(), [SetupAction::Close]);
}

#[gpui_kit::test]
fn ambiguous_says_repair_by_uninstalling_first_then_close(cx: &mut TestAppContext) {
    let (window, sent) = open(cx, view(SetupPage::Ambiguous));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let text = window.find("ambiguous-text");
        assert_eq!(text.role(), Some(Role::Alert));
        assert!(
            text.label()
                .expect("text")
                .starts_with("Repair by uninstalling first")
        );
        window.click("ambiguous-close", cx);
    })
    .expect("window");
    assert_eq!(*sent.borrow(), [SetupAction::Close]);
}

#[gpui_kit::test]
fn ambiguous_unsafe_name_shows_its_own_text_and_never_says_uninstall(cx: &mut TestAppContext) {
    let own = "Navo is installed in C:\\R&D\\Navo. Setup cannot update a folder whose name contains &. Keep using Navo there; it updates itself.";
    let unsafe_name = SetupView {
        notice: Some(own.into()),
        ..view(SetupPage::Ambiguous)
    };
    let (window, _) = open(cx, unsafe_name);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let node = window.find("ambiguous-text");
        let label = node.label().expect("text");
        assert_eq!(label, own);
        assert!(!label.to_lowercase().contains("uninstall"), "{label}");
        assert!(window.try_find("ambiguous-close").is_some());
    })
    .expect("window");
}

#[gpui_kit::test]
fn elevated_and_no_payload_offer_only_close(cx: &mut TestAppContext) {
    for (page, text, close) in [
        (SetupPage::Elevated, "elevated-text", "elevated-close"),
        (SetupPage::NoPayload, "nopayload-text", "nopayload-close"),
    ] {
        let (window, sent) = open(cx, view(page));
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find(text).role(), Some(Role::Alert));
            assert_eq!(window.find(close).label(), Some("Close"));
            window.click(close, cx);
        })
        .expect("window");
        assert_eq!(*sent.borrow(), [SetupAction::Close], "{page:?}");
    }
}

#[gpui_kit::test]
fn blocked_names_the_folder_and_retry_follows_the_gate(cx: &mut TestAppContext) {
    let blocked = SetupView {
        can_retry: true,
        ..view(SetupPage::Blocked)
    };
    let (window, sent) = open(cx, blocked);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("setup-heading").label(),
            Some("Close Navo to continue")
        );
        let text = window.find("blocked-text");
        assert_eq!(text.role(), Some(Role::Alert));
        let label = text.label().expect("text");
        assert!(
            label.contains("C:\\Users\\admin\\Navo") && label.contains("Try again"),
            "{label}"
        );
        assert_eq!(window.find("blocked-retry").label(), Some("Try again"));
        window.click("blocked-retry", cx);
        window.click("blocked-close", cx);
    })
    .expect("window");
    assert_eq!(*sent.borrow(), [SetupAction::TryAgain, SetupAction::Close]);
    let (window, sent) = open(cx, view(SetupPage::Blocked));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("blocked-retry", cx);
    })
    .expect("window");
    assert!(sent.borrow().is_empty());
}

#[test]
fn access_keys_list_the_buttons_of_each_page_without_a_clash() {
    let strings = Strings::english();
    let keys = |page, can_retry| {
        access_keys(
            &SetupView {
                can_retry,
                ..view(page)
            },
            &strings,
        )
    };
    assert_eq!(
        keys(SetupPage::Same, false),
        [('o', SetupAction::Primary), ('c', SetupAction::Close)]
    );
    assert_eq!(keys(SetupPage::Newer, false), [('c', SetupAction::Close)]);
    assert_eq!(
        keys(SetupPage::Elevated, false),
        [('c', SetupAction::Close)]
    );
    assert_eq!(
        keys(SetupPage::Blocked, true),
        [('t', SetupAction::TryAgain), ('c', SetupAction::Close)]
    );
    assert_eq!(keys(SetupPage::Blocked, false), [('c', SetupAction::Close)]);
}
