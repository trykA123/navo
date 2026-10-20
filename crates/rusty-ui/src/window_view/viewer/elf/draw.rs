use gpui_kit::base::TestSupportExt;
use gpui_kit::{ClickEvent, Role, WeakEntity};

use super::keys::{CONTEXT, TABLE_CONTEXT};
use super::text::{Check, address, is_wide, row_label, status};
use super::{ElfView, Focus, Index, Phase};
use crate::file_list::filter_bar;
use crate::prelude::*;
use crate::primitives::VirtualList;
use crate::tokens::{self, Theme};
use crate::window_view::viewer::rich_header::{HeaderParts, header};
use crate::window_view::viewer::search::bar::{BarLook, bar_row};

const PER_MILLE: u64 = 1000;

fn select_row(
    view: WeakEntity<ElfView>,
    row: usize,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) {
    move |_, window, cx| {
        let _ = view.update(cx, |view, cx| {
            window.focus(&view.focus, cx);
            view.focus_in = Focus::Content;
            view.go_address = None;
            view.move_cursor(row, cx);
            view.open_inspector_if_asked(cx);
        });
    }
}

impl ElfView {
    fn row_view(&self, row: usize, view: &WeakEntity<Self>, cx: &App) -> AnyElement {
        let t = tokens::current(cx);
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
            .children(self.symbol_cells(row, symbol, cx))
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
                let this = entity.read(cx);
                range.map(|row| this.row_view(row, &view, cx)).collect()
            },
        )
        .label("Symbols")
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

    fn header_view(&self, t: &Theme) -> Div {
        let bar = self.go_to.as_ref().map(|go_to| {
            let look = BarLook {
                id: "elf-go-to",
                input_id: "elf-go-to-input",
                error_id: "elf-go-to-error",
                label: "Go to address",
                input: &go_to.input,
                is_find: false,
                match_case: None,
                count: None,
                error: go_to.error.clone(),
            };
            bar_row(t, look, |_, _, _| {})
        });
        header(t, self.header_parts()).children(bar)
    }

    fn checks(&self) -> Vec<Check> {
        let mut checks = match &self.index {
            Index::Ready(elf) => status(elf, self.lines.as_ref()),
            Index::Failed(error) => vec![Check {
                text: error.to_string(),
                is_bad: true,
            }],
            Index::Running => Vec::new(),
        };
        checks.extend(self.lines_error.as_ref().map(|error| Check {
            text: error.to_string(),
            is_bad: true,
        }));
        checks
    }

    fn status_line(&self, t: &Theme) -> impl IntoElement {
        let checks = self.checks();
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
        self.ensure_columns(window, cx);
        let table = div()
            .id("elf-table")
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .children(self.column_head())
            .child(self.list(cx))
            .child(self.drawers(&t, window, cx));
        let filter = self
            .is_filtering()
            .then(|| filter_bar(&t, self.query.text.clone().into(), self.rows.len()));
        let body = div()
            .id("elf-body")
            .key_context(TABLE_CONTEXT)
            .track_focus(&self.focus)
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(table)
            .children(filter);
        let root = div().id("elf-view").key_context(CONTEXT);
        Self::keys(root, cx)
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .size_full()
            .bg(t.colors.surface)
            .text_size(t.typography.body)
            .child(self.header_view(&t))
            .child(Self::typing(body, cx))
            .child(self.status_line(&t))
    }
}
