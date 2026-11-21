//! Width check of the compare tab's hex body (B2): the bytes per row adapt to the width of a
//! half (16, 8 or 4), so every hex cell and the ASCII column stay inside their half. A row's
//! box always spans its half, so the check measures the row's drawn text (its label) at the
//! test text system's column width, 0.6 of the font size.

use navo_compare_cli::args::SideSpec;
use std::fs;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Pixels, TestAppContext, VisualTestContext, px, size,
};
use sidewise_host::compare::diff::hooks::DiffBackend;
use sidewise_kit::tokens;

use crate::compare_tab_script::scripted_shell;
use crate::compare_tab_tests::TempDir;
use crate::screens::compare_host::TEST_DIFF_HOOKS;
use crate::screens::compare_tab::CompareRequest;
use crate::start_tabs::StartChoice;

/// What row 0 draws: each half's bounds and the width its text needs, and the bytes per row.
struct Drawn {
    sides: [Bounds<Pixels>; 2],
    needs: [Pixels; 2],
    per_row: u64,
}

/// The width a hex row's content needs: gutter, offset, hex and ASCII with their gaps and
/// padding, at 0.6 × the small font size per column. The label joins the three with 2 spaces.
fn needed(label: &str, t: &tokens::Theme) -> Pixels {
    let columns = label.chars().count().saturating_sub(4) as f32;
    let pads = t.spacing.xs + t.spacing.sm + t.spacing.xl * 4.;
    pads + t.typography.small * (0.6 * columns)
}

/// Opens two 256-byte binary files (a few bytes differ) on the real diff backend.
fn open_binary_pair(cx: &mut TestAppContext) -> (AnyWindowHandle, TempDir) {
    let dir = TempDir::new("hexwidth");
    let left: Vec<u8> = (0..=255u8).collect();
    let mut right = left.clone();
    right[5] = 0xFF;
    right[200] = 0xAA;
    let (l, r) = (dir.path().join("l.bin"), dir.path().join("r.bin"));
    fs::write(&l, &left).expect("write left");
    fs::write(&r, &right).expect("write right");
    let (window, view) = scripted_shell(cx);
    TEST_DIFF_HOOKS.with_borrow_mut(|slot| *slot = Some(Arc::new(DiffBackend::default())));
    let request = CompareRequest {
        left: SideSpec::Path(l),
        right: SideSpec::Path(r),
        choice: StartChoice::default(),
    };
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |shell, cx| shell.open_compare((0, request), window, cx))
    })
    .expect("window")
    .expect("open");
    (window, dir)
}

/// Resizes the window, then renders until row 1 is drawn with the same layout twice running.
fn drawn_at(cx: &mut TestAppContext, window: AnyWindowHandle, width: f32, height: f32) -> Drawn {
    VisualTestContext::from_window(window, cx).simulate_resize(size(px(width), px(height)));
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut last: Option<(u64, Bounds<Pixels>)> = None;
    while Instant::now() < deadline {
        cx.run_until_parked();
        let seen = cx
            .update_window(window, |_, window, cx| {
                window.render_frame(cx);
                let t = tokens::current(cx);
                let need = |at: usize| -> Option<Pixels> {
                    Some(needed(window.try_find(("paged-row", at))?.label()?, t))
                };
                let side = |at: usize| {
                    window
                        .try_find(("compare-hex-side", at))
                        .map(|s| s.bounds())
                };
                let label = window.try_find(("paged-row", 2usize))?.label()?.to_string();
                let per_row = u64::from_str_radix(label.split_whitespace().next()?, 16).ok()?;
                Some(Drawn {
                    sides: [side(0)?, side(1)?],
                    needs: [need(0)?, need(1)?],
                    per_row,
                })
            })
            .expect("window");
        if let Some(drawn) = seen {
            let key = (drawn.per_row, drawn.sides[1]);
            if last == Some(key) {
                return drawn;
            }
            last = Some(key);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("the hex rows never settled at {width}x{height}");
}

/// Each half's row text, ASCII column included, fits inside its own half.
fn assert_inside(drawn: &Drawn, width: f32) {
    for half in 0..2 {
        let (need, side) = (drawn.needs[half], drawn.sides[half].size.width);
        assert!(
            need <= side,
            "at {width} px half {half} needs {need:?} but has {side:?} ({} bytes per row)",
            drawn.per_row
        );
    }
}

/// At 1200 and 780 px a 16-byte row does not fit a half: the row narrows and stays inside;
/// at 1920 px it is 16 bytes again.
#[gpui_kit::test]
// A real pair-diff thread trips GPUI's deterministic test scheduler on Windows.
#[cfg_attr(windows, ignore = "real pair-diff worker; Linux covers the layout")]
fn hex_rows_narrow_to_fit_their_half(cx: &mut TestAppContext) {
    let (window, _dir) = open_binary_pair(cx);
    let wide = drawn_at(cx, window, 1920., 1080.);
    assert_eq!(wide.per_row, 16, "a wide window keeps 16 bytes per row");
    assert_inside(&wide, 1920.);
    let mid = drawn_at(cx, window, 1200., 760.);
    assert_eq!(mid.per_row, 8, "1200 px fits 8 bytes per row");
    assert_inside(&mid, 1200.);
    let narrow = drawn_at(cx, window, 780., 560.);
    assert_eq!(narrow.per_row, 4, "780 px fits 4 bytes per row");
    assert_inside(&narrow, 780.);
}
