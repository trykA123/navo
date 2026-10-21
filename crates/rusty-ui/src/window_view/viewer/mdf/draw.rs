use gpui_kit::base::TestSupportExt;
use gpui_kit::{ClickEvent, Role, WeakEntity};
use rusty_l10n::t;

use super::keys::CONTEXT;
use super::text::{Check, heads, spaces_detail, status};
use super::{Index, MdfView};
use crate::prelude::*;
use crate::primitives::{ColumnHead, VirtualList};
use crate::side_sheet::{SheetFrame, Side, SideSheet};
use crate::tokens::{self, Theme};
use crate::window_view::viewer::rich_header::{HeaderParts, header};

const INSPECTOR_WIDTH: f32 = 380.;

fn select_row(
    view: WeakEntity<MdfView>,
    row: usize,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) {
    move |_, window, cx| {
        let _ = view.update(cx, |view, cx| {
            window.focus(&view.focus, cx);
            view.move_cursor(row, cx);
            view.open_inspector_if_asked(cx);
        });
    }
}

impl MdfView {
    fn row_view(&self, row: usize, view: &WeakEntity<Self>, cx: &App) -> AnyElement {
        let t = tokens::current(cx);
        let (Some(loaded), Some(set)) = (self.loaded(), &self.columns) else {
            return div().into_any_element();
        };
        let Some(texts) = loaded.rows.get(row) else {
            return div().into_any_element();
        };
        let set = set.read(cx);
        let cells = texts.iter().enumerate().map(|(at, text)| {
            let cell = set.cell(at);
            let cell = if at >= 4 {
                cell.font_family(t.typography.mono_font.clone())
            } else if at == 2 {
                cell.font_family(t.typography.numbers_font.clone())
            } else {
                cell
            };
            cell.child(div().min_w_0().truncate().child(text.clone()))
        });
        div()
            .id(("mdf-row", row))
            .aria_label(texts.join(", "))
            .test_support()
            .flex()
            .w_full()
            .h_full()
            .cursor_pointer()
            .when(row % 2 == 1, |line| line.bg(t.colors.zebra))
            .when(row == self.cursor, |line| line.bg(t.colors.hover))
            .children(cells)
            .on_click(select_row(view.clone(), row))
            .into_any_element()
    }

    fn list(&self, cx: &mut Context<Self>) -> VirtualList {
        let view = cx.weak_entity();
        VirtualList::new(
            "mdf-list",
            self.row_count(),
            &self.scroll,
            move |range, _, cx| {
                let Some(entity) = view.upgrade() else {
                    return Vec::new();
                };
                let this = entity.read(cx);
                range.map(|row| this.row_view(row, &view, cx)).collect()
            },
        )
        .label("Chips")
    }

    fn progress(&self) -> Option<SharedString> {
        match &self.index {
            Index::Running => Some("Reading…".into()),
            _ => self.note.clone(),
        }
    }

    fn header_parts(&self) -> HeaderParts {
        HeaderParts {
            title: self.title.clone(),
            mode: t!("viewer-mdf-mode").into(),
            len: self.len,
            detail: self
                .loaded()
                .map(|loaded| ("mdf-spaces", spaces_detail(&loaded.mdf).into())),
            status: self.progress().map(|text| ("mdf-progress", text)),
        }
    }

    fn checks(&self) -> Vec<Check> {
        match &self.index {
            Index::Ready(loaded) => status(&loaded.mdf),
            Index::Failed(error) => vec![Check {
                text: error.to_string(),
                is_bad: true,
            }],
            Index::Running => Vec::new(),
        }
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
                    .id(("mdf-status-part", at))
                    .test_support()
                    .flex()
                    .gap(t.spacing.sm)
                    .when(is_bad, |part| part.flex_none())
                    .when(!is_bad, |part| part.min_w_0().overflow_hidden())
                    .when(at > 0, |part| {
                        part.child(div().flex_none().text_color(t.colors.faint).child("·"))
                    })
                    .child(div().min_w_0().truncate().text_color(color).child(text))
            });
        div()
            .id("mdf-status")
            .role(Role::Status)
            .aria_label(label)
            .test_support()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.sm)
            .h(t.spacing.status_height)
            .px(t.spacing.xl)
            .min_w_0()
            .overflow_hidden()
            .border_t_1()
            .border_color(t.colors.line)
            .bg(t.colors.rail)
            .text_size(t.typography.small)
            .children(parts)
    }

    fn inspector_body(&self, t: &Theme) -> AnyElement {
        let body = div()
            .id("mdf-inspector-text")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .gap(t.spacing.sm)
            .p(t.spacing.xl);
        let Some(texts) = self
            .loaded()
            .and_then(|loaded| loaded.rows.get(self.cursor))
        else {
            return body
                .child(
                    div()
                        .text_color(t.colors.muted)
                        .child(t!("viewer-mdf-nothing")),
                )
                .into_any_element();
        };
        let heads = heads();
        let spoken = heads
            .iter()
            .zip(texts)
            .map(|(name, text)| format!("{name}: {text}"))
            .collect::<Vec<_>>()
            .join(", ");
        body.aria_label(spoken)
            .test_support()
            .children(heads.iter().zip(texts).map(|(name, text)| {
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(t.typography.tiny)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.colors.muted)
                            .child(name.clone()),
                    )
                    .child(div().font_family(t.typography.mono_font.clone()).child(
                        if text.is_empty() {
                            "not mapped".to_owned()
                        } else {
                            text.clone()
                        },
                    ))
            }))
            .into_any_element()
    }

    fn drawers(&self, t: &Theme, window: &Window, cx: &mut Context<Self>) -> Div {
        let this = cx.weak_entity();
        let name = self
            .loaded()
            .and_then(|loaded| loaded.rows.get(self.cursor))
            .and_then(|texts| texts.first().cloned())
            .unwrap_or_default();
        let width = window.viewport_size().width - t.spacing.md * 2.;
        let frame = SheetFrame::new(width, px(INSPECTOR_WIDTH), Side::Right.other())
            .covering_passive_card();
        let inspector = self.inspector.read(cx).is_shown(cx).then(|| {
            SideSheet::new("mdf-inspector", &self.inspector, frame)
                .label("Inspector")
                .header(
                    div()
                        .min_w_0()
                        .truncate()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(name),
                )
                .close("ctrl-i", move |_, cx| {
                    let _ = this.update(cx, |view, cx| view.toggle_inspector(cx));
                })
                .body(self.inspector_body(t))
        });
        div()
            .absolute()
            .top(t.spacing.xs)
            .bottom(t.spacing.xs)
            .left(t.spacing.md)
            .right(t.spacing.md)
            .children(inspector)
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

impl Render for MdfView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        self.measure(window, &t);
        self.ensure_columns(window, cx);
        let head = self
            .columns
            .as_ref()
            .map(|set| ColumnHead::new("mdf-head", set));
        let table = div()
            .id("mdf-table")
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .children(head)
            .child(self.list(cx))
            .child(self.drawers(&t, window, cx));
        let root = div()
            .id("mdf-view")
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
            .child(self.status_line(&t))
    }
}
