mod blocks;
mod columns;
mod draw;
mod drawers;
mod facts;
mod go_to;
mod jobs;
mod keys;
mod loaded;
mod nav;
mod rich;
mod rich_view;
mod scope;
mod select;
mod table_bar;
mod text;

#[cfg(test)]
pub(in crate::window_view::viewer) mod tests;
#[cfg(test)]
mod tests_unread;

use std::any::Any;
use std::sync::Arc;

use gpui_kit::{AnyView, FocusHandle, Task};

pub(super) use keys::bind_keys;

use self::facts::Inspection;
use self::loaded::Loaded;
use self::nav::NavItem;
use self::select::{Query, Table};
use super::background::Job;
use super::body::{BodyKeys, Host, ViewerBody};
use super::parts::{AUTOMOTIVE_LAYOUT, LayoutParts, PartSet};
use super::search::bar::GO_TO_KEYS;
use super::space::{NEXT_PART, RichSpace, Siblings};
use crate::prelude::*;
use crate::primitives::{ListScroll, TextFieldState};
use crate::settings::Values;
use crate::side_sheet::SheetState;

pub(in crate::window_view::viewer) struct MapFile {
    pub len: u64,
}

enum Index {
    Running,
    Ready(Arc<Loaded>),
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

struct GoTo {
    input: Entity<TextFieldState>,
    error: Option<SharedString>,
    _enter: gpui_kit::Subscription,
    _find: Option<Task<()>>,
}

pub(in crate::window_view::viewer) struct MapView {
    title: SharedString,
    len: u64,
    focus: FocusHandle,
    scroll: ListScroll,
    nav_scroll: ListScroll,
    index: Index,
    scanned: u64,
    _index_job: Job,
    rows: Arc<Vec<u32>>,
    query: Query,
    selecting: Option<Job>,
    cursor: usize,
    inspection: Option<Inspection>,
    nav_items: Vec<NavItem>,
    nav_cursor: usize,
    focus_in: Focus,
    navigator: Entity<SheetState>,
    inspector: Entity<SheetState>,
    top_drawer: Drawer,
    note: Option<SharedString>,
    go_to: Option<GoTo>,
    go_address: Option<u64>,
    wanted_row: Option<u32>,
    page: usize,
    opens_inspector: bool,
    layout: LayoutParts,
    columns: Option<columns::Columns>,
    rich: RichSpace,
    siblings: Arc<Siblings>,
    shows_sections_for_rich: bool,
    _siblings_job: Job,
    model_job: Option<Job>,
    regroup_job: Option<Job>,
}

impl MapView {
    fn new(file: &MapFile, host: &Host, cx: &mut Context<Self>) -> Self {
        let job = jobs::start_index(host, file.len, cx);
        let siblings_job = rich::start_siblings_of(host, cx);
        Self {
            title: host.title.clone(),
            len: file.len,
            focus: cx.focus_handle(),
            scroll: ListScroll::new(),
            nav_scroll: ListScroll::new(),
            index: Index::Running,
            scanned: 0,
            _index_job: job,
            rows: Arc::new(Vec::new()),
            query: Query::default(),
            selecting: None,
            cursor: 0,
            inspection: None,
            nav_items: Vec::new(),
            nav_cursor: 0,
            focus_in: Focus::Content,
            navigator: cx.new(|_| SheetState::new()),
            inspector: cx.new(|_| SheetState::new()),
            top_drawer: Drawer::Navigator,
            note: None,
            go_to: None,
            go_address: None,
            wanted_row: None,
            page: 1,
            opens_inspector: false,
            layout: LayoutParts::default(),
            columns: None,
            rich: RichSpace::default(),
            siblings: Arc::new(Siblings::default()),
            shows_sections_for_rich: false,
            _siblings_job: siblings_job,
            model_job: None,
            regroup_job: None,
        }
    }

    fn loaded(&self) -> Option<&Arc<Loaded>> {
        match &self.index {
            Index::Ready(loaded) => Some(loaded),
            Index::Running | Index::Failed(_) => None,
        }
    }

    fn selected(&self) -> Option<usize> {
        self.rows.get(self.cursor).map(|at| *at as usize)
    }

    fn table(&self) -> Table {
        self.query.table
    }

    fn set_note(&mut self, note: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.note = Some(note.into());
        cx.notify();
    }
}

pub(in crate::window_view::viewer) struct MapBody(pub Entity<MapView>);

impl MapBody {
    pub(in crate::window_view::viewer) fn new(file: MapFile, host: Host, cx: &mut App) -> Self {
        Self(cx.new(|cx| MapView::new(&file, &host, cx)))
    }
}

impl ViewerBody for MapBody {
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
        let mut keys = vec![("Ctrl+B", "Navigator"), ("Ctrl+I", "Inspector")];
        match view.rich_keys(cx) {
            Some(Some(part)) => keys.extend(part),
            rich => {
                keys.extend([
                    ("Ctrl+G", "Go to address"),
                    ("Alt+T", "Table"),
                    ("Ctrl+F3", "Sort by name"),
                ]);
                keys.extend(rich.map(|_| NEXT_PART));
            }
        }
        if view.focus_in == Focus::Navigator {
            keys.insert(0, ("Enter", "Show only this"));
        }
        BodyKeys::Normal(keys)
    }

    fn find_again(&self, _: bool, _: &mut Window, _: &mut App) {}

    fn set_note(&self, note: SharedString, cx: &mut App) {
        self.0.update(cx, |view, cx| view.set_note(note, cx));
    }

    fn is_typing(&self, cx: &App) -> bool {
        self.0.read(cx).loaded().is_some()
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
            view.apply_modules(values.module_prefixes(), cx);
        });
    }
}
