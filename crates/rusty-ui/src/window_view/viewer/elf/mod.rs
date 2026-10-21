mod columns;
mod draw;
mod drawers;
mod go_to;
mod jobs;
mod keys;
mod rich;
mod rich_view;
mod text;

#[cfg(test)]
mod tests;

use std::any::Any;
use std::sync::Arc;

use gpui_kit::{AnyView, FocusHandle, Subscription};
use rusty_automotive::{Elf, Lines, Query, Scope, Symbol};

pub(super) use keys::bind_keys;
pub(in crate::window_view::viewer) use text::parse_address;

use super::background::Job;
use super::body::{BodyKeys, Host, ViewerBody};
use super::parts::{AUTOMOTIVE_LAYOUT, LayoutParts, PartSet};
use super::search::bar::GO_TO_KEYS;
use super::space::{NEXT_PART, RichSpace, Siblings};
use crate::prelude::*;
use crate::primitives::{ListScroll, TextFieldState};
use crate::settings::Values;
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
    Failed(SharedString),
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

struct GoTo {
    input: Entity<TextFieldState>,
    error: Option<SharedString>,
    _enter: Subscription,
}

pub(in crate::window_view::viewer) struct ElfView {
    title: SharedString,
    len: u64,
    focus: FocusHandle,
    scroll: ListScroll,
    nav_scroll: ListScroll,
    index: Index,
    lines: Option<Arc<Lines>>,
    phase: Phase,
    _index_job: Job,
    rows: Arc<Vec<u32>>,
    hidden: u64,
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
    lines_error: Option<SharedString>,
    go_to: Option<GoTo>,
    go_address: Option<u64>,
    wanted_symbol: Option<u32>,
    page: usize,
    opens_inspector: bool,
    layout: LayoutParts,
    columns: Option<columns::Columns>,
    rich: RichSpace,
    siblings: Arc<Siblings>,
    auto_sorted: bool,
    _siblings_job: Job,
    model_job: Option<Job>,
    regroup_job: Option<Job>,
}

impl ElfView {
    fn new(file: ElfFile, host: &Host, cx: &mut Context<Self>) -> Self {
        let job = jobs::start_index(host, file.len, cx);
        let siblings_job = rich::start_siblings_of(host, cx);
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
            hidden: 0,
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
            lines_error: None,
            go_to: None,
            go_address: None,
            wanted_symbol: None,
            page: 1,
            opens_inspector: false,
            layout: LayoutParts::default(),
            columns: None,
            rich: RichSpace::default(),
            siblings: Arc::new(Siblings::default()),
            auto_sorted: false,
            _siblings_job: siblings_job,
            model_job: None,
            regroup_job: None,
        }
    }

    fn elf(&self) -> Option<&Elf> {
        match &self.index {
            Index::Ready(elf) => Some(elf),
            Index::Running | Index::Failed(_) => None,
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
        if view.go_to.is_some() {
            return BodyKeys::Modal(GO_TO_KEYS.to_vec());
        }
        let mut keys = vec![("Ctrl+B", "Sections"), ("Ctrl+I", "Inspector")];
        match view.rich_keys(cx) {
            Some(Some(part)) => keys.extend(part),
            rich => {
                keys.extend([("Ctrl+G", "Go to address"), ("Ctrl+F3", "Sort by name")]);
                keys.extend(rich.map(|_| NEXT_PART));
            }
        }
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
        self.0.read(cx).elf().is_some()
    }

    fn takes_letters(&self, _: &App) -> bool {
        true
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn parts(&self, _: &App) -> Option<&'static PartSet> {
        Some(&AUTOMOTIVE_LAYOUT)
    }

    fn show_parts(&self, values: &Values, cx: &mut App) {
        let opens = values.opens_inspector();
        let layout = LayoutParts::of(values);
        self.0.update(cx, |view, cx| {
            view.opens_inspector = opens;
            view.apply_layout(layout, cx);
        });
    }
}
