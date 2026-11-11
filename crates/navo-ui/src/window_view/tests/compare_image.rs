//! The image diff body (V40, from I4): the two pictures side by side, the overlay wipe, the
//! difference mask, and what the app's scores and D20 fallbacks say about the pair.

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use navo_core::Location;

use super::{
    NavoWindow, StartOptions, fake, open_options_sized, options_for, press, settle, two_panes,
};
use crate::browser::{
    CompareControl, CompareHooks, CompareKind, CompareRow, CompareSide, CompareSink, CompareUpdate,
    RowStatus,
};
use crate::commands::Keymap;
use crate::compare_diff::fake::FakeDiffHooks;
use crate::compare_diff::image_body::{ImageBody, ImageMode, SPLIT_STEP};
use crate::compare_diff::{DiffUpdate, ImageLimit, ImageMask, ImageScores, PairKind};
use crate::primitives::GPU_IMAGES;
use crate::settings::Values;
use crate::window_view::compare_screen::image::Probe;
use crate::window_view::{FileHooks, OpenHooks};

mod review;

const KEYMAP: &str = concat!(
    include_str!("../../commands/keymap.toml"),
    "\n",
    include_str!("../../commands/compare_keymap.toml")
);

type Sink = Arc<Mutex<Option<CompareSink>>>;

/// A folder compare that never walks; the test sends its rows.
struct Idle;

impl CompareControl for Idle {
    fn focus(&self, _row_ids: Vec<u64>) {}
    fn stop_row(&self, _id: u64) {}
    fn stop_all(&self) {}
    fn refresh(&self) {}
    fn continue_walk(&self) {}
}

struct Hooks(Sink);

impl CompareHooks for Hooks {
    fn supports(&self, _location: &Location) -> bool {
        true
    }

    fn start(
        &self,
        _sides: (&Location, &Location),
        _values: &Values,
        sink: CompareSink,
    ) -> Box<dyn CompareControl> {
        *self.0.lock().unwrap() = Some(sink);
        Box::new(Idle)
    }
}

/// One encoded PNG, the same for both sides unless a test asked for a broken one.
fn picture() -> Vec<u8> {
    let image = RgbaImage::from_fn(40, 30, |x, y| {
        Rgba([
            u8::try_from(x % 250).unwrap_or(0),
            u8::try_from(y % 250).unwrap_or(0),
            90,
            255,
        ])
    });
    let mut out = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut out, ImageFormat::Png)
        .expect("a png encodes");
    out.into_inner()
}

/// The bytes the window's read hook serves, by full location, and how often it was asked.
#[derive(Clone, Default)]
struct Disk {
    files: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    reads: Arc<AtomicUsize>,
}

impl Disk {
    fn new(good: bool) -> Self {
        let bytes = picture();
        let files = HashMap::from([
            (
                "home/a.png".to_owned(),
                if good {
                    bytes.clone()
                } else {
                    b"no picture".to_vec()
                },
            ),
            ("home/other.png".to_owned(), bytes),
        ]);
        Self {
            files: Arc::new(Mutex::new(files)),
            reads: Arc::default(),
        }
    }

    fn reads(&self) -> usize {
        self.reads.load(Ordering::Relaxed)
    }

    fn hooks(&self) -> FileHooks {
        let (files, reads) = (self.files.clone(), self.reads.clone());
        FileHooks {
            bytes: Arc::new(move |location, _limit| {
                reads.fetch_add(1, Ordering::Relaxed);
                let name = location.path().to_text();
                let found = files
                    .lock()
                    .expect("the disk")
                    .get(&name)
                    .cloned()
                    .ok_or_else(|| "There is no such file.".to_owned())?;
                Ok(found)
            }),
            ..FileHooks::none()
        }
    }
}

struct Rig {
    window: AnyWindowHandle,
    view: Entity<NavoWindow>,
    diff: Arc<FakeDiffHooks>,
    disk: Disk,
}

/// A folder compare on `a.png` (selected) and `other.png`; `good` says whether it reads as a
/// picture.
fn rig(cx: &mut TestAppContext, good: bool) -> Rig {
    let _shown = crate::compare_visible::show_compare();
    let fs = fake();
    fs.add_file("home/a.png", 40);
    fs.add_file("home/other.png", 40);
    let sink = Sink::default();
    let diff = Arc::new(FakeDiffHooks::default());
    let disk = Disk::new(good);
    let options = StartOptions {
        files: disk.hooks(),
        open: OpenHooks {
            compare: Some(Arc::new(Hooks(sink.clone()))),
            diff: Some(diff.clone()),
            ..OpenHooks::none()
        },
        ..options_for(Arc::clone(&fs), two_panes(&fs))
    };
    let (window, view) = open_options_sized(cx, options, gpui_kit::px(1000.));
    let keymap = Keymap::parse(KEYMAP).expect("keymap");
    cx.update(|cx| crate::window_view::keys::rebind_app_keys(keymap, cx));
    press(cx, window, &view, "ctrl-shift-d");
    let side = Some(CompareSide {
        kind: CompareKind::File,
        size: 40,
        modified_ms: None,
    });
    let rows = (1u64..)
        .zip(["a.png", "other.png"])
        .map(|(id, name)| CompareRow::new(id, name.to_owned(), RowStatus::Different, (side, side)))
        .collect();
    let send = sink.lock().unwrap().clone().expect("a started compare");
    send(CompareUpdate::Rows {
        generation: 1,
        rows,
        progress: Vec::new(),
    });
    settle(cx, &view);
    press(cx, window, &view, "down");
    Rig {
        window,
        view,
        diff,
        disk,
    }
}

impl Rig {
    /// Opens the selected pair and hands it `scores`; `mask` says whether one came with it.
    fn open(&self, cx: &mut TestAppContext, scores: ImageScores, mask: Option<ImageMask>) {
        press(cx, self.window, &self.view, "enter");
        let sink = self.diff.pair_sink();
        sink(DiffUpdate::Opened {
            request: 1,
            kind: PairKind::Image,
            text: None,
        });
        sink(DiffUpdate::Image {
            request: 1,
            scores,
            mask,
        });
        self.paint(cx);
    }

    /// Draws a frame (which starts the read) and waits for the worker to come back.
    fn paint(&self, cx: &mut TestAppContext) {
        for _ in 0..3 {
            settle(cx, &self.view);
            self.redraw(cx);
        }
    }

    fn redraw(&self, cx: &mut TestAppContext) {
        cx.update_window(self.window, |_, window, cx| window.render_frame(cx))
            .expect("window");
    }

    fn press(&self, cx: &mut TestAppContext, key: &str) {
        press(cx, self.window, &self.view, key);
    }

    fn probe(&self, cx: &mut TestAppContext) -> Option<Probe> {
        self.view.update(cx, |view, _| view.image_probe())
    }

    /// The GPU images the screen holds: two pictures, plus the mask once it is drawn.
    fn gpu(&self, cx: &mut TestAppContext) -> u64 {
        cx.read(|cx| crate::primitives::read_tally(&GPU_IMAGES, cx))
            .items
    }
}

fn scored() -> ImageScores {
    ImageScores::Scored {
        ssim: 0.98,
        rgb_min_rms_similarity: 0.97,
        luma_hellinger_similarity: 0.99,
        alpha: None,
    }
}

fn mask() -> ImageMask {
    ImageMask {
        width: 4,
        height: 2,
        pixels: vec![0, 0, 8, 200, 255, 40, 0, 0],
    }
}

#[gpui_kit::test]
fn compare_image_reads_both_sides_and_shows_the_similarity(cx: &mut TestAppContext) {
    let rig = rig(cx, true);
    rig.open(cx, scored(), Some(mask()));
    let asked = rig.diff.requests();
    assert!(
        asked[0].left.is_some() && asked[0].right.is_some(),
        "{asked:?}"
    );
    let probe = rig.probe(cx).expect("an image body");
    assert_eq!(probe.mode, ImageMode::SideBySide);
    assert_eq!(probe.similarity, Some(98), "98 % of the structure agrees");
    assert!(probe.parts, "Rich shows the view row and the scores");
    assert_eq!(probe.notice, None, "a scored pair has nothing to report");
    let picked = cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.find("compare-image-picker").visible()
    });
    assert!(
        picked.is_ok_and(|seen| seen),
        "the image picker is on screen"
    );
    assert_eq!(rig.disk.reads(), 2, "each side was read once");
    assert_eq!(probe.ready, 2, "both sides read");
    assert_eq!(rig.gpu(cx), 3, "both pictures and the mask are on the GPU");
}

#[gpui_kit::test]
fn compare_image_view_keys_walk_the_views_and_the_wipe_stays_inside(cx: &mut TestAppContext) {
    let rig = rig(cx, true);
    rig.open(cx, scored(), Some(mask()));
    rig.press(cx, "2");
    assert_eq!(rig.probe(cx).expect("a body").mode, ImageMode::Overlay);
    for _ in 0..30 {
        rig.press(cx, "left");
    }
    assert_eq!(
        rig.probe(cx).expect("a body").split,
        0.0,
        "stops at the left"
    );
    for _ in 0..60 {
        rig.press(cx, "right");
    }
    assert_eq!(
        rig.probe(cx).expect("a body").split,
        1.0,
        "stops at the right"
    );
    rig.press(cx, "3");
    assert_eq!(rig.probe(cx).expect("a body").mode, ImageMode::Mask);
    rig.press(cx, "1");
    assert_eq!(
        rig.probe(cx).expect("a body").mode,
        ImageMode::SideBySide,
        "1 goes back to the pair"
    );
}

#[gpui_kit::test]
fn compare_image_mask_view_waits_for_a_mask_from_the_app(cx: &mut TestAppContext) {
    let rig = rig(cx, true);
    rig.open(cx, scored(), None);
    let probe = rig.probe(cx).expect("a body");
    assert!(!probe.mask, "no mask arrived");
    rig.press(cx, "3");
    assert_eq!(
        rig.probe(cx).expect("a body").mode,
        ImageMode::SideBySide,
        "the mask view is not offered without one"
    );
}

#[gpui_kit::test]
fn compare_image_names_every_way_the_app_gave_up(cx: &mut TestAppContext) {
    let rig = rig(cx, true);
    rig.open(
        cx,
        ImageScores::TooLarge {
            reason: ImageLimit::Pixels,
            equal: false,
        },
        None,
    );
    let probe = rig.probe(cx).expect("a body");
    assert_eq!(probe.notice, Some("compare-image-note-pixels"));
    assert_eq!(probe.similarity, None, "nothing was scored");
    assert_eq!(
        rig.gpu(cx),
        0,
        "an unread pair is not decoded for a hash result"
    );

    rig.open(
        cx,
        ImageScores::SizeMismatch {
            left: crate::compare_diff::ImageSize {
                width: 40,
                height: 30,
            },
            right: crate::compare_diff::ImageSize {
                width: 30,
                height: 40,
            },
        },
        None,
    );
    let probe = rig.probe(cx).expect("a body");
    assert_eq!(probe.notice, Some("compare-image-note-sizes"));
    assert_eq!(rig.gpu(cx), 2, "both sides still show");
}

#[gpui_kit::test]
fn compare_image_simple_hides_the_parts_it_promised(cx: &mut TestAppContext) {
    let rig = rig(cx, true);
    rig.open(cx, scored(), Some(mask()));
    let simple = cx.update_window(rig.window, |_, window, cx| {
        window.dispatch_action(Box::new(crate::window_view::viewer::ViewSimple), cx);
    });
    assert!(simple.is_ok());
    rig.paint(cx);
    let probe = rig.probe(cx).expect("a body");
    assert!(!probe.parts, "Simple shows the pair and nothing else");
    assert_eq!(probe.similarity, None, "the score line is a Rich part");
    rig.press(cx, "2");
    assert_eq!(rig.probe(cx).expect("a body").mode, ImageMode::SideBySide);
}

#[gpui_kit::test]
fn compare_image_a_side_that_cannot_be_read_is_named(cx: &mut TestAppContext) {
    let rig = rig(cx, false);
    rig.open(cx, scored(), Some(mask()));
    let probe = rig.probe(cx).expect("a body");
    assert_eq!(probe.notice, Some("compare-image-note-unreadable"));
    assert_eq!(probe.ready, 0, "neither side is drawn");
}

#[gpui_kit::test]
fn compare_image_closing_the_screen_frees_its_pictures(cx: &mut TestAppContext) {
    let rig = rig(cx, true);
    rig.open(cx, scored(), Some(mask()));
    assert_eq!(rig.gpu(cx), 3);
    rig.press(cx, "escape");
    assert_eq!(rig.gpu(cx), 0, "a closed pair holds no GPU image");
    assert!(rig.probe(cx).is_none());
}

#[gpui_kit::test]
fn compare_image_stale_and_older_requests_do_not_reopen_a_pair(cx: &mut TestAppContext) {
    let rig = rig(cx, true);
    rig.open(cx, scored(), Some(mask()));
    let sink = rig.diff.pair_sink();
    sink(DiffUpdate::Image {
        request: 0,
        scores: ImageScores::HashOnly { equal: true },
        mask: None,
    });
    settle(cx, &rig.view);
    let probe = rig.probe(cx).expect("a body");
    assert_eq!(
        probe.similarity,
        Some(98),
        "an older request changed nothing"
    );
}

#[test]
fn image_body_wipe_and_view_rules_stay_in_range() {
    let mut body = ImageBody::new(true);
    assert_eq!(body.split(), 0.5);
    assert!(body.nudge(false));
    let one = 0.5 - SPLIT_STEP;
    assert!((body.split() - one).abs() < f32::EPSILON);
    for _ in 0..40 {
        body.nudge(false);
    }
    assert_eq!(body.split(), 0.0, "the wipe cannot leave the stage");
    for _ in 0..80 {
        body.nudge(true);
    }
    assert_eq!(body.split(), 1.0);
    assert!(!body.nudge(true), "a wipe at the edge has nowhere to go");
    assert!(!body.set_mode(ImageMode::Mask), "no mask, no mask view");
    body.set_result(scored(), Some(Arc::new(mask())));
    assert!(body.set_mode(ImageMode::Mask));
    let mut plain = ImageBody::new(false);
    plain.set_result(scored(), Some(Arc::new(mask())));
    assert!(
        !plain.set_mode(ImageMode::Overlay),
        "Simple does not offer the extra views"
    );
    assert_eq!(plain.similarity(), None);
    assert_eq!(plain.notice(), None);
    assert_eq!(body.similarity(), Some(98));
}

#[gpui_kit::test]
fn compare_image_details_drawer_lists_kind_sizes_view_and_similarity(cx: &mut TestAppContext) {
    let rig = rig(cx, true);
    rig.open(cx, scored(), Some(mask()));
    rig.press(cx, "ctrl-i");
    rig.paint(cx);
    let facts = rig.view.read_with(cx, |view, _| view.body_facts_probe());
    let facts = facts.expect("the drawer is open");
    assert_eq!(facts[0], "Kind: Image");
    assert!(
        facts[1].starts_with("Sizes: ") && facts[1].contains(" × "),
        "{facts:?}"
    );
    assert_eq!(facts[2], "View: Side by side");
    assert_eq!(facts[3], "Progress: 98 % similar");
}
