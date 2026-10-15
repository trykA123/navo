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
use crate::window_view::viewer::body::Carried;
use crate::window_view::viewer::{FileHooks, FileStamp};

const SAVE_AFTER: Duration = Duration::from_millis(800);

#[derive(Clone)]
pub struct MemoryLink {
    pub hooks: FileHooks,
    pub location: Location,
    pub stamp: Option<FileStamp>,
    pub carried: Option<Carried>,
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
            .filter(|_| link.carried.is_none())
            .and_then(|stamp| (link.hooks.memory)(&link.location, stamp)),
    }
}

fn merge_marks(into: &mut MarkSet, loaded: MarkSet) {
    for mark in loaded.iter() {
        if into.colour_of(&mark.needle).is_none() {
            let _ = into.set(mark.needle.clone(), mark.colour);
        }
    }
}

fn merge_bookmarks(into: &mut Bookmarks, loaded: &Bookmarks) {
    let current: Vec<u64> = into.iter().collect();
    for at in loaded.iter().filter(|at| !current.contains(at)) {
        into.toggle(at);
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
    mut link: MemoryLink,
    cx: &mut Context<V>,
) {
    if let Some(carried) = link.carried.clone() {
        let marking = view.marking();
        marking.marks.set = carried.marks;
        marking.bookmarks = carried.bookmarks.unwrap_or_default();
        recount_marks(view, cx);
    }
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
    link.carried = None;
    let memory = &mut view.marking().memory;
    memory.link = Some(link);
    memory.loading = Some(loading);
    memory._release = Some(release);
}

fn remembered_loaded<V: Marked>(view: &mut V, remembered: Remembered, cx: &mut Context<V>) {
    let marking = view.marking();
    marking.memory.loading = None;
    marking.history.merge_older(remembered.history);
    if let Some((marks, bookmarks)) = remembered.memory {
        merge_marks(&mut marking.marks.set, marks);
        merge_bookmarks(&mut marking.bookmarks, &bookmarks);
        marking.rev += 1;
        recount_marks(view, cx);
    }
    cx.notify();
}

pub(in crate::window_view::viewer) fn restamp<V: Marked>(
    view: &mut V,
    stamp: FileStamp,
    cx: &mut Context<V>,
) {
    let Some(link) = &mut view.marking().memory.link else {
        return;
    };
    if link.stamp.is_none() {
        return;
    }
    link.stamp = Some(stamp);
    memory_changed(view, cx);
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

#[cfg(test)]
mod tests {
    use rusty_core::marks::{Bookmarks, MarkSet};
    use rusty_core::paging::Needle;

    use super::{merge_bookmarks, merge_marks};

    fn word(text: &str) -> Needle {
        Needle::Text {
            text: text.to_owned(),
            match_case: false,
        }
    }

    #[test]
    fn late_memory_adds_to_what_was_set_meanwhile_and_keeps_its_colours() {
        let mut current = MarkSet::default();
        current.set(word("alpha"), 1).expect("room");
        let mut loaded = MarkSet::default();
        loaded.set(word("alpha"), 5).expect("room");
        loaded.set(word("beta"), 2).expect("room");
        merge_marks(&mut current, loaded);
        assert_eq!(current.colour_of(&word("alpha")), Some(1));
        assert_eq!(current.colour_of(&word("beta")), Some(2));
        let mut marked = Bookmarks::default();
        marked.toggle(10);
        let mut remembered = Bookmarks::default();
        remembered.toggle(10);
        remembered.toggle(40);
        merge_bookmarks(&mut marked, &remembered);
        assert_eq!(marked.iter().collect::<Vec<_>>(), [10, 40]);
    }
}
