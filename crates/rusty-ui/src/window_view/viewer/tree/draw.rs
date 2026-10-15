use gpui_kit::Role;
use gpui_kit::base::TestSupportExt;

use super::keys::CONTEXT;
use super::row::{RowLook, tree_row};
use super::{TreeState, TreeView};
use crate::file_list::filter_bar;
use crate::format;
use crate::prelude::*;
use crate::primitives::{Kbd, VirtualList};
use crate::tokens::{self, Theme};
use crate::window_view::viewer::rich_header::{HeaderParts, header};

const PAGE_MARGIN_ROWS: usize = 4;

pub(in crate::window_view::viewer) fn message_card(
    t: &Theme,
    id: &'static str,
    title: SharedString,
    detail: SharedString,
) -> impl IntoElement {
    let c = &t.colors;
    let hint = div()
        .flex()
        .items_center()
        .gap(t.spacing.sm)
        .text_size(t.typography.small)
        .text_color(c.muted)
        .children(Kbd::new("open-as-text", "1"))
        .child("Open as text");
    div()
        .flex_1()
        .flex()
        .items_start()
        .justify_center()
        .pt(t.spacing.xxl * 3.)
        .child(
            div()
                .id(id)
                .role(Role::Alert)
                .aria_label(detail.clone())
                .test_support()
                .flex()
                .flex_col()
                .gap(t.spacing.md)
                .max_w(t.spacing.rail_width * 2.5)
                .p(t.spacing.xxl)
                .rounded(t.radii.lg)
                .border_1()
                .border_color(c.line)
                .bg(c.rail)
                .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
                .child(
                    div()
                        .font_family(t.typography.mono_font.clone())
                        .text_size(t.typography.small)
                        .text_color(c.text)
                        .child(detail),
                )
                .child(hint),
        )
}

impl TreeView {
    pub(super) fn toggle_row(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        if let Some(ready) = self.ready_mut() {
            let step = ready.outline.toggle(&ready.tree, index);
            self.follow(step, cx);
        }
    }

    pub(super) fn click_row(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        if let Some(ready) = self.ready_mut() {
            ready.outline.move_to(index);
            self.note = None;
            cx.notify();
        }
    }

    fn status(&self) -> Option<(&'static str, SharedString)> {
        let text: SharedString = match &self.state {
            TreeState::Indexing(count) => format!(
                "Indexing… {} · Esc stops",
                format::count(*count, "element", "elements")
            )
            .into(),
            TreeState::Stopped => "Indexing stopped".into(),
            TreeState::Failed(_) => return None,
            TreeState::Ready(ready) => {
                if let Some(note) = &self.note {
                    note.clone()
                } else if !self.loads.is_empty() {
                    "Loading…".into()
                } else {
                    format::count(ready.elements?, "element", "elements").into()
                }
            }
        };
        Some(("tree-status", text))
    }

    fn rows_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(ready) = self.ready() else {
            return div().into_any_element();
        };
        let view = cx.weak_entity();
        let len = ready.outline.rows().len();
        VirtualList::new("tree-list", len, &self.scroll, move |range, _, cx| {
            let Some(entity) = view.upgrade() else {
                return Vec::new();
            };
            let t = tokens::current(cx).clone();
            let this = entity.read(cx);
            let Some(ready) = this.ready() else {
                return Vec::new();
            };
            let rows = ready.outline.rows();
            range
                .filter_map(|index| {
                    let row = *rows.get(index)?;
                    let look = RowLook {
                        t: &t,
                        tree: &ready.tree,
                        row,
                        index,
                        is_cursor: index == ready.outline.cursor(),
                        is_open: ready.outline.is_open(row.node),
                        is_loading: this.loads.iter().any(|(node, _)| *node == row.node),
                    };
                    Some(tree_row(&look, &view))
                })
                .collect()
        })
        .label("Tree")
        .into_any_element()
    }

    fn body(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        match &self.state {
            TreeState::Ready(_) => self.rows_view(cx),
            TreeState::Indexing(count) => div()
                .flex_1()
                .p(t.spacing.xxl)
                .text_color(t.colors.muted)
                .child(format!(
                    "Reading the structure of {}… {} so far.",
                    self.title,
                    format::count(*count, "element", "elements")
                ))
                .into_any_element(),
            TreeState::Failed(error) => message_card(
                t,
                "tree-error",
                format!("This is not valid {}.", self.format.name()).into(),
                error.to_string().into(),
            )
            .into_any_element(),
            TreeState::Stopped => message_card(
                t,
                "tree-error",
                "Indexing stopped.".into(),
                "The tree was not built. The file is unchanged.".into(),
            )
            .into_any_element(),
        }
    }

    fn filter_view(&self, t: &Theme) -> Option<impl IntoElement> {
        let ready = self.ready()?;
        let filter = ready.outline.filter();
        if filter.is_empty() {
            return None;
        }
        let matches = ready.outline.matching(&ready.tree);
        Some(filter_bar(t, filter.to_owned().into(), matches))
    }
}

impl Render for TreeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let rows = (window.viewport_size().height / t.spacing.row_height).floor();
        self.page = if rows.is_finite() && rows > 0. {
            (rows as usize).saturating_sub(PAGE_MARGIN_ROWS).max(1)
        } else {
            1
        };
        let parts = HeaderParts {
            title: self.title.clone(),
            mode: self.format.label().into(),
            len: self.len,
            detail: self.selected_path().map(|path| ("tree-path", path)),
            status: self.status(),
        };
        let root = div()
            .id("tree")
            .key_context(CONTEXT)
            .track_focus(&self.focus);
        let root = Self::commands(Self::moves(root, cx), cx);
        root.flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .size_full()
            .bg(t.colors.surface)
            .text_size(t.typography.body)
            .child(header(&t, parts))
            .children(self.filter_view(&t))
            .child(self.body(&t, cx))
    }
}
