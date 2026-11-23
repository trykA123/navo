//! The confirm dialog's options row: one pick among a few labelled choices (a segmented control).
//! Tab reaches it, Left and Right move the pick, Enter confirms (Space does not), and each choice
//! is a click target.

use std::rc::Rc;

use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, Div, FocusHandle, FontWeight, InteractiveElement, IntoElement, KeyDownEvent,
    ParentElement, Role, SharedString, Stateful, StatefulInteractiveElement, Styled, Window, div,
    px,
};

use super::ConfirmDialog;
use super::keys::{self, Keys};
use crate::dialog_focus::{NextButton, PreviousButton};
use crate::tokens::Theme;

/// What a pick runs: the index of the chosen option.
pub(super) type PickHandler = Rc<dyn Fn(usize, &mut Window, &mut App)>;

const SEGMENT_HEIGHT: f32 = 24.;

/// One choice in the row: its element id (for tests and clicks) and its label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OptionItem {
    pub id: SharedString,
    pub label: SharedString,
}

/// The options row: the spoken name of the group, its choices in order and the picked index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    pub label: SharedString,
    pub items: Vec<OptionItem>,
    pub picked: usize,
}

impl Options {
    /// The index one step from the pick, kept inside the row; `forward` moves right.
    fn step(&self, forward: bool) -> usize {
        let last = self.items.len().saturating_sub(1);
        if forward {
            (self.picked + 1).min(last)
        } else {
            self.picked.saturating_sub(1)
        }
    }
}

impl ConfirmDialog {
    /// Adds the options row below the summary; `handler` gets the index of a newly picked choice.
    pub fn options(
        mut self,
        options: Options,
        handler: impl Fn(usize, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.options = Some(options);
        self.on_pick = Some(Rc::new(handler));
        self
    }
}

/// The row's pick handler together with the options it was drawn from.
#[derive(Clone)]
struct Picker {
    handler: PickHandler,
    options: Rc<Options>,
}

impl Picker {
    /// Runs the handler for `index` when it differs from the current pick.
    fn pick(&self, index: usize, window: &mut Window, cx: &mut App) {
        if index != self.options.picked {
            (self.handler)(index, window, cx);
        }
    }

    /// Picks the choice one step from the current one; `forward` moves right.
    fn step(&self, forward: bool, window: &mut Window, cx: &mut App) {
        self.pick(self.options.step(forward), window, cx);
    }
}

/// Draws one choice; the picked one is raised like a settings segment.
fn segment(t: &Theme, item: &OptionItem, is_on: bool) -> ObservedElement<Stateful<Div>> {
    div()
        .id(item.id.clone())
        .role(Role::RadioButton)
        .aria_label(item.label.clone())
        .aria_selected(is_on)
        .test_support()
        .flex()
        .items_center()
        .h(px(SEGMENT_HEIGHT))
        .px(t.spacing.lg)
        .rounded(t.radii.sm)
        .whitespace_nowrap()
        .cursor_pointer()
        .text_color(if is_on { t.colors.text } else { t.colors.muted })
        .when(is_on, |segment| {
            segment
                .bg(t.surfaces.sheet)
                .border_1()
                .border_color(t.surfaces.card_edge)
                .font_weight(FontWeight::SEMIBOLD)
        })
        .child(item.label.clone())
}

/// Enter on the row confirms the dialog; Space does nothing (it would read as a pick).
fn enter_only(keys: &Keys, event: &KeyDownEvent, window: &mut Window, cx: &mut App) {
    match event.keystroke.key.as_str() {
        "enter" if keys.can_confirm => {
            keys.confirm(window, cx);
            cx.stop_propagation();
        }
        "enter" | "space" => cx.stop_propagation(),
        _ => {}
    }
}

/// The sunken rail the segments sit on.
fn rail(t: &Theme) -> Div {
    div()
        .flex()
        .gap(t.spacing.xxs)
        .p(t.spacing.xxs)
        .rounded(t.radii.md)
        .border_1()
        .border_color(t.colors.line)
        .bg(t.colors.rail)
        .text_size(t.typography.small)
}

/// The focusable row: a ring that follows `handle`, Left/Right picks and a click per choice.
pub(super) fn options_row(
    t: &Theme,
    options: Options,
    keys: &Rc<Keys>,
    window: &Window,
) -> Option<impl IntoElement + use<>> {
    let (handle, handler) = (keys.options.clone()?, keys.on_pick.clone()?);
    let picker = Picker {
        handler,
        options: Rc::new(options),
    };
    let options = picker.options.clone();
    let (next, previous, enter) = (picker.clone(), picker.clone(), keys.clone());
    let segments = options.items.iter().enumerate().map(|(index, item)| {
        let picker = picker.clone();
        segment(t, item, index == options.picked)
            .on_click(move |_, window, cx| picker.pick(index, window, cx))
    });
    let group = rail(t).children(segments);
    Some(
        div()
            .id("options")
            .role(Role::RadioGroup)
            .aria_label(options.label.clone())
            .test_support()
            .key_context(keys::BUTTON)
            .track_focus(&handle)
            .on_action(move |_: &NextButton, window, cx| {
                next.step(true, window, cx);
                cx.stop_propagation();
            })
            .on_key_down(move |event, window, cx| enter_only(&enter, event, window, cx))
            .on_action(move |_: &PreviousButton, window, cx| {
                previous.step(false, window, cx);
                cx.stop_propagation();
            })
            .self_start()
            .p(px(2.))
            .rounded(t.radii.lg)
            .border_2()
            .border_color(ring(&handle, t, window))
            .child(group),
    )
}

/// The focus ring: the accent while the row has focus, clear otherwise (as the buttons do).
pub(super) fn ring(handle: &FocusHandle, t: &Theme, window: &Window) -> gpui_kit::Hsla {
    if handle.is_focused(window) {
        t.colors.accent
    } else {
        gpui_kit::transparent_black()
    }
}
