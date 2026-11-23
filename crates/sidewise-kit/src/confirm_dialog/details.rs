//! The confirm dialog's details list: a link under the options row opens a list in place of the
//! dialog body. Up, Down, Page Up, Page Down, Home and End scroll it; Esc or Back returns to the
//! dialog with focus on the link.

use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{
    Entity, FocusHandle, FontWeight, InteractiveElement, IntoElement, KeyDownEvent, ParentElement,
    Pixels, Role, ScrollHandle, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};

use super::keys::{self, Keys};
use super::{ConfirmDialog, DialogState, Handler};
use crate::dialog_focus::{Stop, focus_ring};
use crate::primitives::{Button, ButtonVariant};
use crate::tokens::Theme;

const LIST_HEIGHT: f32 = 280.;
/// The list's border and padding (2 px each), so the column names line up with the rows' cells.
const INSET: Pixels = px(4.);
/// One arrow press scrolls about one row.
const STEP: f32 = 44.;

/// One row of the list: a name and the two facts columns beside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetailRow {
    pub name: SharedString,
    pub left: SharedString,
    pub right: SharedString,
}

/// The list behind the link: the link's label, the list heading, the two column names, the rows,
/// a line for rows not shown, and the label of the button that goes back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Details {
    pub link: SharedString,
    pub heading: SharedString,
    pub columns: [SharedString; 2],
    pub rows: Vec<DetailRow>,
    pub more: Option<SharedString>,
    pub back: SharedString,
}

impl ConfirmDialog {
    /// Adds a link under the options row that opens `details` in place of the dialog body.
    pub fn details(mut self, details: Details) -> Self {
        self.details = Some(details);
        self
    }
}

/// A handler that opens (`open` true) or closes the list, then focuses the list or the link.
pub(super) fn show(state: &Entity<DialogState>, open: bool) -> Handler {
    let state = state.clone();
    Rc::new(move |window, cx| {
        let focus = state.update(cx, |state, cx| {
            state.details_open = open;
            cx.notify();
            if open {
                state.list.clone()
            } else {
                state.link.clone()
            }
        });
        window.focus(&focus, cx);
    })
}

/// The link that opens the list; `None` when the dialog has no details.
pub(super) fn link(
    t: &Theme,
    details: &Details,
    keys: &Rc<Keys>,
    window: &Window,
) -> Option<impl IntoElement + use<>> {
    let (handle, open) = (keys.details.link.clone()?, keys.details.on_open.clone()?);
    let button = Button::new("details-link-button", details.link.clone())
        .variant(ButtonVariant::Ghost)
        .on_click(move |_, window, cx| open(window, cx));
    let stop = Stop {
        id: "details-link",
        context: keys::BUTTON,
        handle: &handle,
        button,
    };
    Some(div().flex().child(focus_ring(t, stop, window)))
}

/// The new scroll offset after `key`, or `None` for a key the list does not use. `y` is the
/// current offset (zero or negative), `max` the furthest the list can scroll, both in pixels.
pub(super) fn scrolled(key: &str, y: f32, max: f32) -> Option<f32> {
    let page = LIST_HEIGHT - STEP;
    let moved = match key {
        "up" => y + STEP,
        "down" => y - STEP,
        "pageup" => y + page,
        "pagedown" => y - page,
        "home" => 0.,
        "end" => -max,
        _ => return None,
    };
    Some(moved.clamp(-max, 0.))
}

/// Scrolls the list for `event` and redraws; other keys pass on.
fn scroll(scroll: &ScrollHandle, event: &KeyDownEvent, window: &mut Window) -> bool {
    let mut offset = scroll.offset();
    let max = f32::from(scroll.max_offset().y);
    let Some(y) = scrolled(event.keystroke.key.as_str(), f32::from(offset.y), max) else {
        return false;
    };
    offset.y = px(y);
    scroll.set_offset(offset);
    window.refresh();
    true
}

/// One row: the name on top, the two facts below in the columns of the header.
fn row(t: &Theme, index: usize, detail: &DetailRow) -> impl IntoElement + use<> {
    let spoken = format!("{}: {}; {}", detail.name, detail.left, detail.right);
    div()
        .id(("detail", index))
        .role(Role::ListItem)
        .aria_label(SharedString::from(spoken))
        .test_support()
        .flex()
        .flex_col()
        .gap(t.spacing.xxs)
        .py(t.spacing.sm)
        .border_b_1()
        .border_color(t.colors.line)
        .child(
            div()
                .truncate()
                .font_weight(FontWeight::SEMIBOLD)
                .child(detail.name.clone()),
        )
        .child(columns(t, [detail.left.clone(), detail.right.clone()]))
}

/// Two equal columns of muted small text.
fn columns(t: &Theme, texts: [SharedString; 2]) -> impl IntoElement + use<> {
    let cell = |text: SharedString| div().flex_1().min_w_0().truncate().child(text);
    let [left, right] = texts;
    div()
        .flex()
        .gap(t.spacing.md)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(cell(left))
        .child(cell(right))
}

/// The scrolling rows: focusable through `list`, scrolled by keys through `scroll`.
fn list_body(
    t: &Theme,
    details: &Details,
    (list, scroll_handle): (&FocusHandle, &ScrollHandle),
    window: &Window,
) -> impl IntoElement + use<> {
    let scroller = scroll_handle.clone();
    let rows = details.rows.iter().enumerate().map(|(i, d)| row(t, i, d));
    div()
        .id("details-list")
        .role(Role::List)
        .aria_label(details.heading.clone())
        .test_support()
        .track_focus(list)
        .track_scroll(scroll_handle)
        .overflow_y_scroll()
        .max_h(px(LIST_HEIGHT))
        .p(INSET - px(2.))
        .rounded(t.radii.md)
        .border_2()
        .border_color(super::options::ring(list, t, window))
        .on_key_down(move |event, window, cx| {
            if scroll(&scroller, event, window) {
                cx.stop_propagation();
            }
        })
        .children(rows)
}

/// The open list: heading, column names, the scrolling rows, the "more" line and Back.
pub(super) fn panel(
    t: &Theme,
    details: &Details,
    keys: &Rc<Keys>,
    window: &Window,
) -> Option<impl IntoElement + use<>> {
    let (list, back, close) = (
        keys.details.list.clone()?,
        keys.details.back.clone()?,
        keys.details.on_close.clone()?,
    );
    let body = list_body(t, details, (&list, &keys.details.scroll), window);
    let button = Button::new("details-back-button", details.back.clone())
        .on_click(move |_, window, cx| close(window, cx));
    let stop = Stop {
        id: "details-back",
        context: keys::BUTTON,
        handle: &back,
        button,
    };
    let more = details.more.clone().map(|more| {
        div()
            .text_size(t.typography.small)
            .text_color(t.colors.muted)
            .child(more)
    });
    let heading = div()
        .font_weight(FontWeight::SEMIBOLD)
        .child(details.heading.clone());
    Some(
        div()
            .flex()
            .flex_col()
            .gap(t.spacing.md)
            .child(heading)
            .child(div().px(INSET).child(columns(t, details.columns.clone())))
            .child(body)
            .children(more)
            .child(
                div()
                    .flex()
                    .justify_end()
                    .child(focus_ring(t, stop, window)),
            ),
    )
}
