use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::{Entity, TestAppContext};
use rusty_core::marks::{Bookmarks, MarkSet};

use super::search::{editor_view, search_for};
use super::{editor_of, editor_text, press_only, redraw, text_of, window_with};
use crate::window_view::FerriteWindow;
use crate::window_view::tests::{fake, press, settle, type_in, viewer_open};
use crate::window_view::viewer::{FileHooks, FileStamp, FileText, Head, Kinds, SaveError};

#[derive(Default)]
struct Disk {
    text: String,
    stamp: i64,
    held: bool,
    pending: Option<(String, i64)>,
    writes: usize,
}

type Shared = Arc<Mutex<Disk>>;

fn stamp_of(text: &str, stamp: i64) -> FileStamp {
    FileStamp {
        len: text.len() as u64,
        modified_ms: stamp,
    }
}

fn disk_hooks(disk: &Shared) -> FileHooks {
    let (head, read, write) = (disk.clone(), disk.clone(), disk.clone());
    FileHooks {
        head: Arc::new(move |_| {
            let disk = head.lock().expect("disk");
            Ok(Head {
                bytes: disk.text.as_bytes().to_vec(),
                len: disk.text.len() as u64,
            })
        }),
        read: Arc::new(move |_| {
            let disk = read.lock().expect("disk");
            Ok(FileText {
                stamp: Some(stamp_of(&disk.text, disk.stamp)),
                ..FileText::plain(disk.text.clone())
            })
        }),
        write: Arc::new(move |_, save| {
            let mut disk = write.lock().expect("disk");
            if save
                .expected
                .is_some_and(|stamp| stamp.modified_ms != disk.stamp)
            {
                return Err(SaveError::Changed);
            }
            disk.writes += 1;
            let next = disk.stamp + 1;
            if disk.held {
                disk.pending = Some((save.text.to_owned(), next));
            } else {
                (disk.text, disk.stamp) = (save.text.to_owned(), next);
            }
            Ok(Some(stamp_of(save.text, next)))
        }),
        ..FileHooks::none()
    }
}

fn release(disk: &Shared) {
    let mut disk = disk.lock().expect("disk");
    disk.held = false;
    if let Some((text, stamp)) = disk.pending.take() {
        (disk.text, disk.stamp) = (text, stamp);
    }
}

#[gpui_kit::test]
fn a_save_that_lands_after_a_kind_switch_never_updates_the_new_editor(cx: &mut TestAppContext) {
    let disk = Shared::new(Mutex::new(Disk {
        text: "hello\n".to_owned(),
        stamp: 1,
        ..Disk::default()
    }));
    let (window, view) = window_with(cx, fake(), disk_hooks(&disk), Kinds::builtin());
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    press(cx, window, &view, "ctrl-e");
    type_in(cx, window, &view, "x");
    disk.lock().expect("disk").held = true;
    for key in ["ctrl-s", "alt-3", "alt-3", "alt-1"] {
        press_only(cx, window, key);
    }
    settle(cx, &view);
    redraw(cx, window);
    assert_eq!(editor_text(cx, &view), "hello\n");
    release(&disk);
    press(cx, window, &view, "ctrl-e");
    type_in(cx, window, &view, "y");
    press(cx, window, &view, "ctrl-s");
    redraw(cx, window);
    assert!(text_of(cx, window, "viewer-message").starts_with("Changed on disk"));
    let disk = disk.lock().expect("disk");
    assert_eq!((disk.writes, disk.text.as_str()), (1, "xhello\n"));
}

#[gpui_kit::test]
fn marks_save_again_under_the_new_stamp_after_ctrl_s(cx: &mut TestAppContext) {
    let disk = Shared::new(Mutex::new(Disk {
        text: "hello\n".to_owned(),
        stamp: 1,
        ..Disk::default()
    }));
    let stamps = Arc::new(Mutex::new(Vec::new()));
    let saved = stamps.clone();
    let files = FileHooks {
        save_memory: Arc::new(move |_, stamp, _, _| {
            saved.lock().expect("stamps").push(stamp.modified_ms);
        }),
        ..disk_hooks(&disk)
    };
    let (window, view) = window_with(cx, fake(), files, Kinds::builtin());
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    search_for(cx, window, &view, "hello");
    press(cx, window, &view, "ctrl-3");
    press(cx, window, &view, "escape");
    cx.executor().advance_clock(Duration::from_secs(1));
    settle(cx, &view);
    press(cx, window, &view, "ctrl-e");
    press(cx, window, &view, "ctrl-s");
    cx.executor().advance_clock(Duration::from_secs(1));
    settle(cx, &view);
    assert_eq!(*stamps.lock().expect("stamps"), [1, 2]);
}

type Store = Arc<Mutex<Option<(FileStamp, Bookmarks)>>>;

fn stamped_store(disk: &Shared, store: &Store) -> FileHooks {
    let (loader, saver) = (store.clone(), store.clone());
    FileHooks {
        memory: Arc::new(move |_, stamp| {
            let (saved, bookmarks) = loader.lock().expect("store").clone()?;
            let bookmarks = if saved == stamp {
                bookmarks
            } else {
                Bookmarks::default()
            };
            Some((MarkSet::default(), bookmarks))
        }),
        save_memory: Arc::new(move |_, stamp, _, bookmarks| {
            *saver.lock().expect("store") = Some((stamp, bookmarks.clone()));
        }),
        ..disk_hooks(disk)
    }
}

fn editor_bookmarks(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> Vec<u64> {
    let editor = editor_view(cx, view);
    editor.read_with(cx, |editor, _| {
        editor.marking_state().bookmarks.iter().collect()
    })
}

#[gpui_kit::test]
fn bookmarks_set_before_an_edit_never_reload_at_shifted_text(cx: &mut TestAppContext) {
    let disk = Shared::new(Mutex::new(Disk {
        text: "one\ntwo\nthree\n".to_owned(),
        stamp: 1,
        ..Disk::default()
    }));
    let store = Store::default();
    let (window, view) = window_with(cx, fake(), stamped_store(&disk, &store), Kinds::builtin());
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    let editor = editor_of(cx, &view).expect("the text editor is open");
    editor.update(cx, |state, cx| state.set_selected_range(8..8, cx));
    press(cx, window, &view, "ctrl-f2");
    press(cx, window, &view, "ctrl-e");
    editor.update(cx, |state, cx| state.set_selected_range(0..0, cx));
    type_in(cx, window, &view, "zero ");
    press(cx, window, &view, "ctrl-s");
    cx.executor().advance_clock(Duration::from_secs(1));
    settle(cx, &view);
    let stored = store.lock().expect("store").clone();
    assert_eq!(
        stored.map(|(stamp, bookmarks)| (stamp.modified_ms, bookmarks.iter().collect::<Vec<_>>())),
        Some((1, vec![8]))
    );
    press(cx, window, &view, "escape");
    assert!(!viewer_open(cx, &view));
    press(cx, window, &view, "f3");
    settle(cx, &view);
    assert_eq!(editor_text(cx, &view), "zero one\ntwo\nthree\n");
    assert!(editor_bookmarks(cx, &view).is_empty());
}
