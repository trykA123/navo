//! Tests of the undo seam: the hooks' `undo_last`, the running note, the refresh of the open
//! file and the `Wrote` event.

use std::sync::{Arc, Mutex};

use gpui_kit::{AnyWindowHandle, AppContext, Entity, TestAppContext};
use navo_core::Location;

use crate::browser::Waker;
use crate::compare_diff::fake::FakeDiffHooks;
use crate::compare_diff::{CompareOptions, CopyResult, PairRequest, UndoDone};
use crate::window_view::{CompareFiles, CompareScreen, DiffSlot, ScreenEvent};

type Seen = Arc<Mutex<Vec<ScreenEvent>>>;

fn place(name: &str) -> Location {
    Location::new(format!("/work/{name}")).expect("location")
}

/// A screen over a fake with `a.txt` against `b.txt` open, and the events it emits.
fn screen_with_pair(
    cx: &mut TestAppContext,
) -> (
    Arc<FakeDiffHooks>,
    AnyWindowHandle,
    Entity<CompareScreen>,
    Seen,
) {
    let fake = Arc::new(FakeDiffHooks::default());
    let wake: Waker = Arc::new(|| {});
    let hooks = Arc::clone(&fake);
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
    let request = PairRequest {
        id: 0,
        path: "a.txt".into(),
        left: Some(place("a.txt")),
        right: Some(place("b.txt")),
    };
    cx.update(|cx| {
        window
            .update(cx, |_, window, cx| {
                screen.update(cx, |s, cx| s.show_pair((request, None), window, cx))
            })
            .expect("window");
    });
    (fake, window, screen, seen)
}

fn undo(cx: &mut TestAppContext, screen: &Entity<CompareScreen>) -> bool {
    cx.update(|cx| {
        screen.update(cx, |s, cx| {
            s.start_undo(None, cx) != crate::compare_diff::UndoStart::Nothing
        })
    })
}

fn poll(cx: &mut TestAppContext, screen: &Entity<CompareScreen>) -> bool {
    cx.update(|cx| screen.update(cx, |s, cx| s.poll_compare_screen(false, cx)))
}

/// An undo with nothing to undo returns false, so the key propagates, and shows no note.
#[gpui_kit::test]
fn nothing_to_undo_propagates(cx: &mut TestAppContext) {
    let (fake, _, screen, seen) = screen_with_pair(cx);
    assert!(!undo(cx, &screen));
    assert_eq!(fake.undo_calls(), 1);
    assert!(screen.read_with(cx, |s, _| s.copy_note().is_none()));
    assert!(seen.lock().expect("events").is_empty());
}

/// An undo that ends reloads the open file whose side was a target, and emits `Wrote`.
#[gpui_kit::test]
fn undone_copy_refreshes_the_open_file_and_emits_wrote(cx: &mut TestAppContext) {
    let (fake, _, screen, seen) = screen_with_pair(cx);
    fake.set_undoable(true);
    let before = fake.refreshes().len();
    assert!(undo(cx, &screen));
    (fake.undo_sink())(UndoDone {
        targets: vec![place("b.txt")],
        result: CopyResult::Copied,
    });
    assert!(poll(cx, &screen));
    assert_eq!(fake.refreshes().len(), before + 1);
    let note = screen.read_with(cx, |s, _| s.copy_note());
    assert_eq!(note, Some(("Undid the copy".into(), false, false)));
    assert_eq!(
        *seen.lock().expect("events"),
        [ScreenEvent::Wrote {
            text: "Undid the copy".into(),
            job: None,
            reload: vec![place("b.txt")],
        }]
    );
}

/// An undo of files the open pair does not show reloads nothing.
#[gpui_kit::test]
fn undo_of_other_files_leaves_the_open_file(cx: &mut TestAppContext) {
    let (fake, _, screen, _) = screen_with_pair(cx);
    fake.set_undoable(true);
    let before = fake.refreshes().len();
    assert!(undo(cx, &screen));
    (fake.undo_sink())(UndoDone {
        targets: vec![place("other.txt")],
        result: CopyResult::Copied,
    });
    assert!(poll(cx, &screen));
    assert_eq!(fake.refreshes().len(), before);
}

/// A refused undo shows the refusal text as an error and reloads nothing.
#[gpui_kit::test]
fn refused_undo_shows_the_refusal(cx: &mut TestAppContext) {
    let (fake, _, screen, seen) = screen_with_pair(cx);
    fake.set_undoable(true);
    let before = fake.refreshes().len();
    assert!(undo(cx, &screen));
    (fake.undo_sink())(UndoDone {
        targets: vec![place("b.txt")],
        result: CopyResult::Refused("no".into()),
    });
    assert!(poll(cx, &screen));
    assert_eq!(fake.refreshes().len(), before);
    let note = screen.read_with(cx, |s, _| s.copy_note());
    assert_eq!(note, Some(("no".into(), false, true)));
    assert_eq!(seen.lock().expect("events").len(), 1);
}

/// While an undo runs, a second one says a copy is running and does not call the hooks.
#[gpui_kit::test]
fn undo_in_flight_shows_the_running_note(cx: &mut TestAppContext) {
    let (fake, _, screen, _) = screen_with_pair(cx);
    fake.set_undoable(true);
    assert!(undo(cx, &screen));
    assert!(undo(cx, &screen));
    assert_eq!(fake.undo_calls(), 1);
    let note = screen.read_with(cx, |s, _| s.copy_note()).expect("note");
    assert_eq!(note.0, "A copy is still running. Wait for it to end.");
}

/// A finished copy emits `Wrote` with Undo and its target, so other tabs reload it; its note
/// has no Undo button.
#[gpui_kit::test]
fn finished_copy_emits_wrote_with_undo(cx: &mut TestAppContext) {
    use crate::compare_diff::{CopyDirection, CopyDone};
    let (fake, _, screen, seen) = screen_with_pair(cx);
    cx.update(|cx| {
        screen.update(cx, |s, cx| {
            s.copy_change_to(CopyDirection::ToRight, None, cx)
        });
    });
    // No change is picked, so nothing starts; push the end of a copy as the app would.
    cx.update(|cx| {
        screen.update(cx, |s, cx| {
            s.compare.diff.copy.inflight = true;
            cx.notify();
        });
    });
    let sink = cx.update(|cx| screen.update(cx, |s, _| s.compare.diff.copy_sink()));
    sink(CopyDone {
        request: 0,
        direction: CopyDirection::ToRight,
        result: CopyResult::Copied,
        job: Some("j1".into()),
    });
    assert!(poll(cx, &screen));
    let note = screen.read_with(cx, |s, _| s.copy_note());
    assert_eq!(note, Some(("Copied to the right".into(), false, false)));
    assert_eq!(
        *seen.lock().expect("events"),
        [ScreenEvent::Wrote {
            text: "Copied to the right".into(),
            job: Some("j1".into()),
            reload: vec![place("b.txt")],
        }]
    );
    let _ = fake;
}

/// An undo that may have written (NC168) reloads the target, shows the text as an error and
/// still tells the window.
#[gpui_kit::test]
fn unconfirmed_undo_refreshes_the_target(cx: &mut TestAppContext) {
    let (fake, _, screen, seen) = screen_with_pair(cx);
    fake.set_undoable(true);
    let before = fake.refreshes().len();
    assert!(undo(cx, &screen));
    (fake.undo_sink())(UndoDone {
        targets: vec![place("b.txt")],
        result: CopyResult::Unconfirmed("unsure".into()),
    });
    assert!(poll(cx, &screen));
    assert_eq!(fake.refreshes().len(), before + 1, "the target is reloaded");
    let note = screen.read_with(cx, |s, _| s.copy_note());
    assert_eq!(note, Some(("unsure".into(), false, true)));
    assert_eq!(seen.lock().expect("events").len(), 1);
}

/// A copy that may have written (NC168) reloads its pair, shows the text as an error, and
/// emits `Wrote` with its target but no Undo, so other tabs reload it too.
#[gpui_kit::test]
fn unconfirmed_copy_refreshes_the_pair(cx: &mut TestAppContext) {
    use crate::compare_diff::{CopyDirection, CopyDone};
    let (fake, _, screen, seen) = screen_with_pair(cx);
    let before = fake.refreshes().len();
    let sink = cx.update(|cx| screen.update(cx, |s, _| s.compare.diff.copy_sink()));
    sink(CopyDone {
        request: 0,
        direction: CopyDirection::ToLeft,
        result: CopyResult::Unconfirmed("unsure".into()),
        job: None,
    });
    assert!(poll(cx, &screen));
    assert_eq!(fake.refreshes().len(), before + 1, "the pair is reloaded");
    let note = screen.read_with(cx, |s, _| s.copy_note());
    assert_eq!(note, Some(("unsure".into(), false, true)));
    assert_eq!(
        *seen.lock().expect("events"),
        [ScreenEvent::Wrote {
            text: "unsure".into(),
            job: None,
            reload: vec![place("a.txt")],
        }]
    );
}
