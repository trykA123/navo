//! The confirm dialog's check rows: the optional choice and the optional switch, in Tab order.

use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, Div, FocusHandle, InteractiveElement, IntoElement, ParentElement, Role,
    StatefulInteractiveElement, Styled, Toggled, Window, div, px,
};

use super::keys::{self, Keys};
use super::{Choice, ConfirmDialog, Handler};
use crate::primitives::{IconName, icon};
use crate::tokens::Theme;

impl ConfirmDialog {
    /// Adds a second check row below the choice; `handler` runs on a click, Space or Enter.
    pub fn switch(
        mut self,
        choice: Choice,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.switch = Some(choice);
        self.on_switch = Some(Rc::new(handler));
        self
    }
}

/// One check row to draw: its element id, its label and state, and what a click runs.
struct CheckRow {
    id: &'static str,
    choice: Choice,
    on_click: Handler,
}

/// Stacks the choice and switch rows that are set; `None` when neither is, so no gap is left.
pub(super) fn check_rows(
    t: &Theme,
    (choice, switch): (Option<Choice>, Option<Choice>),
    keys: &Rc<Keys>,
    window: &Window,
) -> Option<Div> {
    let toggle = keys.clone();
    let flip = keys.clone();
    let on_toggle: Handler = Rc::new(move |window, cx| toggle.toggle(window, cx));
    let on_switch: Handler = Rc::new(move |window, cx| flip.switch(window, cx));
    let rows = [
        ("choice", choice, keys.choice.as_ref(), on_toggle),
        ("switch", switch, keys.switch.as_ref(), on_switch),
    ]
    .into_iter()
    .filter_map(|(id, choice, handle, on_click)| {
        let row = CheckRow {
            id,
            choice: choice?,
            on_click,
        };
        Some(check_row(t, row, handle?, window))
    })
    .collect::<Vec<_>>();
    (!rows.is_empty()).then(|| div().flex().flex_col().gap(t.spacing.xxs).children(rows))
}

/// Draws one focusable checkbox row; a click runs its handler, the focus ring follows `handle`.
fn check_row(t: &Theme, row: CheckRow, handle: &FocusHandle, window: &Window) -> impl IntoElement {
    let CheckRow {
        id,
        choice,
        on_click,
    } = row;
    let ring = if handle.is_focused(window) {
        t.colors.accent
    } else {
        gpui_kit::transparent_black()
    };
    let mark = div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(16.))
        .rounded(t.radii.sm)
        .border_1()
        .border_color(t.colors.line)
        .when(choice.on, |mark| {
            mark.child(icon(IconName::Check, px(12.), t.colors.accent_text))
        });
    div()
        .id(id)
        .role(Role::CheckBox)
        .aria_label(choice.label.clone())
        .aria_toggled(if choice.on {
            Toggled::True
        } else {
            Toggled::False
        })
        .test_support()
        .key_context(keys::BUTTON)
        .track_focus(handle)
        .flex()
        .items_center()
        .gap(t.spacing.sm)
        .px(t.spacing.xs)
        .py(t.spacing.xxs)
        .rounded(t.radii.md)
        .border_2()
        .border_color(ring)
        .cursor_pointer()
        .child(mark)
        .child(choice.label)
        .on_click(move |_, window, cx| on_click(window, cx))
}
