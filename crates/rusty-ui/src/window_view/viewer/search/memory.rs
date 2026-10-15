use std::time::Duration;

use futures::channel::oneshot;
use gpui_kit::{Subscription, Task};
use rusty_core::Location;
use rusty_core::marks::{Bookmarks, MarkSet, SearchHistory};
use rusty_core::paging::Needle;

use super::jobs::run_detached;
use super::marks::recount_marks;
use super::{Marked, Marking};
use crate::prelude::*;
use crate::window_view::viewer::{FileHooks, FileStamp};

const SAVE_AFTER: Duration = Duration::from_millis(800);

#[derive(Clone)]
pub struct MemoryLink {
    pub hooks: FileHooks,
    pub location: Location,
    pub stamp: Option<FileStamp>,
}

#[derive(Default)]
pub(in crate::window_view::viewer) struct Memory {
    link: Option<MemoryLink>,
    is_dirty: bool,
    pending: Option<Task<()>>,
    loading: Option<Task<()>>,
    _release: Option<Subscription>,
}

struct Remembered {
    history: SearchHistory,
    memory: Option<(MarkSet, Bookmarks)>,
}

fn recall(link: &MemoryLink) -> Remembered {
    Remembered {
        history: (link.hooks.history)(),
        memory: link
            .stamp
            .and_then(|stamp| (link.hooks.memory)(&link.location, stamp)),
    }
}

impl Marking {
    fn save_now(&mut self, cx: &App) {
        let memory = &mut self.memory;
        memory.pending = None;
        if !memory.is_dirty {
            return;
        }
        memory.is_dirty = false;
        let Some((link, stamp)) = memory
            .link
            .clone()
            .and_then(|link| link.stamp.map(|stamp| (link, stamp)))
        else {
            return;
        };
        let (marks, bookmarks) = (self.marks.set.clone(), self.bookmarks.clone());
        run_detached(
            move || (link.hooks.save_memory)(&link.location, stamp, &marks, &bookmarks),
            cx,
        );
    }

    pub(in crate::window_view::viewer) fn remember_search(&mut self, needle: &Needle, cx: &App) {
        if !self.history.push(needle.clone()) {
            return;
        }
        let Some(link) = self.memory.link.clone() else {
            return;
        };
        let needle = needle.clone();
        run_detached(move || (link.hooks.remember)(&needle), cx);
    }
}

pub(in crate::window_view::viewer) fn attach_memory<V: Marked>(
    view: &mut V,
    link: MemoryLink,
    cx: &mut Context<V>,
) {
    let (sender, receiver) = oneshot::channel();
    let worker = link.clone();
    run_detached(
        move || {
            let _ = sender.send(recall(&worker));
        },
        cx,
    );
    let loading = cx.spawn(async move |this, cx| {
        if let Ok(remembered) = receiver.await {
            let _ = this.update(cx, |view, cx| remembered_loaded(view, remembered, cx));
        }
    });
    let release = cx.on_release(|view: &mut V, cx| view.marking().save_now(cx));
    let memory = &mut view.marking().memory;
    memory.link = Some(link);
    memory.loading = Some(loading);
    memory._release = Some(release);
}

fn remembered_loaded<V: Marked>(view: &mut V, remembered: Remembered, cx: &mut Context<V>) {
    let marking = view.marking();
    marking.memory.loading = None;
    marking.history.merge_older(remembered.history);
    if let Some((marks, bookmarks)) = remembered.memory.filter(|_| !marking.memory.is_dirty) {
        marking.marks.set = marks;
        marking.bookmarks = bookmarks;
        marking.rev += 1;
        recount_marks(view, cx);
    }
    cx.notify();
}

pub(in crate::window_view::viewer) fn memory_changed<V: Marked>(view: &mut V, cx: &mut Context<V>) {
    let memory = &mut view.marking().memory;
    if memory.link.is_none() {
        return;
    }
    memory.is_dirty = true;
    memory.pending = Some(cx.spawn(async move |this, cx| {
        cx.background_executor().timer(SAVE_AFTER).await;
        let _ = this.update(cx, |view, cx| view.marking().save_now(cx));
    }));
}
