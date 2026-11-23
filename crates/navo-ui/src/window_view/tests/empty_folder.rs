//! The empty-folder line shows for a finished listing only: not while a folder loads or reloads.

use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt;
use navo_core::PaneId;

use super::drive_fs::{Rig, local};
use super::offline::{enter_slow, stalled};

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
