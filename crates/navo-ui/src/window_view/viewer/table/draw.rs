use std::ops::Range;
use std::sync::Arc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{BoxShadow, ClickEvent, MouseButton, MouseDownEvent, Role, WeakEntity, point};

use super::keys::CONTEXT;
use super::rows::Rows;
use super::{SortKey, TableView};
use crate::format;
use crate::prelude::*;
use crate::primitives::{IconName, VirtualList, icon};
use crate::tokens::{self, Theme};
use crate::window_view::viewer::rich_header::{HeaderParts, header};
use crate::window_view::viewer::search::view_find::{find_strip, hit_tint};

const CHAR_WIDTH: f32 = 7.2;

fn column_px(t: &Theme, chars: usize) -> Pixels {
    px(chars as f32 * CHAR_WIDTH) + t.spacing.md * 2.
}

fn number_px(t: &Theme, rows: usize) -> Pixels {
    let digits = rows.max(1).to_string().len().max(2);
    column_px(t, digits)
}

fn ring(color: Hsla) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color,
        offset: point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(1.),
        inset: true,
    }]
}

fn is_number(cell: &str) -> bool {
    let cell = cell.trim();
    !cell.is_empty() && cell.parse::<f64>().is_ok()
}

fn delimiter_name(delimiter: u8) -> &'static str {
    match delimiter {
        b';' => "Table · semicolons",
        b'\t' => "Table · tabs",
        b'|' => "Table · pipes",
        _ => "Table · commas",
    }
}

fn sort_arrow(sort: SortKey, color: Hsla) -> impl IntoElement {
    let arrow = if sort.descending {
        IconName::ArrowDown
    } else {
        IconName::ArrowUp
    };
    icon(arrow, px(10.), color)
}

fn select_cell<E>(
    view: WeakEntity<TableView>,
    cell: (usize, usize),
) -> impl Fn(&E, &mut Window, &mut App) + 'static {
    move |_, window, cx| {
        let _ = view.update(cx, |view, cx| view.click_cell(cell, window, cx));
    }
}

struct Frame<'a> {
    t: &'a Theme,
    columns: Range<usize>,
    widths: &'a [usize],
    number: Pixels,
}

impl Frame<'_> {
    fn width(&self, column: usize) -> Pixels {
        column_px(self.t, self.widths.get(column).copied().unwrap_or(3))
    }
}

fn cell_view(frame: &Frame<'_>, column: usize, text: &str, is_cursor: bool) -> Div {
    let t = frame.t;
    let c = &t.colors;
    div()
        .flex()
        .flex_none()
        .items_center()
        .w(frame.width(column))
        .h_full()
        .px(t.spacing.md)
        .border_r_1()
        .border_color(c.line.opacity(0.5))
        .when(is_number(text), |cell| {
            cell.justify_end()
                .font_family(t.typography.numbers_font.clone())
                .font_features(t.typography.numbers_font_features.clone())
        })
        .when(is_cursor, |cell| {
            cell.bg(c.selection)
                .shadow(ring(c.accent))
                .when(t.is_contrast, |cell| cell.text_color(c.on_selection))
        })
        .child(div().min_w_0().truncate().child(text.to_owned()))
}

fn number_cell(frame: &Frame<'_>, text: String) -> Div {
    let t = frame.t;
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_end()
        .w(frame.number)
        .h_full()
        .px(t.spacing.md)
        .border_r_1()
        .border_color(t.colors.line)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .font_family(t.typography.numbers_font.clone())
        .font_features(t.typography.numbers_font_features.clone())
        .child(text)
}

impl TableView {
    pub(super) fn shows_column(&self, column: usize) -> bool {
        let Some(available) = self.available else {
            return true;
        };
        let used: f32 = (self.left..=column)
            .map(|at| {
                let chars = self.widths.get(at).copied().unwrap_or(3) as f32;
                f32::from(px(chars * CHAR_WIDTH) + self.padding)
            })
            .sum();
        px(used) <= available
    }

    fn visible_columns(&self, frame_t: &Theme, available: Pixels) -> Range<usize> {
        let mut used = px(0.);
        let mut end = self.left;
        while end < self.columns() {
            used += column_px(frame_t, self.widths.get(end).copied().unwrap_or(3));
            end += 1;
            if used >= available {
                break;
            }
        }
        self.left..end
    }

    fn head(&self, frame: &Frame<'_>, cx: &mut Context<Self>) -> Div {
        let t = frame.t;
        let c = &t.colors;
        let cells = frame.columns.clone().map(|column| {
            let name = self.header.get(column).cloned().unwrap_or_default();
            let sorted = self.sort.filter(|sort| sort.column == column);
            div()
                .id(("table-head", column))
                .role(Role::Button)
                .aria_label(name.clone())
                .test_support()
                .flex()
                .flex_none()
                .items_center()
                .gap(t.spacing.xs)
                .w(frame.width(column))
                .h_full()
                .px(t.spacing.md)
                .cursor_pointer()
                .border_r_1()
                .border_color(c.line)
                .when(sorted.is_some(), |cell| cell.text_color(c.text))
                .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                    window.focus(&view.focus, cx);
                    view.sort_by(column, cx);
                }))
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |view, _: &MouseDownEvent, window, cx| {
                        view.click_cell((view.cursor.0, column), window, cx);
                    }),
                )
                .child(div().min_w_0().truncate().child(name))
                .children(sorted.map(|sort| sort_arrow(sort, c.accent_text)))
        });
        div()
            .flex()
            .flex_none()
            .h(t.spacing.row_height)
            .text_size(t.typography.tiny)
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(c.muted)
            .bg(c.rail)
            .border_b_1()
            .border_color(c.line)
            .child(number_cell(frame, "#".to_owned()))
            .children(cells)
    }

    fn row_view(&self, frame: &Frame<'_>, index: usize, view: &WeakEntity<Self>) -> AnyElement {
        let t = frame.t;
        let c = &t.colors;
        let (row, column) = self.cursor;
        let found = self.rows.get(index);
        let label = found.map_or_else(
            || format!("{}: …", index + 1),
            |(number, cells)| format!("{number}: {}", cells.join(", ")),
        );
        let number = found.map_or(index as u64 + 1, |(number, _)| number);
        let cells: Vec<&str> = frame
            .columns
            .clone()
            .map(|at| {
                found
                    .and_then(|(_, cells)| cells.get(at))
                    .map_or(if found.is_some() { "" } else { "…" }, String::as_str)
            })
            .collect();
        let view = view.clone();
        div()
            .id(("table-row", index))
            .aria_label(label)
            .test_support()
            .flex()
            .w_full()
            .h_full()
            .when(index % 2 == 1, |line| line.bg(c.zebra))
            .when(index == row, |line| line.bg(c.hover))
            .child(number_cell(frame, number.to_string()))
            .children(frame.columns.clone().zip(cells).map(|(at, text)| {
                let is_cursor = index == row && at == column;
                cell_view(frame, at, text, is_cursor)
                    .id(("table-cell", at))
                    .test_support()
                    .when(!is_cursor, |cell| match self.found_cell((index, at)) {
                        Some(is_current) => cell.bg(hit_tint(t, is_current)),
                        None => cell,
                    })
                    .on_click(select_cell::<ClickEvent>(view.clone(), (index, at)))
                    .on_mouse_down(
                        MouseButton::Right,
                        select_cell::<MouseDownEvent>(view.clone(), (index, at)),
                    )
            }))
            .into_any_element()
    }

    fn status(&self) -> Option<(&'static str, SharedString)> {
        let text: SharedString = if let Some(note) = &self.note {
            note.clone()
        } else if self.sorting.is_some() {
            "Sorting…".into()
        } else if let Rows::Indexed(indexed) = &self.rows
            && indexed.index.is_none()
        {
            let done = (u128::from(indexed.scanned) * 100 / u128::from(self.len.max(1))).min(100);
            format!("Indexing rows… {done}%").into()
        } else {
            format::count(self.rows.len() as u64, "row", "rows").into()
        };
        Some(("table-status", text))
    }
}

impl TableView {
    fn click_cell(&mut self, cell: (usize, usize), window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        self.move_to(cell.0, cell.1, cx);
    }

    fn measure(&mut self, window: &Window, t: &Theme) -> (Pixels, Pixels) {
        let viewport = window.viewport_size();
        let rows = (viewport.height / t.spacing.row_height).floor();
        self.page = if rows.is_finite() && rows > 4. {
            rows as usize - 4
        } else {
            1
        };
        let number = number_px(t, self.rows.len());
        let available = viewport.width - number - t.spacing.xl;
        self.available = Some(available);
        self.padding = t.spacing.md * 2.;
        (number, available)
    }

    fn header_parts(&self) -> HeaderParts {
        let (row, column) = self.cursor;
        let detail = self
            .header
            .get(column)
            .map(|name| ("table-cell", format!("Row {} · {name}", row + 1).into()));
        HeaderParts {
            title: self.title.clone(),
            mode: delimiter_name(self.dialect.delimiter).into(),
            len: self.len,
            detail,
            status: self.status(),
        }
    }

    fn list(&self, columns: Range<usize>, number: Pixels, cx: &mut Context<Self>) -> VirtualList {
        let view = cx.weak_entity();
        let widths = Arc::clone(&self.widths);
        VirtualList::new(
            "table-list",
            self.rows.len(),
            &self.scroll,
            move |range, _, cx| {
                let Some(entity) = view.upgrade() else {
                    return Vec::new();
                };
                let t = tokens::current(cx).clone();
                let frame = Frame {
                    t: &t,
                    columns: columns.clone(),
                    widths: &widths,
                    number,
                };
                let this = entity.read(cx);
                range
                    .map(|index| this.row_view(&frame, index, &view))
                    .collect()
            },
        )
        .label("Table")
    }
}

impl Render for TableView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let (number, available) = self.measure(window, &t);
        let top = self.scroll.top_row(t.spacing.row_height);
        self.request_rows(top, cx);
        let columns = self.visible_columns(&t, available);
        let frame = Frame {
            t: &t,
            columns: columns.clone(),
            widths: &self.widths,
            number,
        };
        let head = self.head(&frame, cx);
        let list = self.list(columns, number, cx);
        let root = div()
            .id("table")
            .key_context(CONTEXT)
            .track_focus(&self.focus);
        Self::find_keys(Self::keys(root, cx), cx)
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .size_full()
            .bg(t.colors.surface)
            .text_size(t.typography.body)
            .child(header(&t, self.header_parts()))
            .children(self.find.as_ref().map(|find| find_strip(&t, find)))
            .child(head)
            .child(list)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::{Mode, SystemPalette, contrast_theme};
    use gpui_kit::{black, rgb, white};

    #[test]
    fn selected_cell_uses_system_highlight_text() {
        let palette = SystemPalette {
            window: black(),
            window_text: white(),
            highlight: white(),
            highlight_text: black(),
            button_face: black(),
            button_text: white(),
            gray_text: rgb(0x767676).into(),
            hot_light: white(),
        };
        let theme = contrast_theme(&Theme::new(Mode::Dark), &palette);
        let frame = Frame {
            t: &theme,
            columns: 0..1,
            widths: &[8],
            number: px(20.),
        };
        let mut selected = cell_view(&frame, 0, "value", true);
        assert_eq!(selected.text_style().color, Some(palette.highlight_text));
        let mut plain = cell_view(&frame, 0, "value", false);
        assert_ne!(plain.text_style().color, Some(palette.highlight_text));
    }
}
