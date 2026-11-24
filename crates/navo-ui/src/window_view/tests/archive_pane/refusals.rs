//! Archives stay read-only in the pane (NV10b): Ctrl+C and Ctrl+X refuse before the system
//! clipboard, rename and multi-rename refuse in the browser itself, a streamed paste or a drop
//! into an archive is refused, and F5 skips unsafe entries with one notice.

use std::io::{Cursor, Write as _};
use std::sync::Arc;

use gpui_kit::TestAppContext;
use navo_core::{Location, OpKind, ProviderPath};

use super::{beside_options, cursor_name, open_beside, open_options, press, settle, shown};
use crate::browser::{DropConfirm, DropRequest, PasteRefusal, StreamPaste};
use crate::window_view::tests::clipboard::FakeClipboard;
use crate::window_view::{
    ClipboardEffect, ClipboardFiles, FileClipboardService, NavoWindow, StartOptions,
};

const READ_ONLY: &str = "Archives are read-only here. Copy files out with F5.";
const NO_CLIPBOARD: &str = "Archive entries can't go on the clipboard. Copy them out with F5.";

/// The status notice text shown in the window, if any.
fn notice(cx: &mut TestAppContext, view: &gpui_kit::Entity<NavoWindow>) -> Option<String> {
    view.read_with(cx, |view, _| {
        view.clipboard_notice().map(|n| n.text.to_string())
    })
}

#[gpui_kit::test]
fn ctrl_c_and_ctrl_x_on_a_zip_entry_refuse_before_the_clipboard(cx: &mut TestAppContext) {
    let (dir, out) = (
        tempfile::tempdir().expect("dir"),
        tempfile::tempdir().expect("out"),
    );
    let clipboard = Arc::new(FakeClipboard::default());
    let shared: Arc<dyn FileClipboardService> = clipboard.clone();
    let options = StartOptions {
        file_clipboard: Some(shared),
        ..beside_options(dir.path(), out.path())
    };
    let (window, view) = open_options(cx, options);
    press(cx, window, &view, "enter");
    press(cx, window, &view, "down");
    assert_eq!(cursor_name(cx, &view).as_deref(), Some("top.txt"));
    for key in ["ctrl-c", "ctrl-x"] {
        view.update(cx, |view, _| view.forget_clipboard_notice());
        press(cx, window, &view, key);
        settle(cx, &view);
        assert_eq!(notice(cx, &view).as_deref(), Some(NO_CLIPBOARD), "{key}");
    }
    assert!(clipboard.writes.lock().unwrap().is_empty());
    let entry = view.read_with(cx, |view, _| view.browser.selected_sources());
    let refused = clipboard.write(&ClipboardFiles {
        items: entry,
        effect: ClipboardEffect::Copy,
    });
    assert!(refused.is_err(), "the fake must refuse archive entries");
}

#[gpui_kit::test]
fn rename_new_names_and_ctrl_m_inside_a_zip_are_refused(cx: &mut TestAppContext) {
    let (dir, out) = (
        tempfile::tempdir().expect("dir"),
        tempfile::tempdir().expect("out"),
    );
    let (window, view) = open_beside(cx, dir.path(), out.path());
    let zip = dir.path().join("bundle.zip");
    let before = std::fs::read(&zip).expect("zip bytes");
    press(cx, window, &view, "enter");
    let refused = view.update(cx, |view, _| {
        [
            view.browser.rename("top.txt", "renamed.txt"),
            view.browser.new_folder("made"),
            view.browser.new_file("made.txt"),
        ]
        .map(|result| result.map_err(|problem| problem.to_string()))
    });
    assert_eq!(refused, [0, 1, 2].map(|_| Err(READ_ONLY.to_owned())));
    press(cx, window, &view, "ctrl-a");
    view.update(cx, |view, _| view.forget_clipboard_notice());
    press(cx, window, &view, "ctrl-m");
    let opened = view.read_with(cx, |view, _| view.is_multi_rename_open());
    assert!(!opened, "Ctrl+M opened the multi-rename dialog");
    assert_eq!(notice(cx, &view).as_deref(), Some(READ_ONLY));
    settle(cx, &view);
    assert_eq!(std::fs::read(&zip).expect("zip bytes"), before);
    assert_eq!(shown(cx, &view).1, ["docs", "top.txt"]);
}

#[gpui_kit::test]
fn a_streamed_paste_or_a_drop_into_a_zip_is_refused(cx: &mut TestAppContext) {
    let (dir, out) = (
        tempfile::tempdir().expect("dir"),
        tempfile::tempdir().expect("out"),
    );
    let (_, view) = open_beside(cx, dir.path(), out.path());
    let root = navo_fs::archive::location(&dir.path().join("bundle.zip"), "").expect("root");
    let notes = dir.path().join("notes.txt");
    let notes = Location::local(ProviderPath::from_text(notes.to_str().expect("utf-8")));
    let (streamed, dropped, jobs) = view.update(cx, |view, _| {
        let paste = StreamPaste {
            sources: navo_fs::Sources::new(),
            items: Vec::new(),
            refused: Vec::new(),
            items_only: false,
            explain: None,
        };
        let streamed = view.browser.paste_stream(paste, root.clone());
        let dropped = view.browser.drop_into(DropRequest {
            kind: OpKind::Copy,
            sources: vec![notes.expect("notes")],
            target: root,
            confirm: DropConfirm::Never,
            source_tab: None,
        });
        (streamed, dropped, view.browser.jobs().len())
    });
    assert_eq!(streamed, Err(PasteRefusal::ReadOnlyArchive));
    assert_eq!(dropped, Err(PasteRefusal::ReadOnlyArchive));
    assert_eq!(jobs, 0);
}

#[gpui_kit::test]
fn f5_copies_only_the_safe_entries_and_says_how_many_it_skipped(cx: &mut TestAppContext) {
    let (dir, out) = (
        tempfile::tempdir().expect("dir"),
        tempfile::tempdir().expect("out"),
    );
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    for name in ["../evil.txt", "fine.txt"] {
        writer.start_file(name, options).expect("file");
        writer.write_all(b"x").expect("write");
    }
    let archive = dir.path().join("unsafe.zip");
    std::fs::write(&archive, writer.finish().expect("finish").into_inner()).expect("zip");
    let (window, view) = open_beside(cx, dir.path(), out.path());
    let location = navo_fs::archive::location(&archive, "").expect("archive location");
    view.update(cx, |view, _| {
        let navigate = navo_core::Command::PaneNavigate { location };
        view.browser.command(&navigate).expect("navigate");
    });
    settle(cx, &view);
    press(cx, window, &view, "ctrl-a");
    press(cx, window, &view, "f5");
    assert_eq!(
        notice(cx, &view).as_deref(),
        Some("1 item skipped: unsafe name")
    );
    press(cx, window, &view, "enter");
    settle(cx, &view);
    let names: Vec<_> = std::fs::read_dir(out.path())
        .expect("out")
        .map(|entry| entry.expect("entry").file_name())
        .collect();
    assert_eq!(names, ["fine.txt"]);
    let beside = out.path().parent().expect("parent").join("evil.txt");
    assert!(!beside.exists(), "the unsafe entry escaped");
}
