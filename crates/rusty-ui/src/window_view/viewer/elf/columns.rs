use gpui_kit::Subscription;
use rusty_automotive::{Column, Elf, Symbol};

use super::text::{address, is_wide, place};
use super::{ElfView, Index};
use crate::format;
use crate::prelude::*;
use crate::primitives::{
    ColumnAlign, ColumnEvent, ColumnHead, ColumnSet, ColumnSpec, Fitted, fit_name, symbol_prefix,
};
use crate::tokens;

const NAME: usize = 0;
const ADDRESS: usize = 1;
const SIZE: usize = 2;

const COLUMNS: [(Column, &str, usize, usize); 6] = [
    (Column::Name, "Name", 16, 400),
    (Column::Address, "Address", 10, 24),
    (Column::Size, "Size", 6, 14),
    (Column::Kind, "Type", 6, 14),
    (Column::Bind, "Bind", 6, 12),
    (Column::Section, "Section", 8, 40),
];

pub(super) struct Columns {
    pub set: Entity<ColumnSet>,
    _events: Subscription,
    _redraw: Subscription,
}

fn specs() -> Vec<ColumnSpec> {
    COLUMNS
        .iter()
        .enumerate()
        .map(|(at, (_, title, min_chars, max_chars))| ColumnSpec {
            id: format!("elf-column-{at}").into(),
            title: (*title).into(),
            align: if at == SIZE {
                ColumnAlign::End
            } else {
                ColumnAlign::Start
            },
            min_chars: *min_chars,
            max_chars: *max_chars,
            flex: at == NAME,
        })
        .collect()
}

pub(super) fn texts(elf: &Elf, symbol: &Symbol) -> [String; 6] {
    [
        elf.name(symbol).to_owned(),
        address(symbol.value, is_wide(elf)),
        format::bytes(symbol.size),
        symbol.kind.name(),
        symbol.bind.name(),
        place(elf, symbol),
    ]
}

pub(super) fn fitted_name(name: &str, chars: usize) -> Fitted {
    let (prefix, rest) = symbol_prefix(name);
    fit_name(prefix, rest, chars)
}

impl ElfView {
    pub(super) fn ensure_columns(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.columns.is_some() {
            return;
        }
        let set = cx.new(|cx| ColumnSet::new(specs(), window, cx));
        let events = cx.subscribe_in(&set, window, |view, _, event: &ColumnEvent, window, cx| {
            if let ColumnEvent::Sort(at) = event
                && let Some((column, ..)) = COLUMNS.get(*at)
            {
                window.focus(&view.focus, cx);
                view.sort_by(*column, cx);
            }
        });
        let redraw = cx.observe(&set, |_, _, cx| cx.notify());
        self.columns = Some(Columns {
            set,
            _events: events,
            _redraw: redraw,
        });
        self.refit_columns(cx);
    }

    pub(super) fn refit_columns(&mut self, cx: &mut Context<Self>) {
        let (Some(columns), Index::Ready(elf)) = (&self.columns, &self.index) else {
            return;
        };
        let rows: Vec<Vec<String>> = self
            .rows
            .iter()
            .take(200)
            .filter_map(|at| elf.symbols.get(*at as usize))
            .map(|symbol| texts(elf, symbol).to_vec())
            .collect();
        columns.set.update(cx, |set, _| set.fit_content(rows));
    }

    pub(super) fn column_head(&self) -> Option<ColumnHead> {
        let columns = self.columns.as_ref()?;
        let order = self.query.order;
        let head = ColumnHead::new("elf-head", &columns.set);
        let sorted = COLUMNS
            .iter()
            .position(|(column, ..)| *column == order.column);
        Some(match sorted {
            Some(at) => head.sorted(at, order.descending),
            None => head,
        })
    }

    pub(super) fn symbol_cells(&self, row: usize, symbol: &Symbol, cx: &App) -> Vec<Div> {
        let (Some(columns), Some(elf)) = (&self.columns, self.elf()) else {
            return Vec::new();
        };
        let t = tokens::current(cx);
        let set = columns.set.read(cx);
        let [name, rest @ ..] = texts(elf, symbol);
        let name = fitted_name(&name, set.text_chars(NAME));
        let name_cell = set.cell(NAME).child(
            div()
                .id(("elf-name", row))
                .flex()
                .min_w_0()
                .child(div().text_color(t.colors.muted).child(name.prefix))
                .child(name.rest),
        );
        let others = rest.into_iter().enumerate().map(|(offset, text)| {
            let at = offset + 1;
            let cell = set.cell(at);
            match at {
                ADDRESS => cell.font_family(t.typography.mono_font.clone()),
                SIZE => cell.font_family(t.typography.numbers_font.clone()),
                _ => cell,
            }
            .child(div().min_w_0().truncate().child(text))
        });
        std::iter::once(name_cell).chain(others).collect()
    }
}
