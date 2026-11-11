//! The display line reader of a large pair keeps answering after the diff has finished.

use std::sync::mpsc::Receiver;
use std::thread;
use std::time::{Duration, Instant};

use navo_ui::compare_diff::{DiffUpdate, PairControl};

use super::large::{WAIT, pair, request, sink};
use crate::compare::diff::large::LargeDiff;

/// Waits until a snapshot says every section is diffed.
fn wait_for_the_diff_to_finish(receive: &Receiver<DiffUpdate>) {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        let Ok(update) = receive.recv_timeout(Duration::from_millis(100)) else {
            continue;
        };
        if let DiffUpdate::Large { progress, .. } = update
            && progress.is_some_and(|p| p.total > 0 && p.done >= p.total)
        {
            return;
        }
    }
    panic!("the diff never finished");
}

#[test]
#[ignore = "chk-large-text: line worker dies when the drain worker drops the engine (cancels the shared token)"]
fn a_line_window_asked_after_the_diff_finished_still_arrives() {
    let dir = tempfile::tempdir().unwrap();
    let input = pair(dir.path(), 4_000, 1_234);
    let (sink, receive) = sink();
    let control = LargeDiff::start(request(), input, sink);
    control.lines(0..10);
    wait_for_the_diff_to_finish(&receive);
    // The drain worker polls every 20 ms; a second is ample for it to see the engine end.
    thread::sleep(Duration::from_secs(1));
    control.lines(2_000..2_010);
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut far = None;
    while far.is_none() && Instant::now() < deadline {
        if let Ok(DiffUpdate::LargeLines { rows, left, .. }) =
            receive.recv_timeout(Duration::from_millis(100))
            && rows.start == 2_000
        {
            far = Some(left);
        }
    }
    let far = far.expect("no line window arrived after the diff finished");
    assert_eq!(far[0], "line 2000");
    drop(control);
}
