//! Compare screen refresh: each reload asks for a new request id, older updates are dropped
//! and the screen fails the pair once ids run out.

use gpui_kit::TestAppContext;

use super::rig;
use crate::compare_diff::{DiffUpdate, PairKind};
use crate::window_view::compare_screen::FileState;

#[gpui_kit::test]
fn compare_screen_drops_an_old_opened_after_a_refresh(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.press(cx, "enter");
    let sink = rig.diff.pair_sink();
    let opened = |request, kind| DiffUpdate::Opened {
        request,
        kind,
        text: None,
    };
    rig.push(cx, &sink, opened(1, PairKind::Binary));
    rig.view.update(cx, |view, _| view.refresh_file_screen());
    rig.push(
        cx,
        &sink,
        opened(1, PairKind::Unavailable("compare-reason-unsupported")),
    );
    let shown = Some(FileState::Opened {
        kind: PairKind::Binary,
        text: None,
    });
    assert_eq!(
        rig.state(cx),
        shown,
        "an Opened from before the refresh is stale"
    );
    rig.push(cx, &sink, opened(2, PairKind::Image));
    assert_eq!(
        rig.state(cx),
        Some(FileState::Opened {
            kind: PairKind::Image,
            text: None
        })
    );
}

#[gpui_kit::test]
fn compare_screen_drops_the_first_refresh_after_a_second(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.press(cx, "enter");
    let sink = rig.diff.pair_sink();
    let opened = |request, kind| DiffUpdate::Opened {
        request,
        kind,
        text: None,
    };
    rig.push(cx, &sink, opened(1, PairKind::Binary));
    rig.view.update(cx, |view, _| view.refresh_file_screen());
    rig.view.update(cx, |view, _| view.refresh_file_screen());
    assert_eq!(rig.diff.refreshes(), vec![2, 3]);
    rig.push(cx, &sink, opened(2, PairKind::Image));
    let binary = Some(FileState::Opened {
        kind: PairKind::Binary,
        text: None,
    });
    assert_eq!(
        rig.state(cx),
        binary,
        "request 2 is stale after the second refresh"
    );
    rig.push(cx, &sink, opened(3, PairKind::Image));
    assert_eq!(
        rig.state(cx),
        Some(FileState::Opened {
            kind: PairKind::Image,
            text: None
        })
    );
}

#[gpui_kit::test]
fn compare_screen_fails_a_refresh_when_request_ids_run_out(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.press(cx, "enter");
    let sink = rig.diff.pair_sink();
    let opened = |request| DiffUpdate::Opened {
        request,
        kind: PairKind::Binary,
        text: None,
    };
    rig.push(cx, &sink, opened(u64::MAX));
    rig.view.update(cx, |view, _| view.refresh_file_screen());
    assert!(
        matches!(rig.state(cx), Some(FileState::Failed(_))),
        "no id is left for the reload: {:?}",
        rig.state(cx)
    );
    assert!(rig.diff.refreshes().is_empty(), "no id is reused");
    rig.push(cx, &sink, opened(u64::MAX));
    assert!(
        matches!(rig.state(cx), Some(FileState::Failed(_))),
        "a late update for the old request is dropped"
    );
}
