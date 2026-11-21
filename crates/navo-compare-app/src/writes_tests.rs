//! Tests of the app's write wiring (P1 A1): the `Wrote` toast and its Undo, Ctrl+Z in the file
//! diff, the copy host in the diff hooks, and the journal's start-up outcomes.

use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, SharedString, TestAppContext};
use sidewise_host::compare::diff::copy_port::CopyHost;
use sidewise_journal::Recovered;
use sidewise_ui::compare_diff::{
    CompareOptions, CopyChange, CopyDirection, CopyDone, CopyResult, CopySink, DiffHooks, DiffSink,
    PairControl, PairRequest, SheetPlanSink, SheetSink, StreamControl, StreamSink, UndoSink,
    UndoStart,
};
use sidewise_ui::window_view::ScreenEvent;

use crate::compare_tab_script::scripted_shell;
use crate::compare_tab_tests::{TempDir, submit, wait_for_row};
use crate::screens::compare_host::{TEST_DIFF_HOOKS, bind_compare_keys, hooks_with};
use crate::screens::compare_tab::{CompareTab, TabView};
use crate::shell::Shell;
use crate::writes::{open_writes, recovery_note};

pub(crate) type Opened = (AnyWindowHandle, Entity<Shell>);

/// Diff hooks over the quiet test hooks that count `undo_last` calls and always have one.
struct UndoProbe {
    inner: Arc<dyn DiffHooks>,
    undos: Arc<AtomicUsize>,
}

impl DiffHooks for UndoProbe {
    /// Opens the pair on the quiet hooks.
    fn open_pair(
        &self,
        request: PairRequest,
        options: &CompareOptions,
        sink: DiffSink,
    ) -> Box<dyn PairControl> {
        self.inner.open_pair(request, options, sink)
    }

    /// Opens the stream on the quiet hooks.
    fn open_stream(&self, options: &CompareOptions, sink: StreamSink) -> Box<dyn StreamControl> {
        self.inner.open_stream(options, sink)
    }

    /// Counts the call and says an undo started; it never ends.
    fn undo_last(&self, _job: Option<&str>, _done: UndoSink) -> UndoStart {
        self.undos.fetch_add(1, Ordering::SeqCst);
        UndoStart::Started
    }
}

/// A shell with a submitted folder compare whose screens use the undo probe; returns the
/// shell, its compare tab, the undo count and the folders.
fn opened(
    cx: &mut TestAppContext,
    tag: &str,
) -> (Opened, Entity<CompareTab>, Arc<AtomicUsize>, [TempDir; 2]) {
    let left = TempDir::new(&format!("{tag}-l"));
    let right = TempDir::new(&format!("{tag}-r"));
    fs::write(left.path().join("a.txt"), "one").expect("write left");
    fs::write(right.path().join("a.txt"), "two").expect("write right");
    let opened = scripted_shell(cx);
    let undos = Arc::new(AtomicUsize::new(0));
    let inner = TEST_DIFF_HOOKS
        .with_borrow(Clone::clone)
        .expect("quiet hooks");
    let probe = UndoProbe {
        inner,
        undos: Arc::clone(&undos),
    };
    TEST_DIFF_HOOKS.with_borrow_mut(|slot| *slot = Some(Arc::new(probe)));
    submit(cx, &opened, [left.path(), right.path()]);
    assert!(wait_for_row(cx, &opened, "a.txt"), "rows reach the view");
    let tab = opened
        .1
        .read_with(cx, |shell, _| shell.active_compare().cloned())
        .expect("compare tab");
    (opened, tab, undos, [left, right])
}

/// Emits `event` from the tab's compare screen, as a finished copy or undo does.
pub(crate) fn emit(
    cx: &mut TestAppContext,
    (window, _): &Opened,
    tab: &Entity<CompareTab>,
    event: ScreenEvent,
) {
    let screen = tab.read_with(cx, |tab, _| tab.screen().clone());
    cx.update_window(*window, |_, _, cx| {
        screen.update(cx, |_, cx| cx.emit(event));
    })
    .expect("window");
    cx.run_until_parked();
}

/// The texts of the shell's toasts.
pub(crate) fn toasts(cx: &mut TestAppContext, (_, shell): &Opened) -> Vec<SharedString> {
    shell.read_with(cx, |shell, cx| shell.toast_host().read(cx).messages())
}

/// Presses the newest toast's Undo.
pub(crate) fn press_toast_undo(cx: &mut TestAppContext, (_, shell): &Opened) {
    let host = shell.read_with(cx, |shell, _| shell.toast_host());
    cx.update(|cx| host.update(cx, |host, cx| host.undo(cx)));
    cx.run_until_parked();
}

/// A finished copy toasts its text with Undo, and the toast's Undo undoes on the screen.
#[gpui_kit::test]
fn a_copy_toasts_with_undo_and_undo_reaches_the_screen(cx: &mut TestAppContext) {
    let (open, tab, undos, _dirs) = opened(cx, "wrote-undo");
    let copied = ScreenEvent::Wrote {
        text: "Copied to the right".into(),
        job: Some("j1".into()),
        reload: Vec::new(),
    };
    emit(cx, &open, &tab, copied);
    assert_eq!(toasts(cx, &open), ["Copied to the right"]);
    press_toast_undo(cx, &open);
    assert_eq!(undos.load(Ordering::SeqCst), 1, "Undo calls undo_write");
    assert!(toasts(cx, &open).is_empty(), "the Undo toast is dismissed");
}

/// A finished undo toasts its text with no Undo.
#[gpui_kit::test]
fn an_undo_toasts_without_undo(cx: &mut TestAppContext) {
    let (open, tab, undos, _dirs) = opened(cx, "wrote-plain");
    let undone = ScreenEvent::Wrote {
        text: "Undid the copy".into(),
        job: None,
        reload: Vec::new(),
    };
    emit(cx, &open, &tab, undone);
    assert_eq!(toasts(cx, &open), ["Undid the copy"]);
    press_toast_undo(cx, &open);
    assert_eq!(undos.load(Ordering::SeqCst), 0, "the toast has no Undo");
    assert_eq!(toasts(cx, &open), ["Undid the copy"]);
}

/// Ctrl+Z in the file diff asks the hooks to undo the last copy.
#[gpui_kit::test]
fn ctrl_z_in_the_file_diff_undoes(cx: &mut TestAppContext) {
    let (open, tab, undos, _dirs) = opened(cx, "ctrl-z");
    cx.update(bind_compare_keys);
    cx.simulate_keystrokes(open.0, "enter");
    cx.run_until_parked();
    assert_eq!(tab.read_with(cx, |tab, _| tab.view()), TabView::File);
    cx.simulate_keystrokes(open.0, "ctrl-z");
    cx.run_until_parked();
    assert_eq!(undos.load(Ordering::SeqCst), 1);
}

/// A copy host that records the copies it was given and answers each as copied.
#[derive(Default)]
struct RecordingHost {
    copies: Mutex<Vec<u64>>,
}

impl CopyHost for RecordingHost {
    /// Records the pair id and answers `Copied`.
    fn copy_change(&self, change: CopyChange, done: CopySink) {
        self.copies.lock().expect("copies").push(change.pair.id);
        done(CopyDone {
            request: change.pair.id,
            direction: change.direction,
            result: CopyResult::Copied,
            job: None,
        });
    }
    /// Unused here.
    fn copy_changes(&self, _changes: Vec<CopyChange>, _done: CopySink) {}
    /// Unused here.
    fn plan_sheet(&self, _changes: Vec<CopyChange>, _planned: SheetPlanSink) {}
    /// Unused here.
    fn run_sheet(&self, _done: SheetSink) {}
}

/// One change of pair 7, copied to the right.
fn change() -> CopyChange {
    CopyChange {
        pair: PairRequest {
            id: 7,
            path: "a.txt".into(),
            left: None,
            right: None,
        },
        left_bytes: 0..1,
        right_bytes: 0..1,
        digest: [0; 16],
        direction: CopyDirection::ToRight,
        lines: None,
    }
}

/// The answer `hooks` gives for one copy.
fn copy_through(hooks: &dyn DiffHooks) -> CopyResult {
    let answer = Arc::new(Mutex::new(None));
    let sink_answer = Arc::clone(&answer);
    let sink: CopySink = Arc::new(move |done| *sink_answer.lock().expect("answer") = Some(done));
    hooks.copy_change(change(), sink);
    let done = answer.lock().expect("answer").take().expect("one answer");
    done.result
}

/// With a host the diff hooks copy through it; without one they refuse (read-only).
#[test]
fn diff_hooks_copy_through_the_host_or_refuse() {
    let host = Arc::new(RecordingHost::default());
    let with = hooks_with(Some(Arc::clone(&host) as Arc<dyn CopyHost>));
    assert_eq!(copy_through(&with), CopyResult::Copied);
    assert_eq!(*host.copies.lock().expect("copies"), [7]);
    let without = hooks_with(None);
    assert!(matches!(copy_through(&without), CopyResult::Refused(_)));
}

/// A fresh profile opens the journal with nothing to report.
#[test]
fn a_fresh_profile_opens_writes_quietly() {
    let profile = TempDir::new("writes-fresh");
    let (_host, note) = open_writes(Some(profile.path())).expect("journal opens");
    assert_eq!(note, None);
}

/// A journal folder that is a file turns copying off with its reason (fail closed).
#[test]
fn a_journal_folder_that_is_a_file_turns_copying_off() {
    let profile = TempDir::new("writes-off");
    let journal = navo_compare_profile::paths::open_profile(Some(profile.path()))
        .expect("profile")
        .journal_dir();
    fs::remove_dir_all(&journal).expect("remove journal folder");
    fs::write(&journal, b"not a folder").expect("journal file");
    let Err(text) = open_writes(Some(profile.path())) else {
        panic!("a journal folder that is a file must not open");
    };
    let off = sidewise_l10n::t!("compare-writes-off", reason = "");
    assert!(text.starts_with(&off), "{text}");
}

/// The recovery note says a rolled-back copy (once, not beside a restore), counts restored
/// files, then names the journal folder when anything was left, old backups that could not be
/// removed included; a clean recovery says nothing.
#[test]
fn the_recovery_note_counts_and_names_the_folder() {
    use sidewise_journal::Swept;
    use sidewise_l10n::t;
    let dir = std::path::Path::new("/j");
    assert_eq!(recovery_note(&Recovered::default(), dir), None);
    let restored = Recovered {
        restored: 1,
        ..Recovered::default()
    };
    let one = t!("compare-recovery-restored", count = 1);
    assert_eq!(recovery_note(&restored, dir), Some(one));
    let rolled = Recovered {
        rolled_back: 1,
        ..Recovered::default()
    };
    let note = t!("compare-recovery-rolled-back", count = 1);
    assert_eq!(recovery_note(&rolled, dir), Some(note));
    let left = Recovered {
        rolled_back: 2,
        restored: 2,
        kept: 1,
        failed: 2,
        damaged: 1,
        swept: Swept {
            failed: 3,
            ..Swept::default()
        },
        ..Recovered::default()
    };
    let parts = [
        t!("compare-recovery-restored", count = 2),
        t!("compare-recovery-kept", count = 1),
        t!("compare-recovery-failed", count = 2),
        t!("compare-recovery-damaged", count = 1),
        t!("compare-recovery-sweep-failed", count = 3),
        t!("compare-recovery-folder", folder = "/j"),
    ];
    assert_eq!(recovery_note(&left, dir), Some(parts.join(" ")));
    let swept = Recovered {
        swept: Swept {
            failed: 1,
            ..Swept::default()
        },
        ..Recovered::default()
    };
    let note = recovery_note(&swept, dir).expect("a failed sweep is reported");
    assert!(
        note.starts_with(&t!("compare-recovery-sweep-failed", count = 1)),
        "{note}"
    );
}

/// A rolled-back undo is named as an undo, never as a copy: alone, with restored files, and
/// beside a rolled-back copy; each kind shows its rolled-back line only without its restores.
#[test]
fn the_recovery_note_names_undos_apart_from_copies() {
    use sidewise_l10n::t;
    let dir = std::path::Path::new("/j");
    let undo = Recovered {
        rolled_back: 1,
        rolled_back_undos: 1,
        ..Recovered::default()
    };
    let note = t!("compare-recovery-undo-rolled-back", count = 1);
    assert!(note.contains("undo"), "{note}");
    assert_eq!(recovery_note(&undo, dir), Some(note));
    let undo_restored = Recovered {
        rolled_back: 2,
        rolled_back_undos: 2,
        restored: 3,
        restored_undos: 3,
        ..Recovered::default()
    };
    let note = t!("compare-recovery-restored-undo", count = 3);
    assert!(note.contains("undo"), "{note}");
    assert_eq!(recovery_note(&undo_restored, dir), Some(note));
    let mixed = Recovered {
        rolled_back: 3,
        rolled_back_undos: 1,
        restored: 2,
        restored_undos: 0,
        ..Recovered::default()
    };
    let parts = [
        t!("compare-recovery-restored", count = 2),
        t!("compare-recovery-undo-rolled-back", count = 1),
    ];
    assert_eq!(recovery_note(&mixed, dir), Some(parts.join(" ")));
    let both_restored = Recovered {
        rolled_back: 2,
        rolled_back_undos: 1,
        restored: 5,
        restored_undos: 4,
        ..Recovered::default()
    };
    let parts = [
        t!("compare-recovery-restored", count = 1),
        t!("compare-recovery-restored-undo", count = 4),
    ];
    assert_eq!(recovery_note(&both_restored, dir), Some(parts.join(" ")));
}
