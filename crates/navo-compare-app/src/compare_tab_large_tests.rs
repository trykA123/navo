//! Tests of H5: a compare still checking after 1,000 ms opens on Large; Enter and the end of
//! the session lead back to Folders; the Large view gets a snapshot after each drain.

use std::sync::Arc;
use std::time::Duration;

use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};
use sidewise_ui::browser::{ComparePhase, CompareUpdate};

use crate::compare_tab_script::{Held, SharedSink};
use crate::compare_tab_tests::{TempDir, shell, submit};
use crate::screens::compare_tab::{CompareTab, TabView, install_test_hooks};

/// A submitted compare on the held engine: shell, tab, the sink and the two folders.
fn started(
    cx: &mut TestAppContext,
    tag: &str,
) -> (
    AnyWindowHandle,
    Entity<CompareTab>,
    SharedSink,
    [TempDir; 2],
) {
    let left = TempDir::new(&format!("{tag}-l"));
    let right = TempDir::new(&format!("{tag}-r"));
    let opened = shell(cx);
    let sink: SharedSink = Arc::default();
    let hooks = Arc::new(Held::new(Arc::clone(&sink)));
    cx.update(|cx| install_test_hooks(hooks, cx));
    submit(cx, &opened, [left.path(), right.path()]);
    cx.run_until_parked();
    let tab = opened
        .1
        .read_with(cx, |shell, _| shell.active_compare().cloned());
    (opened.0, tab.expect("compare tab"), sink, [left, right])
}

/// Sends one engine update and lets the tab drain it.
fn send(cx: &mut TestAppContext, sink: &SharedSink, update: CompareUpdate) {
    let guard = sink.lock().expect("sink lock");
    (guard.as_ref().expect("engine started"))(update);
    drop(guard);
    cx.run_until_parked();
}

/// Moves the virtual clock past the 1,000 ms timer.
fn pass_timer(cx: &mut TestAppContext) {
    cx.executor().advance_clock(Duration::from_millis(1001));
    cx.run_until_parked();
}

/// The tab's shown view.
fn view(cx: &mut TestAppContext, tab: &Entity<CompareTab>) -> TabView {
    tab.read_with(cx, |tab, _| tab.view())
}

#[gpui_kit::test]
fn still_checking_at_one_second_opens_on_large(cx: &mut TestAppContext) {
    let (_window, tab, _sink, _dirs) = started(cx, "slow");
    assert_eq!(view(cx, &tab), TabView::Folders, "Folders before the timer");
    pass_timer(cx);
    assert_eq!(view(cx, &tab), TabView::Large);
}

#[gpui_kit::test]
fn finishing_before_one_second_stays_on_folders(cx: &mut TestAppContext) {
    let (_window, tab, sink, _dirs) = started(cx, "fast");
    send(cx, &sink, CompareUpdate::Finished { generation: 0 });
    pass_timer(cx);
    assert_eq!(view(cx, &tab), TabView::Folders);
}

#[gpui_kit::test]
fn enter_on_large_shows_folders(cx: &mut TestAppContext) {
    let (window, tab, _sink, _dirs) = started(cx, "enter");
    pass_timer(cx);
    assert_eq!(view(cx, &tab), TabView::Large);
    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    assert_eq!(view(cx, &tab), TabView::Folders);
}

#[gpui_kit::test]
fn a_drain_hands_the_view_a_snapshot(cx: &mut TestAppContext) {
    let (_window, tab, sink, _dirs) = started(cx, "snap");
    let update = CompareUpdate::PhaseProgress {
        generation: 1,
        phase: ComparePhase::Listing,
        done: 3,
        total: None,
    };
    send(cx, &sink, update);
    let large = tab.read_with(cx, |tab, _| tab.large().clone());
    let meters = large.read_with(cx, |large, _| large.snapshot().meters.clone());
    assert_ne!(meters, Default::default());
}

#[gpui_kit::test]
fn finishing_while_on_timer_large_shows_folders(cx: &mut TestAppContext) {
    let (_window, tab, sink, _dirs) = started(cx, "end");
    pass_timer(cx);
    assert_eq!(view(cx, &tab), TabView::Large);
    send(cx, &sink, CompareUpdate::Finished { generation: 0 });
    assert_eq!(view(cx, &tab), TabView::Folders);
}

#[gpui_kit::test]
fn finishing_keeps_a_large_the_user_chose(cx: &mut TestAppContext) {
    let (window, tab, sink, _dirs) = started(cx, "chosen");
    pass_timer(cx);
    cx.simulate_keystrokes(window, "2 5");
    cx.run_until_parked();
    assert_eq!(view(cx, &tab), TabView::Large);
    send(cx, &sink, CompareUpdate::Finished { generation: 0 });
    assert_eq!(view(cx, &tab), TabView::Large);
}
