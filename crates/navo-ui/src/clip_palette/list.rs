use std::collections::HashSet;
use std::rc::Rc;

use gpui_kit::base::Scrollbar;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, ClickEvent, InteractiveElement, IntoElement, SharedString,
    StatefulInteractiveElement, WeakEntity, px,
};

use navo_core::Location;

use super::rows::{self, Place, ROW, RowElement};
use super::state::{Entry, PaletteState};
use super::view::ClipPalette;
use super::{ClipKind, ClipResults, ClipRow, Scope};
use crate::primitives::{ListScroll, RowSemantics, VirtualList};

/// Everything the list needs to draw a range of rows from a `'static` closure.
pub(super) struct Paint {
    entries: Rc<[Entry]>,
    results: Rc<ClipResults>,
    selected: usize,
    now_ms: i64,
    shows_source: bool,
    missing: Rc<HashSet<Location>>,
    view: WeakEntity<ClipPalette>,
}

fn interactive(
    row: RowElement,
    index: usize,
    is_selected: bool,
    view: &WeakEntity<ClipPalette>,
) -> RowElement {
    let (moved, chosen) = (view.clone(), view.clone());
    row.when(!is_selected, |row| {
        row.on_mouse_move(move |_, _, cx| {
            let _ = moved.update(cx, |palette, cx| palette.select(index, cx));
        })
    })
    .on_click(move |_: &ClickEvent, _, cx| {
        let _ = chosen.update(cx, |palette, cx| palette.choose_at(index, cx));
    })
}

impl Paint {
    pub(super) fn new(
        palette: &ClipPalette,
        shows_source: bool,
        view: WeakEntity<ClipPalette>,
    ) -> Self {
        let state: &PaletteState = &palette.state;
        Self {
            entries: state.entries().clone(),
            results: state.results().clone(),
            selected: state.selected(),
            now_ms: palette.now_ms,
            shows_source,
            missing: palette.missing_folders(),
            view,
        }
    }

    fn is_missing(&self, row: &ClipRow) -> bool {
        match &row.scope {
            Scope::Folder(at) => row.kind == ClipKind::Note && self.missing.contains(at),
            Scope::Everywhere => false,
        }
    }

    fn rows(&self, range: std::ops::Range<usize>, cx: &App) -> Vec<AnyElement> {
        let t = crate::tokens::current(cx);
        range
            .map(|position| match self.entries[position] {
                Entry::Head(origin) => rows::head(t, origin, position).into_any_element(),
                Entry::NotesHead(which) => rows::notes_head(t, which, position).into_any_element(),
                Entry::Row(index) => {
                    let is_selected = index == self.selected;
                    let data = &self.results.rows[index];
                    let place = Place {
                        index: position,
                        is_selected,
                        now_ms: self.now_ms,
                        gutter: Scrollbar::width(),
                        shows_source: self.shows_source,
                        is_missing: self.is_missing(data),
                        slot: Some(index),
                    };
                    let row = rows::row(t, data, place);
                    interactive(row, index, is_selected, &self.view).into_any_element()
                }
            })
            .collect()
    }

    pub(super) fn list(
        self,
        scroll: &ListScroll,
        label: impl Into<SharedString>,
    ) -> impl IntoElement {
        let (len, entries, selected) = (self.entries.len(), self.entries.clone(), self.selected);
        VirtualList::new("clip-palette-list", len, scroll, move |range, _, cx| {
            self.rows(range, cx)
        })
        .row_height(px(ROW))
        .label(label)
        .describe_rows(move |position| match entries[position] {
            Entry::Head(_) | Entry::NotesHead(_) => RowSemantics::default(),
            Entry::Row(index) => RowSemantics {
                label: None,
                is_selected: Some(index == selected),
                is_active_descendant: index == selected,
            },
        })
    }
}
