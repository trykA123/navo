use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};
use rusty_core::paging::{ByteSource, LineIndex};

use super::find::numbered_lines;
use super::{EDIT_LIMIT, Setup, hooks, redraw, text_of, top, window_with};
use crate::window_view::FerriteWindow;
use crate::window_view::tests::{fake, press, settle};
use crate::window_view::viewer::{FileHooks, Head, Kinds, Paged};

#[derive(Clone, Default)]
struct Growing {
    bytes: Arc<Mutex<Vec<u8>>>,
    changed: Arc<AtomicBool>,
}

impl Growing {
    fn append(&self, more: &[u8]) {
        self.bytes.lock().expect("bytes").extend_from_slice(more);
        self.changed.store(true, Ordering::Relaxed);
    }

    fn snapshot(&self) -> Vec<u8> {
        self.bytes.lock().expect("bytes").clone()
    }
}

impl ByteSource for Growing {
    fn len(&self) -> u64 {
        self.bytes.lock().expect("bytes").len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        self.bytes
            .lock()
            .expect("bytes")
            .as_slice()
            .read_at(offset, buf)
    }

    fn take_changed(&self) -> bool {
        self.changed.swap(false, Ordering::Relaxed)
    }
}

fn paged(growing: &Growing) -> Paged {
    let (reopened, indexed) = (growing.clone(), growing.clone());
    Paged {
        source: Arc::new(growing.clone()),
        reopen: Arc::new(move || Ok(Box::new(reopened.clone()) as Box<dyn ByteSource + Send>)),
        index: Arc::new(move |_, _| {
            let mut index = LineIndex::new();
            index.feed(&indexed.snapshot());
            index.finish();
            Ok(Some(index))
        }),
        summary: None,
        editable: false,
        stamp: None,
    }
}

fn growing_hooks(growing: &Growing) -> FileHooks {
    let (source, head) = (growing.clone(), growing.clone());
    FileHooks {
        head: Arc::new(move |_| {
            Ok(Head {
                bytes: head.snapshot().into_iter().take(8 * 1024).collect(),
                len: EDIT_LIMIT + 1,
            })
        }),
        paged: Arc::new(move |_| Ok(paged(&source))),
        ..hooks(Setup::new(Vec::new()), &super::Log::default())
    }
}

fn tick(cx: &mut TestAppContext, window: AnyWindowHandle, view: &Entity<FerriteWindow>) {
    cx.executor().advance_clock(Duration::from_secs(1));
    settle(cx, view);
    redraw(cx, window);
    redraw(cx, window);
}

fn more_lines(from: usize) -> Vec<u8> {
    (from..from + 200)
        .map(|line| format!("line {line:05}\n"))
        .collect::<String>()
        .into_bytes()
}

#[gpui_kit::test]
fn follow_keeps_the_view_on_the_end_until_a_scroll_up_pauses_it(cx: &mut TestAppContext) {
    let growing = Growing::default();
    growing.append(&numbered_lines());
    growing.changed.store(false, Ordering::Relaxed);
    let (window, view) = window_with(cx, fake(), growing_hooks(&growing), Kinds::builtin());
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    press(cx, window, &view, "alt-f");
    assert_eq!(text_of(cx, window, "paged-follow"), "Following");
    let at_end = top(cx, &view);
    assert!(at_end > 0);
    growing.append(&more_lines(2001));
    tick(cx, window, &view);
    let grown = top(cx, &view);
    assert!(grown > at_end, "{grown} stayed at {at_end}");
    press(cx, window, &view, "up");
    assert!(text_of(cx, window, "paged-follow").starts_with("Paused"));
    let paused = top(cx, &view);
    growing.append(&more_lines(2201));
    tick(cx, window, &view);
    assert_eq!(top(cx, &view), paused);
}
