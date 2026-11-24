//! The tree query bar (Ctrl+Shift+F): a JSONPath-like query over the tree view.
//! It highlights the selected nodes and steps through them with Enter and Shift+Enter.

use std::time::Duration;

use gpui_kit::base::TestSupportExt;
use gpui_kit::component::input::InputEvent;
use gpui_kit::{Focusable, Role, Subscription, Task};
use navo_l10n::{t, tr};
use navo_structured::NodeId;
use navo_structured::tree::Query;

use super::TreeView;
use crate::prelude::*;
use crate::primitives::{TextField, TextFieldState, text_field_state};
use crate::tokens::Theme;
use crate::window_view::viewer::search::view_find::{Found, hit_tint};

/// The pause after the last keystroke before the query runs (debounce).
const QUERY_DELAY: Duration = Duration::from_millis(150);

/// The open query bar: its field, the last result or parse error, and the debounce timer.
pub(super) struct TreeQuery {
    input: Entity<TextFieldState>,
    found: Option<Found<NodeId>>,
    error: Option<SharedString>,
    wait: Option<Task<()>>,
    _events: Subscription,
}

impl TreeView {
    /// Opens the query bar, or focuses it when it is already open.
    pub(super) fn open_query(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ready().is_none() {
            return;
        }
        if let Some(query) = &self.query {
            let focus = query.input.read(cx).focus_handle(cx);
            return window.focus(&focus, cx);
        }
        let input = text_field_state(tr("tree-query-placeholder"), window, cx);
        let events =
            cx.subscribe_in(
                &input,
                window,
                |view, _, event: &InputEvent, _, cx| match event {
                    InputEvent::PressEnter { shift, .. } => view.step_query(*shift, cx),
                    InputEvent::Change => view.schedule_query(cx),
                    _ => {}
                },
            );
        window.focus(&input.read(cx).focus_handle(cx), cx);
        self.query = Some(TreeQuery {
            input,
            found: None,
            error: None,
            wait: None,
            _events: events,
        });
        cx.notify();
    }

    /// Closes the query bar and returns focus to the tree; false when it was not open.
    pub(super) fn close_query(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.query.take().is_none() {
            return false;
        }
        window.focus(&self.focus, cx);
        cx.notify();
        true
    }

    /// Reports whether the query field holds the keyboard focus.
    pub(super) fn is_query_focused(&self, window: &Window, cx: &App) -> bool {
        self.query.as_ref().is_some_and(|query| {
            query
                .input
                .read(cx)
                .focus_handle(cx)
                .contains_focused(window, cx)
        })
    }

    fn schedule_query(&mut self, cx: &mut Context<Self>) {
        let wait = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(QUERY_DELAY).await;
            let _ = this.update(cx, Self::run_query);
        });
        if let Some(query) = self.query.as_mut() {
            query.wait = Some(wait);
        }
    }

    /// Parses the field's text and selects its nodes on the UI thread.
    /// Inline is enough: a 5 MB file takes at most about 25 ms in a release build.
    fn run_query(&mut self, cx: &mut Context<Self>) {
        let (Some(query), Some(ready)) = (self.query.as_ref(), self.ready()) else {
            return;
        };
        let text = query.input.read(cx).value().to_string();
        let (found, error) = if text.is_empty() {
            (None, None)
        } else {
            match Query::parse(&text) {
                Ok(parsed) => (Some(Found::new(ready.tree.select(&parsed), 0)), None),
                Err(error) => {
                    let message = t!(
                        "tree-query-error",
                        message = error.message,
                        column = error.column
                    );
                    (None, Some(message.into()))
                }
            }
        };
        let current = found.as_ref().and_then(Found::current);
        if let Some(query) = self.query.as_mut() {
            query.wait = None;
            query.found = found;
            query.error = error;
        }
        if let Some(node) = current {
            self.reveal(node, cx);
        }
        cx.notify();
    }

    fn step_query(&mut self, backward: bool, cx: &mut Context<Self>) {
        let node = self
            .query
            .as_mut()
            .and_then(|query| query.found.as_mut())
            .and_then(|found| found.step(backward));
        if let Some(node) = node {
            self.reveal(node, cx);
        }
    }

    /// Tints a row the query selected; the current match gets the stronger tint.
    pub(super) fn tint_query(&self, t: &Theme, node: NodeId, row: AnyElement) -> AnyElement {
        let Some(found) = self
            .query
            .as_ref()
            .and_then(|query| query.found.as_ref())
            .filter(|found| found.contains(node))
        else {
            return row;
        };
        div()
            .size_full()
            .bg(hit_tint(t, found.current() == Some(node)))
            .child(row)
            .into_any_element()
    }

    /// Draws the query bar with its match count, and the parse error under it.
    pub(super) fn query_strip(&self, t: &Theme) -> Option<impl IntoElement> {
        let query = self.query.as_ref()?;
        let label: SharedString = tr("tree-query-label").into();
        let count = query
            .found
            .as_ref()
            .map(|found| SharedString::from(found.label()));
        let bar = div()
            .flex()
            .items_center()
            .gap(t.spacing.sm)
            .h(t.spacing.path_height)
            .child(small(t, t.colors.muted).child(label.clone()))
            .child(div().w(t.spacing.rail_width).child(TextField::new(
                "tree-query-input",
                &query.input,
                label,
            )))
            .children(count.map(|count| {
                small(t, t.colors.muted)
                    .id("tree-query-count")
                    .role(Role::Status)
                    .aria_label(count.clone())
                    .test_support()
                    .child(count)
            }));
        let error = query.error.clone().map(|error| {
            small(t, t.colors.mark)
                .id("tree-query-error")
                .role(Role::Alert)
                .aria_label(error.clone())
                .test_support()
                .pb(t.spacing.sm)
                .child(error)
        });
        Some(
            div()
                .id("tree-query")
                .flex()
                .flex_col()
                .flex_none()
                .px(t.spacing.xl)
                .border_b_1()
                .border_color(t.colors.line)
                .bg(t.colors.rail)
                .child(bar)
                .children(error),
        )
    }
}

fn small(t: &Theme, color: Hsla) -> Div {
    div()
        .flex_none()
        .text_size(t.typography.small)
        .text_color(color)
}
