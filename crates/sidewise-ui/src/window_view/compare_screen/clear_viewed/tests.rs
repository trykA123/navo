//! Tests of "Clear all viewed marks": the action clears the store through the hooks, empties
//! the Stream's marks and counter, and tells the window.

use std::sync::{Arc, Mutex};

use gpui_kit::{AnyWindowHandle, AppContext, Entity, TestAppContext};

use crate::browser::{CompareKind, CompareRow, CompareSide, RowStatus, Waker};
use crate::commands::compare::ClearViewed;
use crate::compare_diff::fake::FakeDiffHooks;
use crate::compare_diff::{CompareOptions, StreamUpdate, stream_items};
use crate::window_view::{CompareFiles, CompareScreen, DiffSlot, ScreenEvent};

type Seen = Arc<Mutex<Vec<ScreenEvent>>>;

/// Two differing files, ids 1 and 2.
fn two_rows() -> Vec<CompareRow> {
    let side = || {
        Some(CompareSide {
            kind: CompareKind::File,
            size: 1,
            modified_ms: None,
        })
    };
    (1..=2)
        .map(|id| {
            let pair = (side(), side());
            CompareRow::new(id, format!("f{id}.txt"), RowStatus::Different, pair)
        })
        .collect()
}

/// A Stream screen over a fake with two files, the first one marked viewed.
fn stream_with_mark(
    cx: &mut TestAppContext,
) -> (
    Arc<FakeDiffHooks>,
    AnyWindowHandle,
    Entity<CompareScreen>,
    Seen,
) {
    let fake = Arc::new(FakeDiffHooks::default());
    let (hooks, wake): (_, Waker) = (Arc::clone(&fake), Arc::new(|| {}));
    let (window, screen) = cx.update(|cx| {
        gpui_kit::init(cx);
        crate::tokens::apply(crate::tokens::Mode::Light, cx);
        gpui_kit::open_window(Default::default(), cx, |_, cx| {
            cx.new(|_| {
                CompareScreen::new(
                    DiffSlot::new(Some(hooks), wake),
                    CompareOptions::default(),
                    CompareFiles::none(),
                )
            })
        })
        .expect("open test window")
    });
    let seen: Seen = Seen::default();
    let sink = Arc::clone(&seen);
    cx.update(|cx| {
        cx.subscribe(&screen, move |_, event: &ScreenEvent, _| {
            sink.lock().expect("events").push(event.clone());
        })
        .detach();
    });
    let rows = two_rows();
    cx.update(|cx| {
        screen.update(cx, |s, cx| {
            let list = stream_items(&rows, "");
            s.start_stream((list, Vec::new()), cx);
            if let Some(stream) = s.stream_mut() {
                stream.model.apply(&StreamUpdate::Viewed {
                    id: 1,
                    viewed: true,
                });
            }
        });
    });
    (fake, window, screen, seen)
}

/// The Stream's "Viewed n of N" counter.
fn counter(cx: &mut TestAppContext, screen: &Entity<CompareScreen>) -> (usize, usize) {
    screen.read_with(cx, |s, _| s.stream_probe().expect("stream").0.counter())
}

/// The action clears the store, drops the counter to 0 and emits one `ViewedCleared`.
#[gpui_kit::test]
fn clear_action_empties_the_store_and_the_counter(cx: &mut TestAppContext) {
    let (fake, window, screen, seen) = stream_with_mark(cx);
    assert_eq!(counter(cx, &screen), (1, 2));
    cx.update(|cx| {
        window
            .update(cx, |_, window, cx| {
                screen.update(cx, |s, cx| {
                    let focus = s.stream_mut().expect("stream").focus.clone();
                    window.focus(&focus, cx);
                });
                window.dispatch_action(Box::new(ClearViewed), cx);
            })
            .expect("window");
    });
    cx.run_until_parked();
    assert_eq!(fake.clears(), 1);
    assert_eq!(counter(cx, &screen), (0, 2));
    assert_eq!(
        *seen.lock().expect("events"),
        [ScreenEvent::ViewedCleared {
            text: "Cleared 2 viewed marks.".into(),
            cleared: true
        }]
    );
}
