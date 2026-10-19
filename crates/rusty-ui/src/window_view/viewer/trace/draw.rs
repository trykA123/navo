use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::{ClickEvent, Role, Stateful, WeakEntity};

use super::keys::CONTEXT;
use super::text::{Check, Tone, column_titles, row, status};
use super::{Focus, TraceView};
use crate::format;
use crate::prelude::*;
use crate::primitives::VirtualList;
use crate::tokens::{self, Theme};
use crate::window_view::viewer::rich_header::{HeaderParts, header};

fn mono(t: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .px(t.spacing.xl)
        .whitespace_nowrap()
        .overflow_hidden()
        .font_family(t.typography.mono_font.clone())
}

fn select_row(
    view: WeakEntity<TraceView>,
    shown: u64,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) {
    move |_, window, cx| {
        let _ = view.update(cx, |view, cx| {
            window.focus(&view.focus, cx);
            view.focus_in = Focus::Content;
            view.follow_moved(shown + 1 >= view.shown_rows());
            view.move_cursor(shown, cx);
        });
    }
}

fn tone_color(t: &Theme, tone: Tone) -> Hsla {
    match tone {
        Tone::Plain => t.colors.text,
        Tone::Error | Tone::Bad => t.colors.warn,
        Tone::Quiet => t.colors.muted,
    }
}

fn status_part(t: &Theme, at: usize, Check { text, is_bad }: Check) -> Div {
    let color = if is_bad {
        t.colors.warn
    } else {
        t.colors.muted
    };
    div()
        .flex()
        .gap(t.spacing.sm)
        .when(at > 0, |part| {
            part.child(div().text_color(t.colors.faint).child("·"))
        })
        .child(div().text_color(color).child(text))
}

impl TraceView {
    fn row_view(&self, t: &Theme, shown: u64, view: &WeakEntity<Self>) -> AnyElement {
        let base = self.trace.notation().base;
        let (text, tone) = self
            .entry(shown)
            .map_or_else(|| (String::new(), Tone::Quiet), |entry| row(entry, base));
        let is_error = tone == Tone::Error;
        let element: ObservedElement<Stateful<Div>> = mono(t)
            .id(("trace-row", shown as usize))
            .aria_label(text.clone())
            .test_support();
        element
            .h_full()
            .w_full()
            .text_color(tone_color(t, tone))
            .when(is_error, |row| {
                row.bg(t.colors.warn.opacity(0.08))
                    .border_l_2()
                    .border_color(t.colors.warn)
            })
            .when(shown == self.cursor, |row| row.bg(t.colors.hover))
            .on_click(select_row(view.clone(), shown))
            .child(text)
            .into_any_element()
    }

    fn list(&self, cx: &mut Context<Self>) -> VirtualList {
        let view = cx.weak_entity();
        let count = usize::try_from(self.shown_rows()).unwrap_or(usize::MAX);
        VirtualList::new("trace-list", count, &self.scroll, move |range, _, cx| {
            let Some(entity) = view.upgrade() else {
                return Vec::new();
            };
            let t = tokens::current(cx).clone();
            let this = entity.read(cx);
            range
                .map(|shown| this.row_view(&t, shown as u64, &view))
                .collect()
        })
        .label("CAN frames")
    }

    fn column_head(t: &Theme) -> Div {
        mono(t)
            .flex_none()
            .h(t.spacing.row_height)
            .text_size(t.typography.small)
            .text_color(t.colors.muted)
            .bg(t.colors.rail)
            .border_b_1()
            .border_color(t.colors.line)
            .child(column_titles())
    }

    fn progress(&self) -> Option<SharedString> {
        if !self.is_indexed && self.reading.is_some() {
            let done = (u128::from(self.scanned) * 100 / u128::from(self.len.max(1))).min(100);
            return Some(format!("Reading frames… {done}%").into());
        }
        self.note.clone()
    }

    fn header_parts(&self) -> HeaderParts {
        let rows = self.shown_rows();
        let detail = (rows > 0).then(|| {
            let at = format!(
                "row {} of {}",
                format::bytes(self.cursor + 1),
                format::bytes(rows)
            );
            ("trace-cursor", at.into())
        });
        HeaderParts {
            title: self.title.clone(),
            mode: "Vector ASC".into(),
            len: self.len,
            detail,
            status: self.progress().map(|text| ("trace-progress", text)),
        }
    }

    fn checks(&self) -> Vec<Check> {
        let mut checks = status(&self.trace);
        if let (Some(name), Some(filter)) = (self.filter_name(), &self.filter) {
            let shown = format::count(filter.rows.len() as u64, "frame", "frames");
            checks.insert(
                0,
                Check {
                    text: format!("ID {name}: {shown} · Esc shows all"),
                    is_bad: false,
                },
            );
        }
        checks.extend(self.file_checks());
        if let Some(label) = self.follow_label() {
            checks.push(Check {
                text: label.to_owned(),
                is_bad: false,
            });
        }
        checks
    }

    fn file_checks(&self) -> Vec<Check> {
        let errors = [&self.index_error, &self.read_error];
        let mut checks: Vec<Check> = errors
            .into_iter()
            .flatten()
            .map(|error| Check {
                text: error.to_string(),
                is_bad: true,
            })
            .collect();
        let is_settled = self.is_indexed && self.reading.is_none();
        if is_settled && self.trace.end < self.len {
            checks.push(Check {
                text: "last line incomplete".to_owned(),
                is_bad: false,
            });
        }
        if self.is_replaced {
            checks.push(Check {
                text: "file was replaced".to_owned(),
                is_bad: false,
            });
        }
        checks
    }

    fn status_line(&self, t: &Theme) -> impl IntoElement {
        let checks = self.checks();
        let label = checks
            .iter()
            .map(|check| check.text.as_str())
            .collect::<Vec<_>>()
            .join(" · ");
        div()
            .id("trace-status")
            .role(Role::Status)
            .aria_label(label)
            .test_support()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.sm)
            .h(t.spacing.status_height)
            .px(t.spacing.xl)
            .border_t_1()
            .border_color(t.colors.line)
            .bg(t.colors.rail)
            .text_size(t.typography.small)
            .children(
                checks
                    .into_iter()
                    .enumerate()
                    .map(|(at, check)| status_part(t, at, check)),
            )
    }

    fn measure(&mut self, window: &Window, t: &Theme) {
        let rows = (window.viewport_size().height / t.spacing.row_height).floor();
        self.page = if rows.is_finite() && rows > 6. {
            rows as u64 - 6
        } else {
            1
        };
    }
}

impl Render for TraceView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        self.measure(window, &t);
        let top = self.scroll.top_row(t.spacing.row_height) as u64;
        self.request_rows(top, cx);
        self.request_inspect(cx);
        let table = div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(Self::column_head(&t))
            .child(self.list(cx))
            .child(self.drawers(&t, window, cx));
        let root = div()
            .id("can-trace")
            .key_context(CONTEXT)
            .track_focus(&self.focus);
        let root = Self::moves(root, cx);
        Self::commands(root, cx)
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
