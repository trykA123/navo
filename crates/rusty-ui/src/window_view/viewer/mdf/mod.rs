mod draw;
mod jobs;
mod keys;
mod text;

#[cfg(test)]
pub(in crate::window_view::viewer) mod tests;

use std::any::Any;
use std::sync::Arc;

use gpui_kit::{AnyView, FocusHandle, Subscription};
use rusty_automotive::Mdf;

use self::text::heads;
use super::background::Job;
use super::body::{BodyKeys, Host, ViewerBody};
use super::parts::{AUTOMOTIVE_TABLE, PartSet};
use crate::prelude::*;
use crate::primitives::{ColumnAlign, ColumnSet, ColumnSpec, ListScroll};
use crate::settings::Values;
use crate::side_sheet::SheetState;

pub(in crate::window_view::viewer) struct MdfFile {
    pub len: u64,
}

pub(super) struct Loaded {
    mdf: Mdf,
    rows: Vec<Vec<String>>,
}

enum Index {
    Running,
    Ready(Arc<Loaded>),
    Failed(SharedString),
}

pub(in crate::window_view::viewer) struct MdfView {
    title: SharedString,
    len: u64,
    focus: FocusHandle,
    scroll: ListScroll,
    index: Index,
    _job: Job,
    cursor: usize,
    inspector: Entity<SheetState>,
    columns: Option<Entity<ColumnSet>>,
    _redraw: Option<Subscription>,
    note: Option<SharedString>,
    page: usize,
    opens_inspector: bool,
}

const COLUMN_CHARS: [(usize, usize); 7] = [
    (10, 40),
    (4, 6),
    (8, 12),
    (8, 24),
    (12, 18),
    (12, 18),
    (12, 18),
];

fn specs() -> Vec<ColumnSpec> {
    heads()
        .into_iter()
        .zip(COLUMN_CHARS)
        .enumerate()
        .map(|(at, (title, (min_chars, max_chars)))| ColumnSpec {
            id: format!("mdf-column-{at}").into(),
            title: title.into(),
            align: if at == 2 {
                ColumnAlign::End
            } else {
                ColumnAlign::Start
            },
            min_chars,
            max_chars,
            flex: at == 0,
        })
        .collect()
}

impl MdfView {
    fn new(file: &MdfFile, host: &Host, cx: &mut Context<Self>) -> Self {
        Self {
            title: host.title.clone(),
            len: file.len,
            focus: cx.focus_handle(),
            scroll: ListScroll::new(),
            index: Index::Running,
            _job: jobs::start_parse(host, cx),
            cursor: 0,
            inspector: cx.new(|_| SheetState::new()),
            columns: None,
            _redraw: None,
            note: None,
            page: 1,
            opens_inspector: false,
        }
    }

    fn loaded(&self) -> Option<&Arc<Loaded>> {
        match &self.index {
            Index::Ready(loaded) => Some(loaded),
            Index::Running | Index::Failed(_) => None,
        }
    }

    fn row_count(&self) -> usize {
        self.loaded().map_or(0, |loaded| loaded.rows.len())
    }

    fn refresh_cursor(&mut self) {
        self.cursor = self.cursor.min(self.row_count().saturating_sub(1));
    }

    fn ensure_columns(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (None, Some(loaded)) = (&self.columns, self.loaded().cloned()) else {
            return;
        };
        let set = cx.new(|cx| {
            let mut set = ColumnSet::new(specs(), window, cx);
            set.fit_content(loaded.rows.iter().cloned());
            set
        });
        self._redraw = Some(cx.observe(&set, |_, _, cx| cx.notify()));
        self.columns = Some(set);
    }

    fn set_note(&mut self, note: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.note = Some(note.into());
        cx.notify();
    }
}

pub(in crate::window_view::viewer) struct MdfBody(pub Entity<MdfView>);

impl MdfBody {
    pub(in crate::window_view::viewer) fn new(file: &MdfFile, host: &Host, cx: &mut App) -> Self {
        Self(cx.new(|cx| MdfView::new(file, host, cx)))
    }
}

impl ViewerBody for MdfBody {
    fn view(&self) -> AnyView {
        self.0.clone().into()
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.0.read(cx).focus.clone()
    }

    fn keys(&self, _: &App) -> BodyKeys {
        BodyKeys::Normal(vec![("Ctrl+I", "Inspector")])
    }

    fn find_again(&self, _: bool, _: &mut Window, _: &mut App) {}

    fn set_note(&self, note: SharedString, cx: &mut App) {
        self.0.update(cx, |view, cx| view.set_note(note, cx));
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn parts(&self, _: &App) -> Option<&'static PartSet> {
        Some(&AUTOMOTIVE_TABLE)
    }

    fn show_parts(&self, values: &Values, cx: &mut App) {
        let opens = values.opens_inspector();
        self.0.update(cx, |view, _| view.opens_inspector = opens);
    }
}
