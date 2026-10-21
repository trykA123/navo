#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    reason = "a release timing test prints its budgets"
)]

#[path = "../../../../../rusty-automotive/tests/elf_build/mod.rs"]
mod elf_build;
#[path = "../../../../../rusty-automotive/tests/elf_build/rich.rs"]
#[allow(
    dead_code,
    reason = "the shared fixtures serve more tests than this one"
)]
mod rich;

use std::time::{Duration, Instant};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};

use crate::window_view::FerriteWindow;
use crate::window_view::tests::{press, settle};
use crate::window_view::viewer::tests::elf_rich::{open_with_mdf, preset, settle_all};
use crate::window_view::viewer::tests::{text_of, visible};

const FRAME_BUDGET_MS: u64 = 100;
const MOVES: usize = 10;

fn millis(took: Duration) -> f64 {
    took.as_secs_f64() * 1000.0
}

fn frame(cx: &mut TestAppContext, window: AnyWindowHandle) -> Duration {
    let started = Instant::now();
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("window");
    started.elapsed()
}

fn run(cx: &mut TestAppContext, name: &str, bytes: Vec<u8>) -> Vec<(String, f64)> {
    let mut rows = Vec::new();
    let started = Instant::now();
    let (window, view) = open_with_mdf(cx, bytes, rich::MDF, None);
    rows.push((
        format!("{name}: open and index (Simple)"),
        millis(started.elapsed()),
    ));
    assert!(visible(cx, window, "elf-status"), "the index is ready");

    let started = Instant::now();
    switch_to_rich(cx, window, &view);
    rows.push((format!("{name}: switch call"), millis(started.elapsed())));
    let first = frame(cx, window);
    rows.push((format!("{name}: first Rich frame"), millis(first)));
    let started = Instant::now();
    settle_all(cx, window, &view);
    rows.push((
        format!("{name}: model job and settle"),
        millis(started.elapsed()),
    ));
    rows.push((
        format!("{name}: steady Rich frame"),
        millis(frame(cx, window)),
    ));

    press(cx, window, &view, "tab");
    press(cx, window, &view, "home");
    let mut moves = Vec::new();
    for _ in 0..MOVES {
        let started = Instant::now();
        press(cx, window, &view, "right");
        moves.push(millis(started.elapsed()));
    }
    assert!(!text_of(cx, window, "elf-inspector-name").is_empty());
    let worst = moves.iter().copied().fold(0.0, f64::max);
    let mean = moves.iter().sum::<f64>() / MOVES as f64;
    rows.push((format!("{name}: strip move, mean of {MOVES}"), mean));
    rows.push((format!("{name}: strip move, worst of {MOVES}"), worst));
    rows
}

fn switch_to_rich(cx: &mut TestAppContext, window: AnyWindowHandle, view: &Entity<FerriteWindow>) {
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, cx| view.set_values(preset(1), window, cx));
    })
    .expect("window");
    settle(cx, view);
}

fn report(rows: &[(String, f64)]) {
    println!("| metric | ms | budget ms |\n| --- | ---: | ---: |");
    for (name, took) in rows {
        let budget = if name.contains("frame") || name.contains("strip move") {
            FRAME_BUDGET_MS.to_string()
        } else {
            "—".to_owned()
        };
        println!("| {name} | {took:.3} | {budget} |");
    }
    #[cfg(not(debug_assertions))]
    for (name, took) in rows {
        if name.contains("first Rich frame") {
            assert!(*took <= FRAME_BUDGET_MS as f64, "{name}: {took:.3} ms");
        }
    }
}

#[gpui_kit::test]
#[ignore = "release timing on ~30 MB files; run explicitly with --run-ignored all"]
fn large_elf_files_stay_within_the_rich_frame_budget(cx: &mut TestAppContext) {
    let mut rows = run(cx, "merged", rich::merged_layout(99_000).build());
    rows.extend(run(cx, "split", rich::split_layout(56_000).build()));
    report(&rows);
}
