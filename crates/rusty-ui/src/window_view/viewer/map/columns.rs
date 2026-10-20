use gpui_kit::Subscription;

use super::loaded::Loaded;
use super::select::{Column, Table};
use super::text::{cells, heads};
use super::{Index, MapView};
use crate::prelude::*;
use crate::primitives::{
    ColumnAlign, ColumnEvent, ColumnHead, ColumnSet, ColumnSpec, Fitted, fit_name, map_prefix,
    symbol_prefix,
};
use crate::tokens;

const FIT_ROWS: usize = 200;

pub(super) struct Columns {
    sets: [Entity<ColumnSet>; 3],
    _subscriptions: Vec<Subscription>,
}

fn chars(column: Column) -> (usize, usize) {
    match column {
        Column::Name => (16, 400),
        Column::Address => (10, 24),
        Column::Size | Column::Used | Column::Free => (6, 14),
        Column::Percent => (6, 10),
        Column::Region | Column::Space => (8, 40),
    }
}

fn is_number(column: Column) -> bool {
    matches!(
        column,
        Column::Size | Column::Used | Column::Free | Column::Percent
    )
}

fn specs(table: Table, columns: &[Column], names: &[&'static str]) -> Vec<ColumnSpec> {
    columns
        .iter()
        .zip(names)
        .enumerate()
        .map(|(at, (column, name))| {
            let (min_chars, max_chars) = chars(*column);
            ColumnSpec {
                id: format!("map-column-{}-{at}", table.index()).into(),
                title: (*name).into(),
                align: if is_number(*column) {
                    ColumnAlign::End
                } else {
                    ColumnAlign::Start
                },
                min_chars,
                max_chars,
                flex: *column == Column::Name,
            }
        })
        .collect()
}

pub(super) fn fitted_name(table: Table, name: &str, chars: usize) -> Fitted {
    let (prefix, rest) = match table {
        Table::Memory => ("", name),
        Table::Sections => map_prefix(name),
        Table::Symbols => symbol_prefix(name),
    };
    fit_name(prefix, rest, chars)
}

impl Columns {
    pub(super) fn set(&self, table: Table) -> &Entity<ColumnSet> {
        &self.sets[table.index()]
    }
}

fn build_set(
    loaded: &Loaded,
    table: Table,
    window: &mut Window,
    cx: &mut Context<MapView>,
) -> (Entity<ColumnSet>, [Subscription; 2]) {
    let shown = heads(loaded, table);
    let columns: Vec<Column> = shown.iter().map(|head| head.column).collect();
    let names: Vec<&'static str> = shown.iter().map(|head| head.name).collect();
    let rows: Vec<Vec<String>> = (0..table.len(loaded).min(FIT_ROWS))
        .map(|at| cells(loaded, table, at))
        .collect();
    let set = cx.new(|cx| {
        let mut set = ColumnSet::new(specs(table, &columns, &names), window, cx);
        set.fit_content(rows);
        set
    });
    let sort = cx.subscribe_in(
        &set,
        window,
        move |view, _, event: &ColumnEvent, window, cx| {
            if let ColumnEvent::Sort(at) = event {
                view.sort_from_head((table, *at), window, cx);
            }
        },
    );
    let redraw = cx.observe(&set, |_, _, cx| cx.notify());
    (set, [sort, redraw])
}

impl MapView {
    pub(super) fn ensure_columns(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (None, Index::Ready(loaded)) = (&self.columns, &self.index) else {
            return;
        };
        let loaded = loaded.clone();
        let built = Table::ALL.map(|table| build_set(&loaded, table, window, cx));
        let sets = built.each_ref().map(|(set, _)| set.clone());
        let subscriptions = built.into_iter().flat_map(|(_, subs)| subs).collect();
        self.columns = Some(Columns {
            sets,
            _subscriptions: subscriptions,
        });
    }

    fn sort_from_head(
        &mut self,
        (table, at): (Table, usize),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let column = self
            .loaded()
            .and_then(|loaded| heads(loaded, table).get(at).map(|head| head.column));
        let Some(column) = column.filter(|_| self.table() == table) else {
            return;
        };
        window.focus(&self.focus, cx);
        self.sort_by(column, cx);
    }

    fn column_set(&self) -> Option<&Entity<ColumnSet>> {
        self.columns
            .as_ref()
            .map(|columns| columns.set(self.table()))
    }

    pub(super) fn column_head(&self) -> Option<ColumnHead> {
        let set = self.column_set()?;
        let loaded = self.loaded()?;
        let order = self.query.order();
        let head = ColumnHead::new("map-head", set);
        let sorted = heads(loaded, self.table())
            .iter()
            .position(|head| head.column == order.column);
        Some(match sorted {
            Some(at) => head.sorted(at, order.descending),
            None => head,
        })
    }

    pub(super) fn row_cells(&self, row: usize, texts: &[String], cx: &App) -> Vec<Div> {
        let (Some(set), Some(loaded)) = (self.column_set(), self.loaded()) else {
            return Vec::new();
        };
        let t = tokens::current(cx);
        let set = set.read(cx);
        let table = self.table();
        heads(loaded, table)
            .iter()
            .zip(texts)
            .enumerate()
            .map(|(at, (head, text))| {
                let cell = set.cell(at);
                if head.column == Column::Name {
                    let name = fitted_name(table, text, set.text_chars(at));
                    return cell.child(
                        div()
                            .id(("map-name", row))
                            .flex()
                            .min_w_0()
                            .child(div().text_color(t.colors.muted).child(name.prefix))
                            .child(name.rest),
                    );
                }
                let cell = match head.column {
                    Column::Address => cell.font_family(t.typography.mono_font.clone()),
                    column if is_number(column) => {
                        cell.font_family(t.typography.numbers_font.clone())
                    }
                    _ => cell,
                };
                cell.child(div().min_w_0().truncate().child(text.clone()))
            })
            .collect()
    }
}
