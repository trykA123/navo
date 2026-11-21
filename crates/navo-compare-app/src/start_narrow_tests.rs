//! Narrow (`500 px` Main) and wide (`920 px`) wrapping tests for [`StartScreen`] field groups.

use std::cell::RefCell;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use gpui_kit::base::TestSupportExt as _;
use gpui_kit::base::test_support::{find, snapshots, texts};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Context, ElementId, Entity, InteractiveElement, IntoElement,
    ParentElement, Pixels, Render, Styled, TestAppContext, VisualTestContext, Window, div, px,
    size,
};
use navo_compare_cli::args::SideSpec;
use navo_compare_profile::recent::StartRows;
use sidewise_kit::tokens::{self, Mode};

use crate::start::{Side, SideHint, StartEvent, StartScreen};
use crate::test_window::open;
use crate::theme_data::palettes::DIMS;

/// Hosts a [`StartScreen`] entity inside a fixed-width bounded column within a wider viewport.
struct BoundedStart {
    width: Pixels,
    screen: Entity<StartScreen>,
}

impl Render for BoundedStart {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("start-host")
            .test_support()
            .w(self.width)
            .h(px(600.))
            .child(self.screen.clone())
    }
}

type HostWindow = (AnyWindowHandle, Entity<BoundedStart>, Entity<StartScreen>);

/// Opens a [`BoundedStart`] host via `test_window::open` resized to `viewport_w` in `mode`.
fn open_bounded(cx: &mut TestAppContext, viewport_w: f32, parent_w: f32, mode: Mode) -> HostWindow {
    let cwd = std::env::current_dir().unwrap_or_default();
    let (window, host): (_, Entity<BoundedStart>) = open(cx, move |window, cx| {
        let screen = cx.new(|cx| StartScreen::new(StartRows::default(), cwd, window, cx));
        BoundedStart {
            width: px(parent_w),
            screen,
        }
    });
    cx.update(|cx| tokens::apply(mode, cx));
    cx.simulate_window_resize(window, size(px(viewport_w), px(600.)));
    let screen = host.read_with(cx, |h, _| h.screen.clone());
    cx.update_window(window, |_, window, cx| {
        screen.update(cx, |s, cx| s.focus_left(window, cx));
        window.render_frame(cx);
    })
    .expect("window");
    (window, host, screen)
}

/// Sets both fields and runs bounded executor/render turns until both hints leave `Checking`.
fn settle_sides(cx: &mut TestAppContext, hw: &HostWindow, left: &str, right: &str) {
    cx.update_window(hw.0, |_, window, cx| {
        hw.2.update(cx, |sc, cx| sc.set_sides(left, right, window, cx));
        window.render_frame(cx);
    })
    .expect("window");
    for _ in 0..50 {
        cx.run_until_parked();
        let done = cx
            .update_window(hw.0, |_, window, cx| {
                window.render_frame(cx);
                let (l, r) = hw.2.read(cx).hints();
                *l != SideHint::Checking && *r != SideHint::Checking
            })
            .expect("window");
        if done {
            return;
        }
    }
    panic!("start hints did not settle within bounded frames");
}

/// Verifies `id` is visible, positive-sized, and inside `[min_x, max_x]`, returning its bounds.
fn assert_in_span(
    window: &Window,
    id: &str,
    min_x: Pixels,
    max_x: Pixels,
) -> gpui_kit::Bounds<Pixels> {
    let snap =
        find(window, &[], &ElementId::Name(id.into())).unwrap_or_else(|| panic!("missing {id}"));
    assert!(snap.visible(), "{id} must be visible");
    let b = snap.bounds();
    assert!(
        b.size.width > px(0.) && b.size.height > px(0.),
        "{id} must have positive bounds: {b:?}"
    );
    assert!(
        b.origin.x >= min_x && b.origin.x + b.size.width <= max_x,
        "{id} ({b:?}) outside [{min_x:?}, {max_x:?}]"
    );
    b
}

/// Verifies `text` appears `expected` times as visible `TextSnapshot` inside `[min_x, max_x]`.
fn assert_text_count(window: &Window, text: &str, expected: usize, max_x: Pixels) {
    let min_x = px(64.);
    let hits: Vec<_> = texts(window)
        .into_iter()
        .filter(|e| e.visible() && e.text() == text)
        .collect();
    assert_eq!(
        hits.len(),
        expected,
        "expected {text:?} x{expected}, got {}",
        hits.len()
    );
    for hit in hits {
        let b = hit.bounds();
        assert!(
            b.size.width > px(0.) && b.origin.x >= min_x && b.origin.x + b.size.width <= max_x,
            "{text:?} ({b:?}) outside [{min_x:?}, {max_x:?}]"
        );
    }
}

/// Asserts narrow two-row wrap (`>= 80 px` inputs, Swap/Compare, labels, chips, hints, zero overflow).
fn assert_narrow_wrapped(window: &Window, parent_w: f32, chips: &[&str]) -> (Pixels, Pixels) {
    let host = assert_in_span(window, "start-host", px(0.), px(parent_w));
    assert_eq!(host.size.width, px(parent_w));
    let (min_x, max_x) = (px(64.), px(parent_w - 64.));
    let left = assert_in_span(window, "start-left", min_x, max_x);
    let swap = assert_in_span(window, "start-swap", min_x, max_x);
    let right = assert_in_span(window, "start-right", min_x, max_x);
    let compare = assert_in_span(window, "start-compare", min_x, max_x);
    assert!(
        left.size.width >= px(80.),
        "left field >= 80px, got {left:?}"
    );
    assert!(
        right.size.width >= px(80.),
        "right field >= 80px, got {right:?}"
    );
    assert!(
        left.origin.x + left.size.width <= swap.origin.x,
        "Left before Swap"
    );
    assert!(
        right.origin.y > left.origin.y,
        "Right wraps below Left at {parent_w}px"
    );
    assert!(
        right.origin.x + right.size.width <= compare.origin.x,
        "Right before Compare"
    );
    assert_in_span(window, "hint-tab", min_x, max_x);
    assert_in_span(window, "hint-enter", min_x, max_x);
    assert_text_count(window, "Left", 1, max_x);
    assert_text_count(window, "Right", 1, max_x);
    for chip in chips {
        let count = chips.iter().filter(|c| *c == chip).count();
        assert_text_count(window, chip, count, max_x);
    }
    for snap in snapshots(window) {
        let b = snap.bounds();
        if snap.visible() && b.size.width > px(0.) {
            assert!(
                b.origin.x >= px(0.) && b.origin.x + b.size.width <= px(parent_w),
                "{:?} overflows parent: {b:?}",
                snap.path()
            );
        }
    }
    (left.size.width, right.size.width)
}

/// Narrow `500 px` parent inside `780` & `1200` viewports (Light & Dark) keeps both Inputs `>= 80 px`.
#[gpui_kit::test]
fn narrow_500px_parent_keeps_both_inputs_usable_with_not_found_chips(cx: &mut TestAppContext) {
    assert!(DIMS.start_field_min_w >= px(224.));
    let mut report = String::new();
    for mode in [Mode::Light, Mode::Dark] {
        for viewport_w in [780., 1200.] {
            let hw = open_bounded(cx, viewport_w, 500., mode);
            settle_sides(cx, &hw, "no/such/left", "no/such/right");
            let (lw, rw) = cx
                .update_window(hw.0, |_, window, cx| {
                    assert_eq!(
                        hw.2.read(cx).hints(),
                        (&SideHint::NotFound, &SideHint::NotFound)
                    );
                    assert_narrow_wrapped(window, 500., &["Not found", "Not found"])
                })
                .expect("window");
            let _ = writeln!(
                report,
                "500px@{viewport_w} {mode:?}: left={lw:?} right={rw:?}"
            );
            let swap_at = cx
                .update_window(hw.0, |_, window, _| {
                    window.find("start-swap").bounds().center()
                })
                .expect("window");
            VisualTestContext::from_window(hw.0, cx).simulate_click(swap_at, Default::default());
            cx.run_until_parked();
            cx.update_window(hw.0, |_, window, cx| {
                assert_eq!(
                    hw.2.read(cx).fields[0].read(cx).value().as_str(),
                    "no/such/right"
                );
                window.render_frame(cx);
            })
            .expect("window");
        }
    }
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../target/start500/bounds.log"
    );
    // The evidence log lives in the repo target dir, which a custom CARGO_TARGET_DIR leaves absent.
    if let Some(dir) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(dir).expect("create bounds log dir");
    }
    std::fs::write(path, report).expect("write start500 bounds");
}

/// Exclusive temporary directory created by this test and deleted only if created here.
struct OwnedTempDir(PathBuf);

impl OwnedTempDir {
    /// Creates a unique directory with `std::fs::create_dir`, retrying on `AlreadyExists`.
    fn create() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let pid = std::process::id();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let root = std::env::temp_dir();
        for _ in 0..64 {
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = root.join(format!("sidewise-start-narrow-{pid}-{nanos}-{n}"));
            match std::fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("create temp dir {path:?}: {e}"),
            }
        }
        panic!("exhausted unique temp dir attempts");
    }
}

impl Drop for OwnedTempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Valid Path, GitRef, and Empty hints also keep >=80 px Inputs and zero overflow at 500 px.
#[gpui_kit::test]
fn narrow_500px_handles_path_gitref_and_empty_hints_without_overflow(cx: &mut TestAppContext) {
    let dir = OwnedTempDir::create();
    let hw = open_bounded(cx, 780., 500., Mode::Light);
    let path_str = dir.0.to_string_lossy().into_owned();
    let git_str = format!("{path_str}@main");

    settle_sides(cx, &hw, &path_str, &git_str);
    cx.update_window(hw.0, |_, window, cx| {
        assert_eq!(
            hw.2.read(cx).hints(),
            (
                &SideHint::Path(SideSpec::Path(dir.0.clone())),
                &SideHint::GitProblem(sidewise_git::repo::GitProblem::NotRepository),
            )
        );
        assert_narrow_wrapped(window, 500., &["Path", "Not a Git repository"]);
        assert_in_span(window, "start-git-tip", px(64.), px(500. - 64.));
    })
    .expect("window");

    settle_sides(cx, &hw, "", "");
    cx.update_window(hw.0, |_, window, cx| {
        assert_eq!(hw.2.read(cx).hints(), (&SideHint::Empty, &SideHint::Empty));
        assert_narrow_wrapped(window, 500., &[]);
    })
    .expect("window");
}

/// Asserts single-row DOM order (`Left`, `Swap`, `Right`, `Compare`) and chips in a `920 px` host.
fn assert_wide_single_row(window: &Window) {
    let max_x = px(920. - 64.);
    let (l, s) = (
        assert_in_span(window, "start-left", px(64.), max_x),
        assert_in_span(window, "start-swap", px(64.), max_x),
    );
    let (r, c) = (
        assert_in_span(window, "start-right", px(64.), max_x),
        assert_in_span(window, "start-compare", px(64.), max_x),
    );
    assert_eq!(l.origin.y, r.origin.y, "wide host keeps single row");
    assert!(l.origin.x + l.size.width <= s.origin.x && s.origin.x + s.size.width <= r.origin.x);
    assert!(r.origin.x + r.size.width <= c.origin.x);
    assert_text_count(window, "Not found", 2, max_x);
}

/// Wide `920 px` host keeps single-row order, Tab trap, and invalid Compare focus redirect.
#[gpui_kit::test]
fn wide_920px_parent_stays_one_row_and_keeps_focus_and_compare(cx: &mut TestAppContext) {
    let hw = open_bounded(cx, 1200., 920., Mode::Dark);
    settle_sides(cx, &hw, "no/such/left", "no/such/right");
    cx.update_window(hw.0, |_, window, _| assert_wide_single_row(window))
        .expect("window");
    let events = Rc::new(RefCell::new(Vec::new()));
    let log = Rc::clone(&events);
    let _sub = cx.update(|cx| {
        cx.subscribe(&hw.2, move |_, ev: &StartEvent, _| {
            log.borrow_mut().push(ev.clone())
        })
    });
    cx.simulate_keystrokes(hw.0, "tab");
    cx.update(|cx| {
        hw.1.update(cx, |h, cx| {
            h.width = px(500.);
            cx.notify();
        })
    });
    cx.update_window(hw.0, |_, w, cx| w.render_frame(cx))
        .expect("window");
    cx.simulate_keystrokes(hw.0, "shift-tab");
    cx.update_window(hw.0, |_, w, cx| {
        assert_eq!(hw.2.read(cx).focused_side(w, cx), Some(Side::Left))
    })
    .expect("window");
    cx.simulate_keystrokes(hw.0, "tab");
    let compare_at = cx
        .update_window(hw.0, |_, window, cx| {
            assert_eq!(hw.2.read(cx).focused_side(window, cx), Some(Side::Right));
            window.find("start-compare").bounds().center()
        })
        .expect("window");
    VisualTestContext::from_window(hw.0, cx).simulate_click(compare_at, Default::default());
    cx.run_until_parked();
    cx.update_window(hw.0, |_, window, cx| {
        assert_eq!(hw.2.read(cx).focused_side(window, cx), Some(Side::Left));
    })
    .expect("window");
    assert!(events.borrow().is_empty(), "NotFound Compare stays silent");
}

/// A resolved `repo@rev` side shows the "Git revision" chip; the tip is Folder mode only.
#[gpui_kit::test]
fn git_revision_chip_shows_and_tip_follows_the_mode(cx: &mut TestAppContext) {
    let fixture = sidewise_git::fixture::Fixture::new();
    let _ = fixture.commit("main", &[("a.txt", b"one\n")]);
    let hw = open_bounded(cx, 780., 500., Mode::Dark);
    let git_str = format!("{}@main", fixture.dir.path().to_string_lossy());
    settle_sides(cx, &hw, &git_str, "");
    cx.update_window(hw.0, |_, window, _| {
        assert_narrow_wrapped(window, 500., &["Git revision"]);
        assert_in_span(window, "start-git-tip", px(64.), px(500. - 64.));
    })
    .expect("window");
    let file_tab = cx
        .update_window(hw.0, |_, window, _| {
            window.find("start-tab-file").bounds().center()
        })
        .expect("window");
    VisualTestContext::from_window(hw.0, cx).simulate_click(file_tab, Default::default());
    cx.update_window(hw.0, |_, window, cx| {
        window.render_frame(cx);
        let id = ElementId::Name("start-git-tip".into());
        assert!(find(window, &[], &id).is_none(), "no tip in File mode");
    })
    .expect("window");
}
