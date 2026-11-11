//! Review fixes for the image body (V40): the wipe follows the drawn line, a refresh frees
//! the old pictures, a failed read is named, modified keys do nothing, a size mismatch has no
//! overlay, and a changed mask is rebuilt.

use gpui_kit::{TestAppContext, px};

use super::{Rig, mask, rig, scored};
use crate::compare_diff::image_body::{ImageBody, ImageMode};
use crate::compare_diff::{DiffUpdate, ImageMask, ImageScores, ImageSize, PairKind};
use crate::window_view::compare_screen::image::Pointer;

fn mismatch() -> ImageScores {
    let size = |width, height| ImageSize { width, height };
    ImageScores::SizeMismatch {
        left: size(40, 30),
        right: size(30, 40),
    }
}

impl Rig {
    /// Sends one more Image update on request 1 and lets the screen settle.
    fn send_image(&self, cx: &mut TestAppContext, scores: ImageScores, mask: Option<ImageMask>) {
        (self.diff.pair_sink())(DiffUpdate::Image {
            request: 1,
            scores,
            mask,
        });
        self.paint(cx);
    }

    fn pointer(&self, cx: &mut TestAppContext, at: Pointer, x: f32) {
        self.view
            .update(cx, |view, cx| view.image_pointer(at, px(x), cx));
    }

    fn split(&self, cx: &mut TestAppContext) -> f32 {
        self.probe(cx).expect("a body").split
    }
}

#[gpui_kit::test]
fn compare_image_the_wipe_follows_the_pictures_not_the_stage(cx: &mut TestAppContext) {
    let rig = rig(cx, true);
    rig.open(cx, scored(), Some(mask()));
    rig.press(cx, "2");
    rig.paint(cx);
    let (width, height, left) = rig.probe(cx).expect("a body").placed.expect("a frame");
    assert_eq!(
        (width, height),
        (40., 30.),
        "the pictures, not the whole stage"
    );
    rig.pointer(cx, Pointer::Down, left + width * 0.25);
    assert!((rig.split(cx) - 0.25).abs() < 0.001, "{}", rig.split(cx));
}

#[gpui_kit::test]
fn compare_image_a_drag_released_outside_ends_on_the_next_move(cx: &mut TestAppContext) {
    let rig = rig(cx, true);
    rig.open(cx, scored(), Some(mask()));
    rig.press(cx, "2");
    rig.paint(cx);
    let (width, _, left) = rig.probe(cx).expect("a body").placed.expect("a frame");
    rig.pointer(cx, Pointer::Down, left + width * 0.25);
    rig.pointer(cx, Pointer::Move(true), left + width * 0.5);
    assert!((rig.split(cx) - 0.5).abs() < 0.001, "a held move drags");
    rig.pointer(cx, Pointer::Move(false), left + width * 0.75);
    rig.pointer(cx, Pointer::Move(true), left + width * 0.9);
    assert!(
        (rig.split(cx) - 0.5).abs() < 0.001,
        "the drag ended without a button"
    );
    rig.pointer(cx, Pointer::Down, left + width * 0.1);
    rig.pointer(cx, Pointer::Up, left + width * 0.1);
    rig.pointer(cx, Pointer::Move(true), left + width * 0.8);
    assert!((rig.split(cx) - 0.1).abs() < 0.001, "a release ends it too");
}

#[gpui_kit::test]
fn compare_image_a_refresh_gives_the_old_pictures_back(cx: &mut TestAppContext) {
    let rig = rig(cx, true);
    rig.open(cx, scored(), Some(mask()));
    assert_eq!(rig.gpu(cx), 3);
    rig.view.update(cx, |view, _| view.refresh_file_screen());
    let sink = rig.diff.pair_sink();
    sink(DiffUpdate::Opened {
        request: 2,
        kind: PairKind::Image,
        text: None,
    });
    sink(DiffUpdate::Image {
        request: 2,
        scores: scored(),
        mask: Some(mask()),
    });
    rig.paint(cx);
    assert_eq!(
        rig.gpu(cx),
        3,
        "the first load's pictures were freed, not leaked"
    );
}

#[gpui_kit::test]
fn compare_image_a_refresh_to_another_kind_or_a_failure_drops_the_body(cx: &mut TestAppContext) {
    let rig = rig(cx, true);
    rig.open(cx, scored(), Some(mask()));
    rig.view.update(cx, |view, _| view.refresh_file_screen());
    (rig.diff.pair_sink())(DiffUpdate::Opened {
        request: 2,
        kind: PairKind::Binary,
        text: None,
    });
    rig.paint(cx);
    assert!(rig.probe(cx).is_none(), "a binary pair shows no picture");
    assert_eq!(rig.gpu(cx), 0);

    rig.view.update(cx, |view, _| view.refresh_file_screen());
    (rig.diff.pair_sink())(DiffUpdate::Opened {
        request: 3,
        kind: PairKind::Image,
        text: None,
    });
    rig.paint(cx);
    assert_eq!(
        rig.gpu(cx),
        0,
        "the pair is not scored yet, so nothing is read"
    );
    (rig.diff.pair_sink())(DiffUpdate::Image {
        request: 3,
        scores: scored(),
        mask: None,
    });
    rig.paint(cx);
    assert_eq!(rig.gpu(cx), 2);
    (rig.diff.pair_sink())(DiffUpdate::Failed {
        request: 3,
        text: "gone".into(),
    });
    rig.paint(cx);
    assert!(rig.probe(cx).is_none(), "a failed pair shows its failure");
    assert_eq!(rig.gpu(cx), 0, "and holds no picture");
}

#[gpui_kit::test]
fn compare_image_a_read_that_fails_is_named(cx: &mut TestAppContext) {
    let rig = rig(cx, true);
    rig.disk.files.lock().unwrap().remove("home/a.png");
    rig.open(cx, scored(), Some(mask()));
    let probe = rig.probe(cx).expect("a body");
    assert_eq!(probe.notice, Some("compare-image-note-unreadable"));
    assert_eq!(probe.ready, 0, "neither side could be read");
}

#[gpui_kit::test]
fn compare_image_modified_keys_leave_the_body_alone(cx: &mut TestAppContext) {
    let rig = rig(cx, true);
    rig.open(cx, scored(), Some(mask()));
    rig.press(cx, "ctrl-2");
    assert_eq!(rig.probe(cx).expect("a body").mode, ImageMode::SideBySide);
    rig.press(cx, "2");
    rig.press(cx, "ctrl-left");
    rig.press(cx, "alt-end");
    assert_eq!(
        rig.split(cx),
        0.5,
        "Ctrl+Left and Alt+End are not the wipe's"
    );
    rig.press(cx, "ctrl-3");
    assert_eq!(rig.probe(cx).expect("a body").mode, ImageMode::Overlay);
}

#[gpui_kit::test]
fn compare_image_sizes_that_differ_offer_no_overlay(cx: &mut TestAppContext) {
    let rig = rig(cx, true);
    rig.open(cx, mismatch(), None);
    rig.press(cx, "2");
    let probe = rig.probe(cx).expect("a body");
    assert_eq!(probe.mode, ImageMode::SideBySide);
    assert!(probe.parts, "the picker still shows");
    let mut body = ImageBody::new(true);
    body.set_result(scored(), None);
    assert!(body.set_mode(ImageMode::Overlay));
    body.set_result(mismatch(), None);
    assert_eq!(body.mode(), ImageMode::SideBySide, "an open overlay closes");
}

#[gpui_kit::test]
fn compare_image_a_new_mask_is_rebuilt_and_a_missing_one_is_dropped(cx: &mut TestAppContext) {
    let rig = rig(cx, true);
    rig.open(cx, scored(), Some(mask()));
    let other = ImageMask {
        width: 2,
        height: 2,
        pixels: vec![1, 2, 3, 4],
    };
    rig.send_image(cx, scored(), Some(other));
    let probe = rig.probe(cx).expect("a body");
    assert!(
        probe.mask && probe.mask_ready,
        "the new mask is drawn: {probe:?}"
    );
    assert_eq!(rig.gpu(cx), 3, "the old mask picture was given back");
    rig.press(cx, "3");
    rig.send_image(cx, scored(), None);
    let probe = rig.probe(cx).expect("a body");
    assert!(!probe.mask && !probe.mask_ready, "no mask now: {probe:?}");
    assert_eq!(probe.mode, ImageMode::SideBySide, "the mask view closed");
    assert_eq!(rig.gpu(cx), 2);
}

#[gpui_kit::test]
fn compare_image_an_idle_screen_asks_for_no_redraw(cx: &mut TestAppContext) {
    let rig = rig(cx, true);
    rig.open(cx, scored(), Some(mask()));
    let changed = rig.view.update(cx, |view, cx| view.poll_compare_screen(cx));
    assert!(!changed, "nothing is left to start");
}
