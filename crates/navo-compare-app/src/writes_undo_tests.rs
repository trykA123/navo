//! Tests of the copy toast's Undo by job id (NC166) and of the reload fan-out after an undo
//! (NC148): an older or undone copy's Undo undoes nothing, a closed tab's Undo says so, and
//! every other compare tab reloads the files an undo rewrote.

use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use gpui_kit::{AppContext as _, Entity, SharedString, TestAppContext};
use sidewise_l10n::t;
use sidewise_ui::compare_diff::{
    CompareOptions, DiffHooks, DiffSink, PairControl, PairRequest, StreamControl, StreamSink,
    UndoSink, UndoStart,
};
use sidewise_ui::window_view::ScreenEvent;

use crate::compare_tab_script::scripted_shell;
use crate::compare_tab_tests::{TempDir, submit, wait_for_row};
use crate::screens::compare_host::{TEST_DIFF_HOOKS, bind_compare_keys};
use crate::screens::compare_tab::{CompareTab, TabView};
use crate::writes_tests::{Opened, emit, press_toast_undo, toasts};

/// What the probe saw: the jobs asked to undo, the undos started, the newest job, and each
/// opened pair with its refresh count.
#[derive(Default)]
struct Seen {
    asked: Mutex<Vec<Option<String>>>,
    undos: AtomicUsize,
    top: Mutex<Option<String>>,
    opened: Mutex<Vec<(PairRequest, Arc<AtomicUsize>)>>,
}

/// Diff hooks over the quiet test hooks with an undo stack whose newest job is `seen.top`.
struct Probe {
    inner: Arc<dyn DiffHooks>,
    seen: Arc<Seen>,
}

/// A pair control that counts reloads.
struct Counting {
    inner: Box<dyn PairControl>,
    refreshes: Arc<AtomicUsize>,
}

impl PairControl for Counting {
    /// Passes on.
    fn spans(&self, rows: std::ops::Range<u32>) {
        self.inner.spans(rows);
    }

    /// Counts the reload and passes on.
    fn refresh(&self, request: u64) {
        self.refreshes.fetch_add(1, Ordering::SeqCst);
        self.inner.refresh(request);
    }
}

impl DiffHooks for Probe {
    /// Opens the pair on the quiet hooks and records it with a reload counter.
    fn open_pair(
        &self,
        request: PairRequest,
        options: &CompareOptions,
        sink: DiffSink,
    ) -> Box<dyn PairControl> {
        let refreshes = Arc::new(AtomicUsize::new(0));
        let pair = (request.clone(), Arc::clone(&refreshes));
        self.seen.opened.lock().expect("opened").push(pair);
        let inner = self.inner.open_pair(request, options, sink);
        Box::new(Counting { inner, refreshes })
    }

    /// Opens the stream on the quiet hooks.
    fn open_stream(&self, options: &CompareOptions, sink: StreamSink) -> Box<dyn StreamControl> {
        self.inner.open_stream(options, sink)
    }

    /// Records `job`; refuses it as gone on an empty stack, as not newest when another is on
    /// top, else counts an undo that never ends.
    fn undo_last(&self, job: Option<&str>, _done: UndoSink) -> UndoStart {
        self.seen
            .asked
            .lock()
            .expect("asked")
            .push(job.map(str::to_owned));
        let top = self.seen.top.lock().expect("top").clone();
        if job.is_some() && top.is_none() {
            return UndoStart::Gone;
        }
        if job.is_some() && job != top.as_deref() {
            return UndoStart::NotNewest;
        }
        self.seen.undos.fetch_add(1, Ordering::SeqCst);
        UndoStart::Started
    }
}

/// A shell whose screens use the probe, with a folder compare of `a.txt` submitted and opened
/// in the file diff; returns the shell, the tab, what the probe saw and the folders.
fn opened(
    cx: &mut TestAppContext,
    tag: &str,
) -> (Opened, Entity<CompareTab>, Arc<Seen>, [TempDir; 2]) {
    let opened = scripted_shell(cx);
    let seen = Arc::new(Seen::default());
    let inner = TEST_DIFF_HOOKS
        .with_borrow(Clone::clone)
        .expect("quiet hooks");
    let probe = Probe {
        inner,
        seen: Arc::clone(&seen),
    };
    TEST_DIFF_HOOKS.with_borrow_mut(|slot| *slot = Some(Arc::new(probe)));
    cx.update(bind_compare_keys);
    let dirs = open_file_tab(cx, &opened, tag);
    let tab = active(cx, &opened);
    (opened, tab, seen, dirs)
}

/// Submits a folder compare of `a.txt` in the active start tab and opens the file diff.
fn open_file_tab(cx: &mut TestAppContext, opened: &Opened, tag: &str) -> [TempDir; 2] {
    let left = TempDir::new(&format!("{tag}-l"));
    let right = TempDir::new(&format!("{tag}-r"));
    fs::write(left.path().join("a.txt"), "one").expect("write left");
    fs::write(right.path().join("a.txt"), "two").expect("write right");
    submit(cx, opened, [left.path(), right.path()]);
    assert!(wait_for_row(cx, opened, "a.txt"), "rows reach the view");
    cx.simulate_keystrokes(opened.0, "enter");
    cx.run_until_parked();
    let view = active(cx, opened).read_with(cx, |tab, _| tab.view());
    assert_eq!(view, TabView::File);
    [left, right]
}

/// The active compare tab.
fn active(cx: &mut TestAppContext, (_, shell): &Opened) -> Entity<CompareTab> {
    shell
        .read_with(cx, |shell, _| shell.active_compare().cloned())
        .expect("compare tab")
}

/// A finished copy of `job`, as the screen emits it.
fn copied(job: &str) -> ScreenEvent {
    ScreenEvent::Wrote {
        text: "Copied to the right".into(),
        job: Some(job.into()),
        reload: Vec::new(),
    }
}

/// The newest toast's text.
fn last_toast(cx: &mut TestAppContext, open: &Opened) -> Option<SharedString> {
    toasts(cx, open).last().cloned()
}

/// Copy j1, then a newer copy j2 (another tab): j1's toast Undo undoes nothing and says why.
#[gpui_kit::test]
fn an_older_copy_toast_does_not_undo_the_newer_copy(cx: &mut TestAppContext) {
    let (open, tab, seen, _dirs) = opened(cx, "undo-older");
    emit(cx, &open, &tab, copied("j1"));
    *seen.top.lock().expect("top") = Some("j2".into());
    press_toast_undo(cx, &open);
    assert_eq!(*seen.asked.lock().expect("asked"), [Some("j1".to_owned())]);
    assert_eq!(seen.undos.load(Ordering::SeqCst), 0, "nothing undone");
    let expected = t!("compare-undo-not-newest");
    assert_eq!(last_toast(cx, &open).as_deref(), Some(expected.as_str()));
}

/// Copy j1, then Ctrl+Z undid it: j1's toast Undo undoes nothing and says it is undone.
#[gpui_kit::test]
fn an_undone_copy_toast_says_it_is_already_undone(cx: &mut TestAppContext) {
    let (open, tab, seen, _dirs) = opened(cx, "undo-gone");
    emit(cx, &open, &tab, copied("j1"));
    press_toast_undo(cx, &open);
    assert_eq!(seen.undos.load(Ordering::SeqCst), 0, "nothing undone");
    let expected = t!("compare-undo-gone");
    assert_eq!(last_toast(cx, &open).as_deref(), Some(expected.as_str()));
}

/// The newest copy's toast Undo undoes it.
#[gpui_kit::test]
fn the_newest_copy_toast_undoes_it(cx: &mut TestAppContext) {
    let (open, tab, seen, _dirs) = opened(cx, "undo-newest");
    emit(cx, &open, &tab, copied("j1"));
    *seen.top.lock().expect("top") = Some("j1".into());
    press_toast_undo(cx, &open);
    assert_eq!(seen.undos.load(Ordering::SeqCst), 1);
}

/// A toast whose tab has closed undoes nothing and points to Ctrl+Z.
#[gpui_kit::test]
fn a_closed_tab_toast_points_to_ctrl_z(cx: &mut TestAppContext) {
    let (open, tab, seen, _dirs) = opened(cx, "undo-closed");
    emit(cx, &open, &tab, copied("j1"));
    *seen.top.lock().expect("top") = Some("j1".into());
    drop(tab);
    let (window, shell) = &open;
    cx.update_window(*window, |_, window, cx| {
        shell.update(cx, |shell, cx| shell.close_tab(shell.active(), window, cx));
    })
    .expect("window");
    cx.run_until_parked();
    press_toast_undo(cx, &open);
    assert_eq!(seen.undos.load(Ordering::SeqCst), 0);
    let expected = t!("compare-undo-tab-closed");
    assert_eq!(last_toast(cx, &open).as_deref(), Some(expected.as_str()));
}

/// An undo that ends in tab A reloads the rewritten file in tab B too.
#[gpui_kit::test]
fn an_undo_in_one_tab_reloads_the_file_in_another(cx: &mut TestAppContext) {
    let (open, tab_a, seen, _dirs_a) = opened(cx, "fan-a");
    let (window, shell) = &open;
    cx.update_window(*window, |_, window, cx| {
        shell.update(cx, |shell, cx| shell.new_tab(window, cx));
    })
    .expect("window");
    let _dirs_b = open_file_tab(cx, &open, "fan-b");
    let (pair_b, count_b) = seen
        .opened
        .lock()
        .expect("opened")
        .last()
        .cloned()
        .expect("B");
    let count_a = Arc::clone(&seen.opened.lock().expect("opened")[0].1);
    let (a_before, b_before) = (
        count_a.load(Ordering::SeqCst),
        count_b.load(Ordering::SeqCst),
    );
    let undone = ScreenEvent::Wrote {
        text: "Undid the copy".into(),
        job: None,
        reload: vec![pair_b.right.clone().expect("right side")],
    };
    emit(cx, &open, &tab_a, undone);
    assert_eq!(count_b.load(Ordering::SeqCst), b_before + 1, "B reloads");
    assert_eq!(
        count_a.load(Ordering::SeqCst),
        a_before,
        "A reloaded itself already"
    );
}
