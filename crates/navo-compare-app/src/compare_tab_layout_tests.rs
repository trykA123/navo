//! Layout check of the compare tab (blank diff bug): an opened text diff must fill the tab, so its
//! status line sits at the bottom of the stage, just above the footer, not under the header.

use navo_compare_cli::args::SideSpec;
use std::fs;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, Pixels, TestAppContext};
use sidewise_host::compare::diff::hooks::DiffBackend;

use crate::compare_tab_script::scripted_shell;
use crate::compare_tab_tests::TempDir;
use crate::screens::compare_host::TEST_DIFF_HOOKS;
use crate::screens::compare_tab::CompareRequest;
use crate::shell::Shell;
use crate::start_tabs::StartChoice;

/// Opens two 30-line files (3 lines differ) on Navo's real in-memory diff backend, and renders
/// until the change counter ("3 changes") is drawn. Returns its bottom edge and the footer's top.
fn changes_bottom_and_footer_top(cx: &mut TestAppContext) -> (Pixels, Pixels) {
    let dir = TempDir::new("layout");
    let lines = |tag: &str| -> String {
        (0..30)
            .map(|n| match n % 10 {
                5 => format!("{tag} line {n}\n"),
                _ => format!("same line {n}\n"),
            })
            .collect()
    };
    let (left, right) = (dir.path().join("l.txt"), dir.path().join("r.txt"));
    fs::write(&left, lines("left")).expect("write left");
    fs::write(&right, lines("right")).expect("write right");
    let opened: (AnyWindowHandle, Entity<Shell>) = scripted_shell(cx);
    TEST_DIFF_HOOKS.with_borrow_mut(|slot| *slot = Some(Arc::new(DiffBackend::default())));
    let (window, view) = &opened;
    let request = CompareRequest {
        left: SideSpec::Path(left),
        right: SideSpec::Path(right),
        choice: StartChoice::default(),
    };
    cx.update_window(*window, |_, window, cx| {
        view.update(cx, |shell, cx| shell.open_compare((0, request), window, cx))
    })
    .expect("window")
    .expect("open");
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        cx.run_until_parked();
        let seen = cx
            .update_window(*window, |_, window, cx| {
                window.render_frame(cx);
                let line = window.try_find("compare-screen-changes")?.bounds();
                let footer = window.find("status-bar").bounds();
                Some((line.origin.y + line.size.height, footer.origin.y))
            })
            .expect("window");
        if let Some(found) = seen {
            return found;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("the change counter never drew: the pair did not open");
}

/// The diff's status line must reach the footer; in the blank-diff bug the diff root (`flex_1`)
/// sits in the tab's block-layout div, collapses to header plus status line and leaves the body empty.
#[gpui_kit::test]
// A real pair-diff thread trips GPUI's deterministic test scheduler on Windows.
#[cfg_attr(windows, ignore = "real pair-diff worker; Linux covers the layout")]
fn the_text_diff_fills_the_tab_down_to_the_footer(cx: &mut TestAppContext) {
    let (bottom, footer_top) = changes_bottom_and_footer_top(cx);
    assert!(
        footer_top - bottom < Pixels::from(8.),
        "diff status line ends at {bottom:?} but the footer starts at {footer_top:?}: the diff body has no height"
    );
}
