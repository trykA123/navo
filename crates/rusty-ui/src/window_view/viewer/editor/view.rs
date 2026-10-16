use gpui_kit::base::TestSupportExt;
use gpui_kit::component::input::{Editor, MovePageDown};
use gpui_kit::{Role, Stateful};

use super::EditorView;
use super::overlay::{OverlayLook, overlay};
use crate::format;
use crate::prelude::*;
use crate::tokens::{self, Theme};
use crate::window_view::viewer::search::GO_TO_CONTEXT;
use crate::window_view::viewer::search::bar::{BarLook, bar_row};
use crate::window_view::viewer::search::find_all::{
    close_find_all, find_all_panel, go_to_found, move_in_find_all,
};
use crate::window_view::viewer::search::overview::StripLook;
use crate::window_view::viewer::{
    Back, CopyPathLine, FindAll, FindSelection, ListClose, ListDown, ListGo, ListUp, MatchCase,
    NewerSearch, OlderSearch, OpenFind, OpenGoTo, PageDown,
};

impl EditorView {
    fn go_to_bar(&self, t: &Theme) -> Option<AnyElement> {
        let bar = self.go_to.as_ref()?;
        let look = BarLook {
            id: "editor-go-to",
            input_id: "editor-go-to-input",
            error_id: "editor-go-to-error",
            label: "Go to line",
            input: &bar.input,
            is_find: false,
            match_case: None,
            count: None,
            error: bar.error.clone(),
        };
        let row = bar_row(t, look, |_, _, _| {});
        Some(
            div()
                .key_context(GO_TO_CONTEXT)
                .child(row)
                .into_any_element(),
        )
    }

    fn bar(&self, t: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        if let Some(go_to) = self.go_to_bar(t) {
            return Some(go_to);
        }
        let bar = self.find.bar.as_ref()?;
        let look = BarLook {
            id: "editor-bar",
            input_id: "editor-input",
            error_id: "editor-bar-error",
            label: "Find",
            input: &bar.input,
            is_find: true,
            match_case: Some(self.find.match_case),
            count: self.count_text(),
            error: bar.error.clone(),
        };
        let row = bar_row(t, look, cx.listener(|view, _, _, cx| view.toggle_case(cx)));
        Some(row.into_any_element())
    }

    fn header(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        let mode = if self.editing {
            "Editing"
        } else {
            "Text · read-only"
        };
        let count = self.count_text().filter(|_| self.find.bar.is_none());
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
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.host.title.clone()),
            )
            .child(
                div()
                    .flex()
                    .gap(t.spacing.md)
                    .text_color(t.colors.muted)
                    .text_size(t.typography.small)
                    .child(mode)
                    .children(self.stamp.map(|stamp| format::size(stamp.len)))
                    .child(self.encoding.label()),
            )
            .when(self.dirty, |d| {
                d.child(div().text_color(t.colors.mark).child("●"))
            })
            .child(div().flex_1())
            .when_some(
                self.note.clone().or_else(|| self.preview_status(cx)),
                |d, note| d.child(status(t, "viewer-message", note)),
            )
            .when_some(count, |d, count| d.child(status(t, "find-count", count)))
            .children(self.bar(t, cx))
    }

    fn find_keys(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &OpenFind, window, cx| view.open_find(window, cx)))
            .on_action(cx.listener(|view, _: &Back, window, cx| view.back(window, cx)))
            .on_action(cx.listener(|view, _: &MatchCase, _, cx| view.toggle_case(cx)))
            .on_action(cx.listener(|view, _: &OlderSearch, window, cx| {
                view.recall(true, window, cx);
            }))
            .on_action(cx.listener(|view, _: &NewerSearch, window, cx| {
                view.recall(false, window, cx);
            }))
            .on_action(cx.listener(|view, _: &FindAll, window, cx| {
                view.find_all_in_bar(window, cx);
            }))
            .on_action(cx.listener(|view, _: &FindSelection, window, cx| {
                view.find_selection(window, cx);
            }))
            .on_action(cx.listener(|view, _: &OpenGoTo, window, cx| view.open_go_to(window, cx)))
            .on_action(cx.listener(|view, _: &CopyPathLine, _, cx| view.copy_path_line(cx)))
            .on_action(|_: &PageDown, window, cx| {
                window.dispatch_action(Box::new(MovePageDown), cx);
            })
    }

    fn list_keys(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &ListUp, _, cx| move_in_find_all(view, false, cx)))
            .on_action(cx.listener(|view, _: &ListDown, _, cx| move_in_find_all(view, true, cx)))
            .on_action(cx.listener(|view, _: &ListGo, window, cx| go_to_found(view, window, cx)))
            .on_action(cx.listener(|view, _: &ListClose, window, cx| {
                close_find_all(view, window, cx);
            }))
    }

    fn text_area(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let bookmarks: Vec<u64> = self.marking.bookmarks.iter().collect();
        let line_ends = self.marking.line_ends && !self.editing;
        let look = OverlayLook::new(t, bookmarks, line_ends);
        let this = cx.entity().downgrade();
        let theme = t.clone();
        let strip_look = StripLook {
            t,
            shown: self.marking.ticks(),
            entity: cx.entity_id(),
        };
        let strip = self.overview.element(strip_look, move |y, _, cx| {
            let _ = this.update(cx, |view, cx| view.overview_click(y, &theme, cx));
        });
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_hidden()
            .child(Editor::new(&self.editor).h_full().readonly(!self.editing))
            .child(overlay(self.editor.clone(), look))
            .child(strip)
    }
}

fn status(t: &Theme, id: &'static str, text: SharedString) -> impl IntoElement {
    div()
        .id(id)
        .role(Role::Status)
        .aria_label(text.clone())
        .test_support()
        .text_color(t.colors.muted)
        .text_size(t.typography.small)
        .child(text)
}

impl EditorView {
    fn body_row(&self, t: &Theme, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let text = self.text_area(t, cx).into_any_element();
        match self.split_pane(t, window, cx) {
            Some(pane) => div()
                .flex()
                .flex_1()
                .min_h_0()
                .child(text)
                .child(pane)
                .into_any_element(),
            None => text,
        }
    }
}

impl Render for EditorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        self.sync_decorations(&t, cx);
        self.update_overview(&t, cx);
        self.marking.sync_find_all();
        let rows = self.found_rows(cx);
        let panel = find_all_panel(&t, &self.marking, rows);
        let root = div().id("editor-view");
        Self::preview_keys(Self::list_keys(Self::find_keys(root, cx), cx), cx)
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .size_full()
            .child(self.header(&t, cx))
            .child(self.body_row(&t, window, cx))
            .children(panel)
    }
}
