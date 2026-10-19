use gpui_kit::base::TestSupportExt;
use gpui_kit::{ClickEvent, Role, WeakEntity};
use rusty_automotive::{Column, Elf, Symbol};

use super::keys::CONTEXT;
use super::text::{Check, address, is_wide, place, row_label, status};
use super::{ElfView, Focus, Index, Phase};
use crate::file_list::filter_bar;
use crate::format;
use crate::prelude::*;
use crate::primitives::{IconName, VirtualList, icon};
use crate::tokens::{self, Theme};
use crate::window_view::viewer::rich_header::{HeaderParts, header};

const CHAR_WIDTH: f32 = 7.2;
const PER_MILLE: u64 = 1000;

const COLUMNS: [(Column, &str, usize); 6] = [
    (Column::Name, "Name", 0),
    (Column::Address, "Address", 24),
    (Column::Size, "Size", 10),
    (Column::Kind, "Type", 7),
    (Column::Bind, "Bind", 7),
    (Column::Section, "Section", 14),
];

fn chars(t: &Theme, count: usize) -> Pixels {
    px(count as f32 * CHAR_WIDTH) + t.spacing.md * 2.
}

fn cell(t: &Theme, (column, wide): (Column, bool), text: String) -> Div {
    let width = COLUMNS
        .iter()
        .find(|(known, _, _)| *known == column)
        .map_or(0, |(_, _, width)| *width);
    let width = if column == Column::Address && !wide {
        13
    } else {
        width
    };
    let base = div()
        .flex()
        .items_center()
        .h_full()
        .px(t.spacing.md)
        .overflow_hidden()
        .whitespace_nowrap();
    let base = if width == 0 {
        base.flex_1().min_w_0()
    } else {
        base.flex_none().w(chars(t, width))
    };
    match column {
        Column::Address => base.font_family(t.typography.mono_font.clone()),
        Column::Size => base
            .justify_end()
            .font_family(t.typography.numbers_font.clone()),
        _ => base,
    }
    .child(div().min_w_0().truncate().child(text))
}

fn sort_arrow(descending: bool, color: Hsla) -> impl IntoElement {
    let glyph = if descending {
        IconName::ArrowDown
    } else {
        IconName::ArrowUp
    };
    icon(glyph, px(10.), color)
}

fn select_row(
    view: WeakEntity<ElfView>,
    row: usize,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) {
    move |_, window, cx| {
        let _ = view.update(cx, |view, cx| {
            window.focus(&view.focus, cx);
            view.focus_in = Focus::Content;
            view.move_cursor(row, cx);
            view.open_inspector_if_asked(cx);
        });
    }
}

fn symbol_cells(t: &Theme, elf: &Elf, symbol: &Symbol) -> Vec<Div> {
    let wide = is_wide(elf);
    let texts = [
        elf.name(symbol).to_owned(),
        address(symbol.value, wide),
        format::bytes(symbol.size),
        symbol.kind.name(),
        symbol.bind.name(),
        place(elf, symbol),
    ];
    COLUMNS
        .iter()
        .zip(texts)
        .map(|((column, _, _), text)| cell(t, (*column, wide), text))
        .collect()
}

impl ElfView {
    fn row_view(&self, t: &Theme, row: usize, view: &WeakEntity<Self>) -> AnyElement {
        let (Some(elf), Some((_, symbol))) = (self.elf(), self.symbol_at(row)) else {
            return div().into_any_element();
        };
        div()
            .id(("elf-row", row))
            .aria_label(row_label(elf, symbol))
            .test_support()
            .flex()
            .w_full()
            .h_full()
            .cursor_pointer()
            .when(row % 2 == 1, |line| line.bg(t.colors.zebra))
            .when(row == self.cursor, |line| line.bg(t.colors.hover))
            .children(symbol_cells(t, elf, symbol))
            .on_click(select_row(view.clone(), row))
            .into_any_element()
    }

    fn list(&self, cx: &mut Context<Self>) -> VirtualList {
        let view = cx.weak_entity();
        VirtualList::new(
            "elf-list",
            self.rows.len(),
            &self.scroll,
            move |range, _, cx| {
                let Some(entity) = view.upgrade() else {
                    return Vec::new();
                };
                let t = tokens::current(cx).clone();
                let this = entity.read(cx);
                range.map(|row| this.row_view(&t, row, &view)).collect()
            },
        )
        .label("Symbols")
    }

    fn column_head(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        let wide = self.elf().is_some_and(is_wide);
        let order = self.query.order;
        let cells = COLUMNS.iter().enumerate().map(|(at, (column, name, _))| {
            let column = *column;
            let sorted = (order.column == column).then_some(order.descending);
            let arrow = sorted.map(|descending| sort_arrow(descending, t.colors.accent_text));
            cell(t, (column, wide), (*name).to_owned())
                .id(("elf-head", at))
                .role(Role::Button)
                .aria_label(*name)
                .test_support()
                .gap(t.spacing.xs)
                .cursor_pointer()
                .when(sorted.is_some(), |cell| cell.text_color(t.colors.text))
                .children(arrow)
                .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                    window.focus(&view.focus, cx);
                    view.sort_by(column, cx);
                }))
        });
        div()
            .flex()
            .flex_none()
            .h(t.spacing.row_height)
            .text_size(t.typography.tiny)
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(t.colors.muted)
            .bg(t.colors.rail)
            .border_b_1()
            .border_color(t.colors.line)
            .children(cells)
    }

    fn progress(&self) -> Option<SharedString> {
        let percent =
            |done: u64, of: u64| (u128::from(done) * 100 / u128::from(of.max(1))).min(100);
        match (&self.index, self.phase) {
            (Index::Running, Phase::Symbols(done)) => {
                Some(format!("Reading symbols… {}%", percent(done, self.len)).into())
            }
            (Index::Ready(_), Phase::Lines(done)) => {
                Some(format!("Reading DWARF lines… {}%", percent(done, PER_MILLE)).into())
            }
            _ => self.note.clone(),
        }
    }

    fn header_parts(&self) -> HeaderParts {
        let detail = self.elf().zip(self.selected()).map(|(elf, (_, symbol))| {
            let at = address(symbol.value, is_wide(elf));
            ("elf-cursor", format!("{} · {at}", elf.name(symbol)).into())
        });
        HeaderParts {
            title: self.title.clone(),
            mode: self.elf().map_or_else(
                || "ELF".into(),
                |elf| format!("ELF · {}", elf.kind_name()).into(),
            ),
            len: self.len,
            detail,
            status: self.progress().map(|text| ("elf-progress", text)),
        }
    }

    fn status_line(&self, t: &Theme) -> impl IntoElement {
        let checks = self
            .elf()
            .map(|elf| status(elf, self.lines.as_ref()))
            .unwrap_or_default();
        let label = checks
            .iter()
            .map(|check| check.text.as_str())
            .collect::<Vec<_>>()
            .join(" · ");
        let parts = checks
            .into_iter()
            .enumerate()
            .map(|(at, Check { text, is_bad })| {
                let color = if is_bad {
                    t.colors.warn
                } else {
                    t.colors.muted
                };
                div()
                    .flex()
                    .flex_none()
                    .gap(t.spacing.sm)
                    .when(at > 0, |part| {
                        part.child(div().text_color(t.colors.faint).child("·"))
                    })
                    .child(div().text_color(color).child(text))
            });
        div()
            .id("elf-status")
            .role(Role::Status)
            .aria_label(label)
            .test_support()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.sm)
            .h(t.spacing.status_height)
            .px(t.spacing.xl)
            .overflow_hidden()
            .border_t_1()
            .border_color(t.colors.line)
            .bg(t.colors.rail)
            .text_size(t.typography.small)
            .children(parts)
    }

    fn measure(&mut self, window: &Window, t: &Theme) {
        let rows = (window.viewport_size().height / t.spacing.row_height).floor();
        self.page = if rows.is_finite() && rows > 6. {
            rows as usize - 6
        } else {
            1
        };
    }
}

impl Render for ElfView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        self.measure(window, &t);
        let table = div()
            .id("elf-table")
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(self.column_head(&t, cx))
            .child(self.list(cx))
            .child(self.drawers(&t, window, cx));
        let filter = self
            .is_filtering()
            .then(|| filter_bar(&t, self.query.text.clone().into(), self.rows.len()));
        let root = div()
            .id("elf-view")
            .key_context(CONTEXT)
            .track_focus(&self.focus);
        Self::keys(root, cx)
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .size_full()
            .bg(t.colors.surface)
            .text_size(t.typography.body)
            .child(header(&t, self.header_parts()))
            .child(table)
            .children(filter)
            .child(self.status_line(&t))
    }
}
