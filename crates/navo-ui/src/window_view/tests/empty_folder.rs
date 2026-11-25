//! The empty-folder line shows for a finished listing only: not while a folder loads or reloads.

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};
use navo_core::{Command, FsErrorKind, PaneId};

use super::drive_fs::{Rig, Setup, drive_window, local};
use super::offline::{enter_slow, stalled};
use super::{fake, open_options, options_for, press, two_panes};
use crate::tokens::{self, Mode};

/// True when the pane's list draws the empty-folder line this frame.
fn has_line(rig: &Rig, cx: &mut TestAppContext, pane: PaneId) -> bool {
    rig.frame(cx, |window, _| {
        window
            .within(("list", pane.get()))
            .try_find("list-empty")
            .is_some()
    })
}

#[gpui_kit::test]
fn a_reloading_empty_folder_shows_no_line_until_the_new_listing_lands(cx: &mut TestAppContext) {
    let (rig, pane) = stalled(cx);
    enter_slow(&rig, cx);
    assert!(!has_line(&rig, cx, pane), "loading shows no line");
    rig.fs.release();
    rig.settle_until(cx, |view| {
        view.browser.state().active_tab().listing().is_some() && !view.browser.loading()
    });
    assert!(has_line(&rig, cx, pane), "a finished empty folder says so");
    rig.view.update(cx, |view, cx| {
        view.browser.refresh_unwatched(&[local(r"D:\slow")]);
        cx.notify();
    });
    assert!(
        !has_line(&rig, cx, pane),
        "a reload keeps the old listing but shows no line"
    );
    rig.settle_until(cx, |view| !view.browser.loading());
    assert!(has_line(&rig, cx, pane), "the reloaded listing is finished");
}

#[gpui_kit::test]
fn dual_pane_empty_folder_renders_active_bar_and_empty_card(cx: &mut TestAppContext) {
    let fs = fake();
    fs.add_folder("home/empty");
    let state = two_panes(&fs);
    let (window, view) = open_options(cx, options_for(fs.clone(), state));
    let pane = view.read_with(cx, |view, _| view.browser.state().focused_pane());
    view.update(cx, |view, cx| {
        let _ = view.browser.command(&navo_core::Command::PaneNavigate {
            location: fs.location("home/empty"),
        });
        cx.notify();
    });
    super::settle(cx, &view);
    for mode in [Mode::Light, Mode::Dark] {
        cx.update(|cx| tokens::apply(mode, cx));
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert!(
                window
                    .within(("list", pane.get()))
                    .find("list-empty-card")
                    .visible()
            );
        })
        .expect("window");
    }
}

#[gpui_kit::test]
fn failed_listing_renders_error_card_and_tab_keeps_pane_focus(cx: &mut TestAppContext) {
    let rig = drive_window(cx, Setup::default());
    let panes = rig.panes(cx);
    assert_eq!(panes.len(), 2);
    let pane = rig.focused(cx);
    let other = panes.into_iter().find(|p| *p != pane).expect("other pane");
    rig.fs.fail_list("D/work", 1, FsErrorKind::AccessDenied);
    rig.view.update(cx, |view, cx| {
        let _ = view.browser.command(&Command::PaneOpenTab {
            location: Some(local(r"D:\work")),
        });
        cx.notify();
    });
    rig.settle_until(cx, |view| !view.browser.loading());
    rig.frame(cx, |window, _| {
        assert!(window.find(("pane-error-card", pane.get())).visible());
        assert!(window.find(("pane-error-retry", pane.get())).visible());
    });
    press(cx, rig.window, &rig.view, "tab");
    assert_eq!(rig.focused(cx), other);
}
