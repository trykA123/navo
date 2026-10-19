mod draw;
mod drawers;
mod follow;
mod inspect;
mod jobs;
mod keys;
mod nav;
mod text;

#[cfg(test)]
mod tests;

use std::any::Any;
use std::ops::Range;

use gpui_kit::{AnyView, FocusHandle};
use rusty_automotive::{Entry, Trace};

pub(super) use keys::bind_keys;

use super::background::Job;
use super::body::{BodyAct, BodyKeys, Host, ViewerBody};
use crate::prelude::*;
use crate::primitives::ListScroll;
use crate::side_sheet::SheetState;

pub(in crate::window_view::viewer) struct TraceFile {
    pub len: u64,
}

struct Filter {
    key: u32,
    rows: Vec<u64>,
}

struct Fetched {
    shown: Range<u64>,
    generation: u64,
    entries: Vec<Entry>,
}

struct Inspected {
    row: u64,
    current: Option<Entry>,
    previous: Option<Entry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Drawer {
    Navigator,
    Inspector,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Content,
    Navigator,
}

pub(in crate::window_view::viewer) struct TraceView {
    title: SharedString,
    len: u64,
    host: Host,
    focus: FocusHandle,
    scroll: ListScroll,
    nav_scroll: ListScroll,
    trace: Trace,
    reading: Option<Job>,
    is_indexed: bool,
    scanned: u64,
    filter: Option<Filter>,
    generation: u64,
    cursor: u64,
    fetched: Option<Fetched>,
    fetching: Option<(Range<u64>, u64, Job)>,
    inspected: Option<Inspected>,
    inspecting: Option<(u64, Job)>,
    navigator: Entity<SheetState>,
    inspector: Entity<SheetState>,
    top_drawer: Drawer,
    focus_in: Focus,
    nav: nav::Nav,
    follower: follow::Follower,
    note: Option<SharedString>,
    page: u64,
}

impl TraceView {
    fn new(file: &TraceFile, host: Host, cx: &mut Context<Self>) -> Self {
        let reading = jobs::start_index(&host, file.len, cx);
        Self {
            title: host.title.clone(),
            len: file.len,
            host,
            focus: cx.focus_handle(),
            scroll: ListScroll::new(),
            nav_scroll: ListScroll::new(),
            trace: Trace::default(),
            reading: Some(reading),
            is_indexed: false,
            scanned: 0,
            filter: None,
            generation: 0,
            cursor: 0,
            fetched: None,
            fetching: None,
            inspected: None,
            inspecting: None,
            navigator: cx.new(|_| SheetState::new()),
            inspector: cx.new(|_| SheetState::new()),
            top_drawer: Drawer::Navigator,
            focus_in: Focus::Content,
            nav: nav::Nav::default(),
            follower: follow::Follower::default(),
            note: None,
            page: 1,
        }
    }

    fn shown_rows(&self) -> u64 {
        self.filter
            .as_ref()
            .map_or(self.trace.rows(), |filter| filter.rows.len() as u64)
    }

    fn actual(&self, shown: u64) -> Option<u64> {
        match &self.filter {
            Some(filter) => usize::try_from(shown)
                .ok()
                .and_then(|at| filter.rows.get(at).copied()),
            None => (shown < self.trace.rows()).then_some(shown),
        }
    }

    fn shown_of(&self, row: u64) -> u64 {
        match &self.filter {
            Some(filter) => filter.rows.partition_point(|found| *found < row) as u64,
            None => row,
        }
    }

    fn entry(&self, shown: u64) -> Option<&Entry> {
        let row = self.actual(shown)?;
        let fetched = self.fetched.as_ref()?;
        let at = fetched
            .entries
            .binary_search_by_key(&row, |entry| entry.row)
            .ok()?;
        fetched.entries.get(at)
    }

    fn set_note(&mut self, note: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.note = Some(note.into());
        cx.notify();
    }
}

pub(in crate::window_view::viewer) struct TraceBody(pub Entity<TraceView>);

impl TraceBody {
    pub(in crate::window_view::viewer) fn new(file: &TraceFile, host: Host, cx: &mut App) -> Self {
        Self(cx.new(|cx| TraceView::new(file, host, cx)))
    }
}

impl ViewerBody for TraceBody {
    fn view(&self) -> AnyView {
        self.0.clone().into()
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.0.read(cx).focus.clone()
    }

    fn keys(&self, cx: &App) -> BodyKeys {
        let view = self.0.read(cx);
        let mut keys = vec![
            ("Ctrl+B", "IDs"),
            ("Ctrl+I", "Inspector"),
            ("Alt+F", "Follow"),
        ];
        if view.focus_in == Focus::Navigator {
            keys.insert(0, ("Enter", "Filter by ID"));
            keys.insert(1, ("Alt+S", "Sort"));
        }
        BodyKeys::Normal(keys)
    }

    fn find_again(&self, _: bool, _: &mut Window, _: &mut App) {}

    fn set_note(&self, note: SharedString, cx: &mut App) {
        self.0.update(cx, |view, cx| view.set_note(note, cx));
    }

    fn act(&self, act: BodyAct, _: &mut Window, cx: &mut App) {
        if act == BodyAct::Follow {
            self.0.update(cx, |view, cx| view.toggle_follow(cx));
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
