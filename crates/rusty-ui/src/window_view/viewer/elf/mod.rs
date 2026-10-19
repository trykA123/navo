mod draw;
mod drawers;
mod jobs;
mod keys;
mod text;

#[cfg(test)]
mod tests;

use std::any::Any;
use std::sync::Arc;

use gpui_kit::{AnyView, FocusHandle};
use rusty_automotive::{Elf, Lines, Query, Scope, Symbol};

pub(super) use keys::bind_keys;

use super::background::Job;
use super::body::{BodyKeys, Host, ViewerBody};
use crate::prelude::*;
use crate::primitives::ListScroll;
use crate::side_sheet::SheetState;

pub(in crate::window_view::viewer) struct ElfFile {
    pub len: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Symbols(u64),
    Lines(u64),
    Done,
}

enum Index {
    Running,
    Ready(Arc<Elf>),
    Failed,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NavItem {
    All,
    Section(u32),
    Segment(usize),
}

impl NavItem {
    fn scope(self) -> Scope {
        match self {
            Self::All => Scope::All,
            Self::Section(index) => Scope::Section(index),
            Self::Segment(at) => Scope::Segment(at),
        }
    }
}

pub(in crate::window_view::viewer) struct ElfView {
    title: SharedString,
    len: u64,
    focus: FocusHandle,
    scroll: ListScroll,
    nav_scroll: ListScroll,
    index: Index,
    lines: Option<Lines>,
    phase: Phase,
    _index_job: Job,
    rows: Arc<Vec<u32>>,
    query: Query,
    selecting: Option<Job>,
    cursor: usize,
    nav_items: Vec<NavItem>,
    nav_cursor: usize,
    focus_in: Focus,
    navigator: Entity<SheetState>,
    inspector: Entity<SheetState>,
    top_drawer: Drawer,
    note: Option<SharedString>,
    page: usize,
}

impl ElfView {
    fn new(file: ElfFile, host: &Host, cx: &mut Context<Self>) -> Self {
        let job = jobs::start_index(host, file.len, cx);
        Self {
            title: host.title.clone(),
            len: file.len,
            focus: cx.focus_handle(),
            scroll: ListScroll::new(),
            nav_scroll: ListScroll::new(),
            index: Index::Running,
            lines: None,
            phase: Phase::Symbols(0),
            _index_job: job,
            rows: Arc::new(Vec::new()),
            query: Query::default(),
            selecting: None,
            cursor: 0,
            nav_items: Vec::new(),
            nav_cursor: 0,
            focus_in: Focus::Content,
            navigator: cx.new(|_| SheetState::new()),
            inspector: cx.new(|_| SheetState::new()),
            top_drawer: Drawer::Navigator,
            note: None,
            page: 1,
        }
    }

    fn elf(&self) -> Option<&Elf> {
        match &self.index {
            Index::Ready(elf) => Some(elf),
            Index::Running | Index::Failed => None,
        }
    }

    fn symbol_at(&self, row: usize) -> Option<(usize, &Symbol)> {
        let at = *self.rows.get(row)? as usize;
        Some((at, self.elf()?.symbols.get(at)?))
    }

    fn selected(&self) -> Option<(usize, &Symbol)> {
        self.symbol_at(self.cursor)
    }

    fn is_filtering(&self) -> bool {
        !self.query.text.is_empty()
    }

    fn set_note(&mut self, note: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.note = Some(note.into());
        cx.notify();
    }
}

pub(in crate::window_view::viewer) struct ElfBody(pub Entity<ElfView>);

impl ElfBody {
    pub(in crate::window_view::viewer) fn new(file: ElfFile, host: Host, cx: &mut App) -> Self {
        Self(cx.new(|cx| ElfView::new(file, &host, cx)))
    }
}

impl ViewerBody for ElfBody {
    fn view(&self) -> AnyView {
        self.0.clone().into()
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.0.read(cx).focus.clone()
    }

    fn keys(&self, cx: &App) -> BodyKeys {
        let view = self.0.read(cx);
        let mut keys = vec![
            ("Ctrl+B", "Sections"),
            ("Ctrl+I", "Inspector"),
            ("Ctrl+F3", "Sort by name"),
        ];
        if view.focus_in == Focus::Navigator {
            keys.insert(0, ("Enter", "Show its symbols"));
        }
        BodyKeys::Normal(keys)
    }

    fn find_again(&self, _: bool, _: &mut Window, _: &mut App) {}

    fn set_note(&self, note: SharedString, cx: &mut App) {
        self.0.update(cx, |view, cx| view.set_note(note, cx));
    }

    fn is_typing(&self, cx: &App) -> bool {
        self.0.read(cx).is_filtering()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
