//! The update line at the foot of the Start screen: off, the one-time prompt, on, or a newer version.

use gpui_kit::FocusHandle;
use gpui_kit::base::{TestSupportExt as _, TestTextExt as _};
use navo_compare_profile::appearance::UpdateCheck;
use sidewise_kit::prelude::*;
use sidewise_kit::primitives::{Button, ButtonVariant};
use sidewise_kit::tokens;
use sidewise_l10n::t;

use crate::updates::{UpdateView, view};
use crate::updates_check::{open_release_page, set_choice};

/// The Start screen's update line; it draws nothing while the update global is not installed.
pub(crate) struct UpdateLine {
    first: FocusHandle,
    second: FocusHandle,
}

/// A button of the line: its id, message id, look and click action.
struct Action {
    id: &'static str,
    label: &'static str,
    variant: ButtonVariant,
    run: fn(&mut App),
}

/// Turns the check on.
fn turn_on(cx: &mut App) {
    set_choice(UpdateCheck::On, cx);
}

/// Turns the check off.
fn turn_off(cx: &mut App) {
    set_choice(UpdateCheck::Off, cx);
}

/// The buttons the view draws, first to second; the Off line and the prompt differ in look only.
fn actions(view: &UpdateView) -> Vec<Action> {
    let on = |variant| Action {
        id: "update-turn-on",
        label: "update-turn-on",
        variant,
        run: turn_on,
    };
    let off = Action {
        id: "update-turn-off",
        label: "update-turn-off",
        variant: ButtonVariant::Ghost,
        run: turn_off,
    };
    match view {
        UpdateView::Off => vec![on(ButtonVariant::Ghost)],
        UpdateView::Prompt => vec![
            on(ButtonVariant::Primary),
            Action {
                id: "update-not-now",
                label: "update-not-now",
                variant: ButtonVariant::Ghost,
                run: turn_off,
            },
        ],
        UpdateView::On => vec![off],
        UpdateView::Found(_) => vec![
            Action {
                id: "update-release-page",
                label: "update-release-page",
                variant: ButtonVariant::Secondary,
                run: open_release_page,
            },
            off,
        ],
    }
}

impl UpdateLine {
    /// Makes the line with its two focus handles.
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        Self {
            first: cx.focus_handle(),
            second: cx.focus_handle(),
        }
    }

    /// The handles of the buttons drawn in the current view, first then second; `None` for a button not drawn.
    pub(crate) fn handles(&self, cx: &App) -> [Option<FocusHandle>; 2] {
        let count = view(cx).map_or(0, |v| actions(&v).len());
        [
            (count > 0).then(|| self.first.clone()),
            (count > 1).then(|| self.second.clone()),
        ]
    }

    /// One button of the line; the first tracks `first`, the second `second`.
    fn button(&self, index: usize, action: &Action) -> Button {
        let focus = if index == 0 {
            &self.first
        } else {
            &self.second
        };
        let run = action.run;
        Button::new(action.id, t!(action.label))
            .variant(action.variant)
            .track_focus(focus)
            .on_click(move |_, _, cx| run(cx))
    }
}

/// A line of small text in `color` that also carries its text for tests.
fn text_line(text: String, color: Hsla, cx: &App) -> Div {
    let t = tokens::current(cx);
    div()
        .text_size(t.typography.small)
        .text_color(color)
        .child(text.clone().test_text(text))
}

impl Render for UpdateLine {
    /// Draws the line for the current view, or nothing without the update global.
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(current) = view(cx) else {
            return div().into_any_element();
        };
        let t = tokens::current(cx);
        let buttons: Vec<Button> = actions(&current)
            .iter()
            .enumerate()
            .map(|(i, a)| self.button(i, a))
            .collect();
        let root = div().id("update-line").test_support().py(t.spacing.md);
        if current == UpdateView::Prompt {
            return root
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(t.spacing.sm)
                        .p(t.spacing.lg)
                        .bg(t.colors.surface)
                        .border_1()
                        .border_color(t.colors.line)
                        .rounded(t.radii.card)
                        .child(text_line(t!("update-prompt-title"), t.colors.text, cx))
                        .child(text_line(t!("update-prompt-body"), t.colors.muted, cx))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(t.spacing.sm)
                                .children(buttons),
                        ),
                )
                .into_any_element();
        }
        let message = match &current {
            UpdateView::Found(v) => t!("update-available", version = v.to_string()),
            UpdateView::On => t!("update-line-on"),
            _ => t!("update-line-off"),
        };
        root.child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(t.spacing.md)
                .child(text_line(message, t.colors.muted, cx))
                .children(buttons),
        )
        .into_any_element()
    }
}

#[cfg(test)]
#[path = "start_updates_tests.rs"]
mod tests;
