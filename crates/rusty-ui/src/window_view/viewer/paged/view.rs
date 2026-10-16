use gpui_kit::Role;
use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::{Stateful, Window};

use super::PagedView;
use super::find::{Bar, BarKind};
use super::lines::PagedKind;
use super::moves::Move;
use crate::format;
use crate::prelude::*;
use crate::tokens::{self, Theme};
use crate::window_view::viewer::body::Key;
use crate::window_view::viewer::search::bar::{BarLook, FIND_KEYS, LIST_KEYS, MARK_KEYS, bar_row};
use crate::window_view::viewer::search::count::count_label;
use crate::window_view::viewer::search::find_all::{
    close_find_all, find_all_panel, go_to_found, move_in_find_all,
};
use crate::window_view::viewer::{
    Back, FindAll, GoEnd, GoStart, ListClose, ListDown, ListGo, ListUp, MatchCase, NewerSearch,
    OlderSearch, OpenFind, OpenGoTo, PAGED_CONTEXT, PageDown, PageUp, RowDown, RowUp, ScrollLeft,
    ScrollRight,
};

const GO_TO_KEYS: [Key; 2] = [("Enter", "Go there"), ("Esc", "Close")];

fn small(t: &Theme, color: Hsla) -> Div {
    div()
        .flex_none()
        .text_size(t.typography.small)
        .text_color(color)
}

fn status(t: &Theme, id: &'static str, text: SharedString) -> ObservedElement<Stateful<Div>> {
    small(t, t.colors.muted)
        .id(id)
        .role(Role::Status)
        .aria_label(text.clone())
        .test_support()
        .child(text)
}

fn percent(done: u64, total: u64) -> u64 {
    if total == 0 {
        return 100;
    }
    (u128::from(done) * 100 / u128::from(total)).min(100) as u64
}

impl PagedView {
    fn mode_label(&self) -> &'static str {
        match self.kind {
            PagedKind::Hex => "Hex",
            PagedKind::Text => "Text · read-only",
        }
    }

    pub(in crate::window_view) fn bar_keys(&self) -> Option<&'static [Key]> {
        if self.marking.find_all.is_some() {
            return Some(&LIST_KEYS);
        }
        Some(match self.find.bar.as_ref()?.kind {
            BarKind::Find => &FIND_KEYS,
            BarKind::GoTo => &GO_TO_KEYS,
        })
    }

    pub(in crate::window_view) fn reading_keys(&self) -> Vec<Key> {
        let go_to = match self.kind {
            PagedKind::Hex => ("Ctrl+G", "Go to offset"),
            PagedKind::Text => ("Ctrl+G", "Go to line"),
        };
        let mut keys = vec![("Ctrl+F", "Find"), ("F3", "Next"), go_to];
        keys.extend(MARK_KEYS);
        if self.kind == PagedKind::Text {
            keys.extend([("Alt+L", "Line ends"), ("Alt+F", "Follow")]);
        }
        keys
    }

    fn index_status(&self) -> Option<SharedString> {
        if self.kind != PagedKind::Text || self.index_failed {
            return None;
        }
        let len = self.source.len();
        let index = self.index.as_ref();
        if !self.index_done {
            let scanned = index.map_or(0, |index| index.scanned());
            return Some(format!("Indexing… {}%", percent(scanned, len)).into());
        }
        let lines = self.line_count.unwrap_or(0);
        Some(format::count(lines, "line", "lines").into())
    }

    fn find_status(&self) -> Option<SharedString> {
        if let Some(progress) = &self.find.progress {
            let shown = percent(progress.done, progress.total);
            return Some(format!("Searching… {shown}%").into());
        }
        self.find.note.clone().or_else(|| self.note.clone())
    }

    fn count_text(&self) -> Option<SharedString> {
        let counted = self.marking.counted()?;
        let hit = self.find.hit.as_ref().map(|hit| hit.start);
        Some(count_label(counted, hit).into())
    }

    fn bar_view(
        &self,
        t: &Theme,
        bar: &Bar,
        cx: &mut Context<Self>,
    ) -> ObservedElement<Stateful<Div>> {
        let label = match (bar.kind, self.kind) {
            (BarKind::Find, _) => "Find",
            (BarKind::GoTo, PagedKind::Hex) => "Go to offset",
            (BarKind::GoTo, PagedKind::Text) => "Go to line",
        };
        let is_find = bar.kind == BarKind::Find;
        let is_text_find = is_find && self.kind == PagedKind::Text;
        let look = BarLook {
            id: "paged-bar",
            input_id: "paged-input",
            error_id: "paged-bar-error",
            label,
            input: &bar.input,
            is_find,
            match_case: is_text_find.then_some(self.find.match_case),
            count: self.count_text().filter(|_| is_find),
            error: bar.error.clone(),
        };
        bar_row(t, look, cx.listener(|view, _, _, cx| view.toggle_case(cx)))
    }

    fn header(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        let summary = self.summary.clone().filter(|_| self.kind == PagedKind::Hex);
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.md)
            .px(t.spacing.xl)
            .h(t.spacing.path_height)
            .border_b_1()
            .border_color(t.colors.line)
            .bg(t.colors.rail)
            .child(
                div()
                    .id("viewer-title")
                    .test_support()
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.title.clone()),
            )
            .child(small(t, t.colors.muted).child(self.mode_label()))
            .child(small(t, t.colors.muted).child(format::size(self.source.len())))
            .when_some(summary, |row, summary| {
                row.child(small(t, t.colors.muted).child(summary))
            })
            .when(self.changed && self.follow_label().is_none(), |row| {
                row.child(
                    status(t, "paged-changed", "Changed on disk".into()).text_color(t.colors.warn),
                )
            })
            .children(self.follow_label().map(|label| {
                status(t, "paged-follow", label.into()).text_color(t.colors.accent_text)
            }))
            .child(div().flex_1())
            .children(
                self.index_status()
                    .map(|text| status(t, "paged-index", text)),
            )
            .children(
                self.find_status()
                    .map(|text| status(t, "paged-status", text)),
            )
            .children(
                self.count_text()
                    .filter(|_| self.find.bar.is_none())
                    .map(|text| status(t, "find-count", text)),
            )
            .children(self.find.bar.as_ref().map(|bar| self.bar_view(t, bar, cx)))
    }

    fn movement_keys(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &RowUp, _, cx| view.scroll_rows(-1, cx)))
            .on_action(cx.listener(|view, _: &RowDown, _, cx| view.scroll_rows(1, cx)))
            .on_action(cx.listener(|view, _: &PageUp, _, cx| {
                view.scroll_rows(-view.page(), cx);
            }))
            .on_action(cx.listener(|view, _: &PageDown, _, cx| {
                view.scroll_rows(view.page(), cx);
            }))
            .on_action(cx.listener(|view, _: &GoStart, _, cx| {
                view.user_move(Move::Offset(0), cx);
            }))
            .on_action(cx.listener(|view, _: &GoEnd, _, cx| view.user_move(Move::End, cx)))
            .on_action(cx.listener(|view, _: &ScrollLeft, _, cx| view.shift_by(false, cx)))
            .on_action(cx.listener(|view, _: &ScrollRight, _, cx| view.shift_by(true, cx)))
    }

    fn find_keys(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &OpenFind, window, cx| {
            view.open_bar(BarKind::Find, window, cx);
        }))
        .on_action(cx.listener(|view, _: &OpenGoTo, window, cx| {
            view.open_bar(BarKind::GoTo, window, cx);
        }))
        .on_action(cx.listener(|view, _: &MatchCase, _, cx| view.toggle_case(cx)))
        .on_action(cx.listener(|view, _: &Back, window, cx| view.back(window, cx)))
        .on_action(cx.listener(|view, _: &OlderSearch, window, cx| {
            view.recall(true, window, cx);
        }))
        .on_action(cx.listener(|view, _: &NewerSearch, window, cx| {
            view.recall(false, window, cx);
        }))
        .on_action(cx.listener(|view, _: &FindAll, window, cx| {
            view.find_all_in_bar(window, cx);
        }))
    }

    fn list_keys(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &ListUp, _, cx| move_in_find_all(view, false, cx)))
            .on_action(cx.listener(|view, _: &ListDown, _, cx| move_in_find_all(view, true, cx)))
            .on_action(cx.listener(|view, _: &ListGo, window, cx| go_to_found(view, window, cx)))
            .on_action(cx.listener(|view, _: &ListClose, window, cx| {
                close_find_all(view, window, cx);
            }))
    }

    fn fitting_rows(&self, window: &Window) -> u64 {
        let measured = self.scroll.height();
        let height = if measured > px(0.) {
            measured
        } else {
            window.viewport_size().height
        };
        let rows = (height / self.row_height).floor();
        if rows.is_finite() && rows >= 1. {
            rows as u64
        } else {
            1
        }
    }
}

impl Render for PagedView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        self.row_height = t.typography.line_height + t.spacing.xxs;
        self.follow_drag(cx);
        self.rows = self.fitting_rows(window);
        self.follow_changes(cx);
        self.request_rows(cx);
        self.update_overview(&t);
        self.marking.sync_find_all();
        let rows = self.found_rows(cx);
        let panel = find_all_panel(&t, &self.marking, rows);
        let screen = self.cache.screen(self.want(), self.source.len());
        self.show_position(screen.end);
        let root = div()
            .id("paged")
            .key_context(PAGED_CONTEXT)
            .track_focus(&self.focus);
        let root = Self::find_keys(Self::movement_keys(root, cx), cx);
        Self::list_keys(root, cx)
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .size_full()
            .bg(t.colors.surface)
            .child(self.header(&t, cx))
            .child(self.body(&t, &screen, cx))
            .children(panel)
    }
}
