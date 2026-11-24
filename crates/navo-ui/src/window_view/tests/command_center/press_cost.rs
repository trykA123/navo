//! Timing of one arrow press in Ctrl+K with a full page of file results; run by hand in release.

use std::time::{Duration, Instant};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};

use super::{Rig, hit, location, rig_with};
use crate::command_center::{Group, Target};

/// Returns the median of `samples`.
fn median(mut samples: Vec<Duration>) -> Duration {
    samples.sort();
    samples[samples.len() / 2]
}

/// Times `count` runs of `step` and returns their median.
fn time(count: usize, mut step: impl FnMut()) -> Duration {
    let samples = (0..count)
        .map(|_| {
            let start = Instant::now();
            step();
            start.elapsed()
        })
        .collect();
    median(samples)
}

/// Opens Ctrl+K with `count` file results shown in the files scope.
fn files_rig(cx: &mut TestAppContext, count: usize) -> Rig {
    let hits = (0..count)
        .map(|n| {
            let target = Target::Open {
                location: location(&format!("dest/report_{n}.txt")),
                folder: false,
            };
            hit(&format!("report_{n}.txt"), Group::Files, target)
        })
        .collect();
    let rig = rig_with(cx, hits);
    rig.press(cx, "ctrl-k");
    rig.type_query(cx, "/report");
    rig
}

#[gpui_kit::test]
#[ignore = "timing measurement, run by hand in a release build"]
#[expect(
    clippy::print_stdout,
    reason = "a by-hand timing run prints its numbers"
)]
fn arrow_press_cost_with_500_file_results(cx: &mut TestAppContext) {
    let rig = files_rig(cx, 500);
    assert!(rig.selection(cx).is_some(), "a row is selected");
    let press = time(20, || rig.press(cx, "down"));
    let frame = time(20, || {
        cx.update_window(rig.window, |_, window, cx| window.render_frame(cx))
            .expect("window");
    });
    let key = time(20, || {
        cx.update_window(rig.window, |_, window, cx| window.press("down", cx))
            .expect("window");
    });
    let park = time(20, || {
        cx.update_window(rig.window, |_, window, cx| window.press("down", cx))
            .expect("window");
        cx.run_until_parked();
    });
    println!(
        "down press with two frames and settle: {press:?}; bare frame: {frame:?}; \
         key dispatch: {key:?}; key dispatch and run until parked: {park:?}"
    );
}
