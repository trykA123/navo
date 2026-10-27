use std::sync::Arc;
use std::time::Duration;

use gpui_kit::{AnyWeakView, TestAppContext};
use rusty_fs::FakeFs;

use super::{Rig, open_on};
use crate::window_view::FerriteWindow;
use crate::window_view::preview::Shown;
use crate::window_view::tests::held::{
    Disk, assert_released, cursor_to, gpu_images, held_in, samples,
};
use crate::window_view::viewer::PREVIEW_BODY;

fn rig(cx: &mut TestAppContext) -> (Rig, Disk) {
    let fs = FakeFs::new();
    let files = samples();
    for (name, bytes) in &files {
        fs.add_file(&format!("home/{name}"), bytes.len() as u64);
    }
    let fs = Arc::new(fs);
    let disk = Disk::new(&files);
    let rig = open_on(cx, &fs, fs.clone(), disk.hooks());
    rig.open(cx);
    (rig, disk)
}

fn body_of(view: &FerriteWindow, name: &str) -> Option<AnyWeakView> {
    match view.preview.shown()? {
        Shown::File(file) if file.target.name() == name => {
            Some(file.body.as_ref()?.view().downgrade())
        }
        _ => None,
    }
}

fn show(cx: &mut TestAppContext, rig: &Rig, name: &str) -> AnyWeakView {
    cursor_to(cx, (rig.window, &rig.view), name);
    rig.rest(cx);
    assert!(
        rig.wait_until(cx, |view| body_of(view, name).is_some()),
        "{name}"
    );
    rig.settle_motion(cx);
    rig.read(cx, |view| body_of(view, name)).expect("a body")
}

fn close(cx: &mut TestAppContext, rig: &Rig) {
    rig.key(cx, "escape");
    rig.settle_motion(cx);
    cx.executor().advance_clock(Duration::from_secs(1));
    rig.rest(cx);
    assert!(!rig.read(cx, |view| view.preview.is_open()));
}

#[gpui_kit::test]
fn every_kind_left_in_the_preview_holds_no_view_file_or_gpu_image(cx: &mut TestAppContext) {
    let (rig, disk) = rig(cx);
    let mut previous: Option<(&str, AnyWeakView)> = None;
    for (name, _) in samples() {
        let weak = show(cx, &rig, name);
        if let Some((left, before)) = previous.replace((name, weak)) {
            cx.run_until_parked();
            assert!(before.upgrade().is_none(), "{left} is still alive");
        }
        assert_eq!(held_in(cx, &PREVIEW_BODY).items, 1, "{name}");
    }
    let (name, last) = previous.expect("a file was shown");
    close(cx, &rig);
    assert_eq!(held_in(cx, &PREVIEW_BODY).items, 0);
    assert_released(cx, name, (&last, &disk));
}

#[gpui_kit::test]
fn closing_the_preview_on_each_kind_releases_it(cx: &mut TestAppContext) {
    let (rig, disk) = rig(cx);
    for (name, _) in samples() {
        let weak = show(cx, &rig, name);
        if matches!(name, "manual.pdf" | "photo.png" | "sign.svg") {
            assert!(gpu_images(cx).items > 0, "{name}");
        }
        close(cx, &rig);
        assert_eq!(held_in(cx, &PREVIEW_BODY).items, 0, "{name}");
        assert_released(cx, name, (&weak, &disk));
        rig.open(cx);
    }
}
