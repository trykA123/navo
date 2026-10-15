use std::sync::{Arc, Mutex};

use gpui_kit::TestAppContext;

use super::{editor_text, press_only, redraw, text_of, window_with};
use crate::window_view::tests::{fake, press, settle, type_in};
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
        paged: FileHooks::none().paged,
        bytes: FileHooks::none().bytes,
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
