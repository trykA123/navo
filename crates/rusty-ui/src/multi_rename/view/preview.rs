use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{EventEmitter, Role, Subscription, WeakEntity};
use rusty_core::Location;
use rusty_core::rename::Compiled;
use rusty_core::rename::check::Fold;
use rusty_l10n::t;

use super::cells::{Look, found_style, name_cell, status_cell};
use super::status::{Kind, Line, lines};
use super::summary::{Footer, footer};
use crate::multi_rename::{Item, Preview, Summary};
use crate::prelude::*;
use crate::primitives::{
    ColumnAlign, ColumnEvent, ColumnHead, ColumnSet, ColumnSpec, IconName, ListScroll,
    RowSemantics, VirtualList, icon,
};
use crate::tokens::{self, Theme};

const OLD: usize = 0;
const NEW: usize = 1;
pub(super) const STATUS: usize = 2;
const FIT_ROWS: usize = 200;
const DIGIT_WIDTH: f32 = 8.;
const TOGGLE_SIZE: f32 = 18.;
const PAGE_ROWS: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewEvent {
    RowToggled { parent: Location, name: String },
    Run,
    Cancel,
    Sort(usize),
}

#[derive(Clone, Copy)]
pub struct Shown<'a> {
    pub items: &'a [Item],
    pub preview: &'a Preview,
    pub fold: Fold,
    pub problems_only: bool,
    pub matcher: Option<&'a Compiled>,
    pub edited: bool,
}

pub struct PreviewView {
    columns: Entity<ColumnSet>,
    scroll: ListScroll,
    lines: Rc<Vec<Line>>,
    shown: Rc<Vec<usize>>,
    summary: Summary,
    can_run: bool,
    cursor: Option<usize>,
    sorted: Option<(usize, bool)>,
    _redraw: Subscription,
    _columns: Subscription,
}

impl EventEmitter<PreviewEvent> for PreviewView {}

fn specs() -> Vec<ColumnSpec> {
    let column = |id: &'static str, title: String, (min_chars, max_chars), flex| ColumnSpec {
        id: id.into(),
        title: title.into(),
        align: ColumnAlign::Start,
        min_chars,
        max_chars,
        flex,
    };
    vec![
        column("rename-old", t!("rename-column-old"), (16, 400), true),
        column("rename-new", t!("rename-column-new"), (16, 400), false),
        column("rename-status", t!("rename-column-status"), (12, 60), false),
    ]
}

impl PreviewView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let columns = cx.new(|cx| ColumnSet::new(specs(), window, cx));
        let redraw = cx.observe(&columns, |_, _, cx| cx.notify());
        let events = cx.subscribe(&columns, |_, _, event, cx| {
            if let ColumnEvent::Sort(column) = event {
                cx.emit(PreviewEvent::Sort(*column));
            }
        });
        Self {
            columns,
            scroll: ListScroll::new(),
            lines: Rc::default(),
            shown: Rc::default(),
            summary: Summary::default(),
            can_run: false,
            cursor: None,
            sorted: None,
            _redraw: redraw,
            _columns: events,
        }
    }

    pub fn set_preview(&mut self, shown: Shown<'_>, cx: &mut Context<Self>) {
        let mut rows = lines(shown.items, shown.preview, shown.fold, shown.matcher);
        for row in &mut rows {
            row.is_edited = shown.edited;
        }
        self.shown = Rc::new(shown.preview.visible(shown.problems_only));
        self.cursor = self.cursor.filter(|row| self.shown.contains(row));
        self.summary = shown.preview.summary;
        self.can_run = shown.preview.can_run();
        let sample: Vec<Vec<String>> = self
            .shown
            .iter()
            .take(FIT_ROWS)
            .map(|&row| {
                let line = &rows[row];
                vec![line.old.clone(), line.new.clone(), line.status()]
            })
            .collect();
        self.columns.update(cx, |set, _| set.fit_content(sample));
        self.lines = Rc::new(rows);
        cx.notify();
    }

    #[must_use]
    pub fn cursor(&self) -> Option<usize> {
        self.cursor
    }

    pub fn set_sort(&mut self, sorted: Option<(usize, bool)>) {
        self.sorted = sorted;
    }

    pub fn page(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(last) = self.shown.len().checked_sub(1) else {
            return;
        };
        let at = self
            .cursor
            .and_then(|row| self.shown.binary_search(&row).ok());
        let next = match (at, forward) {
            (None, _) => 0,
            (Some(at), true) => (at + PAGE_ROWS).min(last),
            (Some(at), false) => at.saturating_sub(PAGE_ROWS),
        };
        let row = self.shown.get(next).copied();
        self.set_cursor(row, cx);
    }

    #[must_use]
    pub fn first_shown(&self) -> Option<usize> {
        self.shown.first().copied()
    }

    pub fn set_cursor(&mut self, row: Option<usize>, cx: &mut Context<Self>) {
        self.cursor = row;
        if let Some(at) = row.and_then(|row| self.shown.binary_search(&row).ok()) {
            self.scroll.scroll_to(at);
        }
        cx.notify();
    }
}

struct Colors {
    old: Hsla,
    new: Hsla,
    status: Hsla,
}

fn colors(t: &Theme, kind: Kind) -> Colors {
    let c = &t.colors;
    match kind {
        Kind::Renames | Kind::CaseOnly | Kind::Swap => Colors {
            old: c.text,
            new: c.text,
            status: c.muted,
        },
        Kind::Unchanged | Kind::Excluded => Colors {
            old: c.muted,
            new: c.muted,
            status: c.muted,
        },
        Kind::Problem(_) => Colors {
            old: c.text,
            new: c.mark,
            status: c.mark,
        },
    }
}

struct Table {
    t: Theme,
    columns: Entity<ColumnSet>,
    view: WeakEntity<PreviewView>,
    cursor: Option<usize>,
    lead: Pixels,
}

fn toggle(table: &Table, row: usize, line: &Line) -> impl IntoElement {
    let t = &table.t;
    let (glyph, color) = if line.kind() == Kind::Excluded {
        (IconName::Minus, t.colors.muted)
    } else {
        (IconName::Check, t.colors.accent_text)
    };
    let view = table.view.clone();
    let parent = line.parent.clone();
    let name = line.old.clone();
    div()
        .id(("rename-toggle", row))
        .role(Role::CheckBox)
        .aria_label(line.old.clone())
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(TOGGLE_SIZE))
        .rounded(t.radii.sm)
        .cursor_pointer()
        .hover(|style| style.bg(t.colors.hover))
        .child(icon(glyph, px(12.), color))
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            let _ = view.update(cx, |_, cx| {
                cx.emit(PreviewEvent::RowToggled {
                    parent: parent.clone(),
                    name: name.clone(),
                });
            });
        })
}

fn lead_cell(table: &Table, row: usize, line: &Line) -> Div {
    let t = &table.t;
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.xs)
        .w(table.lead)
        .h_full()
        .px(t.spacing.xs)
        .child(toggle(table, row, line))
        .child(
            div()
                .flex_1()
                .text_right()
                .font_family(t.typography.numbers_font.clone())
                .font_features(t.typography.numbers_font_features.clone())
                .text_size(t.typography.small)
                .text_color(t.colors.muted)
                .child((row + 1).to_string()),
        )
}

fn row_view(table: &Table, (at, row): (usize, usize), line: &Line, cx: &App) -> AnyElement {
    let t = &table.t;
    let set = table.columns.read(cx);
    let kind = line.kind();
    let color = colors(t, kind);
    let is_struck = kind == Kind::Excluded;
    let found = found_style(t);
    let view = table.view.clone();
    div()
        .id(("rename-row", row))
        .flex()
        .w_full()
        .h_full()
        .cursor_pointer()
        .when(at % 2 == 1, |line| line.bg(t.colors.zebra))
        .when(table.cursor == Some(row), |line| line.bg(t.colors.hover))
        .child(lead_cell(table, row, line))
        .child(name_cell(
            set,
            (OLD, &line.old),
            Look {
                color: color.old,
                is_struck,
                marks: &line.old_marks,
                found,
            },
        ))
        .child(name_cell(
            set,
            (NEW, &line.new),
            Look {
                color: color.new,
                is_struck,
                marks: &line.new_marks,
                found,
            },
        ))
        .child(status_cell(set, t, line, color.status))
        .on_click(move |_, _, cx| {
            let _ = view.update(cx, |view, cx| view.set_cursor(Some(row), cx));
        })
        .into_any_element()
}

impl PreviewView {
    fn lead_width(&self, t: &Theme) -> Pixels {
        let digits = self.lines.len().to_string().len().max(2);
        px(DIGIT_WIDTH) * digits as f32 + px(TOGGLE_SIZE) + t.spacing.xs * 3.
    }

    fn list(&self, lead: Pixels, cx: &mut Context<Self>) -> VirtualList {
        let (lines, shown) = (self.lines.clone(), self.shown.clone());
        let describe = (lines.clone(), shown.clone());
        let (columns, view, cursor) = (self.columns.clone(), cx.weak_entity(), self.cursor);
        VirtualList::new(
            "rename-rows",
            shown.len(),
            &self.scroll,
            move |range, _, cx| {
                let table = Table {
                    t: tokens::current(cx).clone(),
                    columns: columns.clone(),
                    view: view.clone(),
                    cursor,
                    lead,
                };
                range
                    .filter_map(|at| {
                        let row = *shown.get(at)?;
                        Some(row_view(&table, (at, row), lines.get(row)?, cx))
                    })
                    .collect()
            },
        )
        .describe_rows(move |at| {
            let line = describe.1.get(at).and_then(|&row| describe.0.get(row));
            RowSemantics {
                label: line
                    .map(|line| format!("{}, {}, {}", line.old, line.new, line.status()).into()),
                ..RowSemantics::default()
            }
        })
    }
}

impl Render for PreviewView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let lead = self.lead_width(&t);
        let head = div()
            .flex()
            .flex_none()
            .child(
                div()
                    .flex_none()
                    .w(lead)
                    .h(t.spacing.row_height)
                    .bg(t.colors.rail)
                    .border_b_1()
                    .border_color(t.colors.line),
            )
            .child(
                div().flex_1().min_w_0().child(
                    ColumnHead::new("rename-head", &self.columns)
                        .when_some(self.sorted, |head, (column, descending)| {
                            head.sorted(column, descending)
                        }),
                ),
            );
        let footer = footer(
            &t,
            Footer {
                summary: self.summary,
                can_run: self.can_run,
                run: cx.listener(|_, _, _, cx| cx.emit(PreviewEvent::Run)),
                cancel: cx.listener(|_, _, _, cx| cx.emit(PreviewEvent::Cancel)),
            },
        );
        div()
            .id("rename-preview")
            .test_support()
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .bg(t.colors.surface)
            .text_size(t.typography.body)
            .child(head)
            .child(div().flex().flex_1().min_h_0().child(self.list(lead, cx)))
            .child(footer)
    }
}
