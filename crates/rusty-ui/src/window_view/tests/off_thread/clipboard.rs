use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use gpui_kit::AppContext as _;
use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};
use rusty_fs::Provider as _;

use super::{SLOW, key};
use crate::window_view::tests::clipboard::FakeClipboard;
use crate::window_view::tests::{fake, home_beside_dest, open_options, options_for, press, settle};
use crate::window_view::{
    ClipboardEffect, ClipboardFault, ClipboardFiles, FerriteWindow, FileClipboardService,
};

#[derive(Default)]
struct SlowClipboard {
    inner: FakeClipboard,
    reads: Mutex<VecDeque<ClipboardFiles>>,
    calls: Mutex<Vec<&'static str>>,
}

impl FileClipboardService for SlowClipboard {
    fn write(&self, files: &ClipboardFiles) -> Result<(), ClipboardFault> {
        std::thread::sleep(SLOW);
        self.calls.lock().unwrap().push("write");
        self.inner.write(files)
    }

    fn read(&self) -> Result<Option<ClipboardFiles>, ClipboardFault> {
        std::thread::sleep(SLOW);
        self.calls.lock().unwrap().push("read");
        let queued = self.reads.lock().unwrap().pop_front();
        queued
            .map(Some)
            .map(Ok)
            .unwrap_or_else(|| self.inner.read())
    }

    fn has_files(&self) -> bool {
        self.inner.has_files()
    }

    fn clear_if_same(&self, files: &ClipboardFiles) -> Result<bool, ClipboardFault> {
        std::thread::sleep(SLOW);
        self.calls.lock().unwrap().push("clear");
        self.inner.clear_if_same(files)
    }
}

fn open(
    cx: &mut TestAppContext,
    clipboard: Arc<SlowClipboard>,
) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let fs = fake();
    let mut options = options_for(fs.clone(), home_beside_dest(&fs));
    options.file_clipboard = Some(clipboard);
    open_options(cx, options)
}

#[gpui_kit::test]
fn copy_cut_and_paste_return_at_once_and_apply_in_order(cx: &mut TestAppContext) {
    let clipboard = Arc::new(SlowClipboard::default());
    let (window, view) = open(cx, clipboard.clone());
    press(cx, window, &view, "down");
    for shortcut in ["ctrl-c", "ctrl-x", "tab", "ctrl-v"] {
        key(cx, window, shortcut);
    }
    assert!(clipboard.calls.lock().unwrap().is_empty());
    settle(cx, &view);
    let writes = clipboard.inner.writes.lock().unwrap();
    assert_eq!(
        writes.iter().map(|files| files.effect).collect::<Vec<_>>(),
        [ClipboardEffect::Copy, ClipboardEffect::Move]
    );
    assert_eq!(
        *clipboard.calls.lock().unwrap(),
        ["write", "write", "read", "clear"]
    );
    assert_eq!(*clipboard.inner.held.lock().unwrap(), None);
    view.read_with(cx, |view, _| {
        let jobs = view.browser.jobs();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].status, crate::browser::JobStatus::Done);
    });
}

#[gpui_kit::test]
fn two_pending_pastes_use_worker_files_in_order_at_the_requested_folder(cx: &mut TestAppContext) {
    let clipboard = Arc::new(SlowClipboard::default());
    let fs = fake();
    *clipboard.reads.lock().unwrap() = ["home/a.txt", "home/docs/report.txt"]
        .map(|path| ClipboardFiles {
            items: vec![fs.location(path)],
            effect: ClipboardEffect::Copy,
        })
        .into();
    let mut options = options_for(fs.clone(), home_beside_dest(&fs));
    options.file_clipboard = Some(clipboard.clone());
    let (window, view) = open_options(cx, options);
    press(cx, window, &view, "tab");
    key(cx, window, "ctrl-v");
    key(cx, window, "ctrl-v");
    key(cx, window, "tab");
    assert!(clipboard.calls.lock().unwrap().is_empty());
    settle(cx, &view);
    view.read_with(cx, |view, _| {
        let jobs = view.browser.jobs();
        assert_eq!(
            jobs.iter().map(|job| job.from.clone()).collect::<Vec<_>>(),
            [Some(fs.location("home")), Some(fs.location("home/docs"))]
        );
        assert!(
            jobs.iter()
                .all(|job| job.target == Some(fs.location("dest")))
        );
    });
    assert!(fs.metadata(&fs.location("dest/a.txt")).is_ok());
    assert!(fs.metadata(&fs.location("dest/report.txt")).is_ok());
}

#[gpui_kit::test]
fn closing_a_window_with_clipboard_work_pending_discards_the_reply(cx: &mut TestAppContext) {
    let clipboard = Arc::new(SlowClipboard::default());
    let (window, view) = open(cx, clipboard.clone());
    key(cx, window, "ctrl-c");
    assert!(view.read_with(cx, |view, _| view.lane.is_pending()));
    cx.update_window(window, |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();
    assert!(cx.update_window(window, |_, _, _| ()).is_err());
    assert_eq!(*clipboard.calls.lock().unwrap(), ["write"]);
}

#[gpui_kit::test]
fn copy_path_waits_for_the_earlier_file_clipboard_write(cx: &mut TestAppContext) {
    let clipboard = Arc::new(SlowClipboard::default());
    let (window, view) = open(cx, clipboard.clone());
    press(cx, window, &view, "down");
    key(cx, window, "ctrl-c");
    key(cx, window, "f9");
    assert!(
        cx.read_from_clipboard().is_none(),
        "path writes must wait for file writes"
    );
    settle(cx, &view);
    assert_eq!(*clipboard.calls.lock().unwrap(), ["write"]);
    assert!(
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .is_some_and(|text| text.ends_with("a.txt"))
    );
    assert_eq!(
        view.read_with(cx, |view, _| view
            .clipboard_notice()
            .unwrap()
            .text
            .to_string()),
        "Copied path"
    );
}
