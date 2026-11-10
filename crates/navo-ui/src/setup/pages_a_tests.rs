//! Window tests for the Welcome, Licence and Folder pages: names, states and emitted actions.

use std::{cell::RefCell, rc::Rc};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Context, IntoElement, Point, Render, Role,
    TestAppContext, Window, WindowBounds, WindowOptions, px, size,
};

use super::{OnAction, SetupAction, SetupCheck, SetupPage, SetupView, Strings, render_page};
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
        render_page(&self.view, &Strings::load(), on_action)
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

fn folder_view(locked: bool) -> SetupView {
    SetupView {
        page: SetupPage::Folder,
        folder_text: "C:\\Navo".into(),
        folder_locked: locked,
        problem: Some("Pick another folder.".into()),
        space_needed: Some(120 << 20),
        space_free: Some(5 << 30),
        ..SetupView::default()
    }
}

#[gpui_kit::test]
fn folder_page_names_its_controls_and_browse_sends_an_action(cx: &mut TestAppContext) {
    let (window, sent) = open(cx, folder_view(false));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let field = window.find("folder-field");
        assert_eq!(field.role(), Some(Role::TextInput));
        assert_eq!(field.value(), Some("C:\\Navo"));
        let problem = window.find("folder-problem");
        assert_eq!(problem.label(), Some("Pick another folder."));
        assert_eq!(window.find("folder-browse").label(), Some("Browse…"));
        window.click("folder-browse", cx);
    })
    .expect("window");
    assert_eq!(*sent.borrow(), [SetupAction::Browse]);
}

#[gpui_kit::test]
fn a_locked_folder_turns_browse_off(cx: &mut TestAppContext) {
    let (window, sent) = open(cx, folder_view(true));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("folder-browse", cx);
    })
    .expect("window");
    assert!(sent.borrow().is_empty());
}

#[gpui_kit::test]
fn licence_accept_box_shows_its_state_and_sends_a_toggle(cx: &mut TestAppContext) {
    let view = SetupView {
        page: SetupPage::Licence,
        licence_accepted: true,
        ..SetupView::default()
    };
    let (window, sent) = open(cx, view);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let accept = window.find("licence-accept");
        assert_eq!(accept.role(), Some(Role::CheckBox));
        assert_eq!(accept.label(), Some("I accept the licence"));
        assert_eq!(accept.checked(), Some(true));
        assert_eq!(window.find("licence-apache").role(), Some(Role::Link));
        window.click("licence-accept", cx);
    })
    .expect("window");
    assert_eq!(
        *sent.borrow(),
        [SetupAction::Toggle(SetupCheck::AcceptLicence)]
    );
}

#[gpui_kit::test]
fn welcome_names_the_version_to_install(cx: &mut TestAppContext) {
    let view = SetupView {
        page: SetupPage::Welcome,
        version: "0.7.0".into(),
        installed_version: Some("0.6.0".into()),
        folder_text: "C:\\Navo".into(),
        ..SetupView::default()
    };
    let (window, _) = open(cx, view);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let heading = window.find("setup-heading");
        assert_eq!(heading.label(), Some("Install Navo 0.7.0"));
    })
    .expect("window");
}
