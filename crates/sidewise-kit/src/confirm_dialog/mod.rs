//! A modal confirm dialog: title, paths, summary, optional check rows, a typed word and buttons.
mod elide;
mod keys;
mod options;
mod rows;

#[cfg(test)]
mod details_tests;
#[cfg(test)]
mod key_tests;
#[cfg(test)]
mod tests;

use std::rc::Rc;

use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, Div, ElementId, Entity, FocusHandle, Focusable, FontWeight, InteractiveElement,
    IntoElement, ParentElement, RenderOnce, Role, SharedString, Stateful,
    StatefulInteractiveElement, Styled, Subscription, Window, div, px,
};

use crate::dialog_focus::{Stop, focus_ring};
use crate::primitives::{
    Button, ButtonVariant, IconName, TextField, TextFieldState, icon, text_field_state,
};
use crate::tokens::{self, Theme};
use keys::Keys;
pub use options::{OptionItem, Options};

type Handler = Rc<dyn Fn(&mut Window, &mut App)>;

const SHOWN_LINES: usize = 5;
const LINE_CHARS: usize = 54;

#[derive(Clone, Debug, PartialEq)]
pub struct ConfirmProps {
    pub title: SharedString,
    pub lines: Vec<SharedString>,
    pub more: u64,
    pub summary: SharedString,
    pub confirm_label: SharedString,
    pub danger: bool,
    pub typed: Option<SharedString>,
    pub mode: Option<SharedString>,
    pub choice: Option<Choice>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub label: SharedString,
    pub on: bool,
}

impl ConfirmProps {
    fn shown(&self) -> &[SharedString] {
        self.lines.get(..SHOWN_LINES).unwrap_or(&self.lines)
    }

    fn hidden(&self) -> u64 {
        let cut = self.lines.len().saturating_sub(SHOWN_LINES);
        self.more
            .saturating_add(u64::try_from(cut).unwrap_or(u64::MAX))
    }
}

#[derive(IntoElement)]
pub struct ConfirmDialog {
    id: ElementId,
    props: ConfirmProps,
    on_confirm: Option<Handler>,
    on_cancel: Option<Handler>,
    on_toggle: Option<Handler>,
    switch: Option<Choice>,
    on_switch: Option<Handler>,
    options: Option<Options>,
    on_pick: Option<options::PickHandler>,
}

impl ConfirmDialog {
    pub fn new(id: impl Into<ElementId>, props: ConfirmProps) -> Self {
        Self {
            id: id.into(),
            props,
            on_confirm: None,
            on_cancel: None,
            on_toggle: None,
            switch: None,
            on_switch: None,
            options: None,
            on_pick: None,
        }
    }

    pub fn on_confirm(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_confirm = Some(Rc::new(handler));
        self
    }

    pub fn on_cancel(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_cancel = Some(Rc::new(handler));
        self
    }

    pub fn on_toggle(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_toggle = Some(Rc::new(handler));
        self
    }
}

struct DialogState {
    root: FocusHandle,
    confirm: FocusHandle,
    cancel: FocusHandle,
    choice: FocusHandle,
    switch: FocusHandle,
    options: FocusHandle,
    field: Option<Entity<TextFieldState>>,
    has_started: bool,
    _field_changes: Option<Subscription>,
}

impl DialogState {
    fn new(
        typed: Option<&SharedString>,
        window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) -> Self {
        let field =
            typed.map(|word| text_field_state(format!("Type {word} to confirm"), window, cx));
        let changes = field
            .as_ref()
            .map(|field| cx.observe(field, |_, _, cx| cx.notify()));
        Self {
            root: cx.focus_handle(),
            confirm: cx.focus_handle(),
            cancel: cx.focus_handle(),
            choice: cx.focus_handle(),
            switch: cx.focus_handle(),
            options: cx.focus_handle(),
            field,
            has_started: false,
            _field_changes: changes,
        }
    }

    fn field_focus(&self, cx: &App) -> Option<FocusHandle> {
        self.field.as_ref().map(|field| field.focus_handle(cx))
    }

    fn first_stop(&self, props: &ConfirmProps, cx: &App) -> FocusHandle {
        match (self.field_focus(cx), props.danger) {
            (Some(field), _) => field,
            (None, true) => self.cancel.clone(),
            (None, false) => self.confirm.clone(),
        }
    }
}

fn is_typed_ok(props: &ConfirmProps, state: &DialogState, cx: &App) -> bool {
    match (&props.typed, &state.field) {
        (Some(word), Some(field)) => field.read(cx).value() == *word,
        (Some(_), None) => false,
        (None, _) => true,
    }
}

fn start_focus(
    state: &Entity<DialogState>,
    props: &ConfirmProps,
    window: &mut Window,
    cx: &mut App,
) {
    let first = state.update(cx, |state, cx| {
        let is_first = !state.has_started;
        state.has_started = true;
        is_first.then(|| state.first_stop(props, cx))
    });
    if let Some(handle) = first {
        window.defer(cx, move |window, cx| window.focus(&handle, cx));
    }
}

fn keys_for(dialog: ConfirmDialog, state: &DialogState, can_confirm: bool, cx: &App) -> Keys {
    let confirm = can_confirm.then(|| state.confirm.clone());
    let buttons: Vec<FocusHandle> = confirm.into_iter().chain([state.cancel.clone()]).collect();
    let choice = dialog.props.choice.is_some().then(|| state.choice.clone());
    let switch = dialog.switch.is_some().then(|| state.switch.clone());
    let options = dialog.options.is_some().then(|| state.options.clone());
    let stops = state
        .field_focus(cx)
        .into_iter()
        .chain(options.clone())
        .chain(choice.clone())
        .chain(switch.clone())
        .chain(buttons.clone())
        .collect();
    Keys {
        stops,
        buttons,
        confirm: state.confirm.clone(),
        can_confirm,
        cancel: state.cancel.clone(),
        choice,
        switch,
        options,
        on_confirm: dialog.on_confirm,
        on_cancel: dialog.on_cancel,
        on_toggle: dialog.on_toggle,
        on_switch: dialog.on_switch,
        on_pick: dialog.on_pick,
    }
}

fn label(id: impl Into<ElementId>, text: SharedString) -> ObservedElement<Stateful<Div>> {
    spoken_label(id, text.clone(), text)
}

fn spoken_label(
    id: impl Into<ElementId>,
    text: SharedString,
    spoken: SharedString,
) -> ObservedElement<Stateful<Div>> {
    div()
        .id(id)
        .role(Role::Label)
        .aria_label(spoken)
        .test_support()
        .min_w_0()
        .truncate()
        .child(text)
}

fn heading(t: &Theme, props: &ConfirmProps) -> Div {
    let color = if props.danger {
        t.colors.mark
    } else {
        t.colors.accent
    };
    div()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .child(icon(IconName::TriangleAlert, t.spacing.icon, color))
        .child(
            label("title", props.title.clone())
                .font_family(t.typography.brand_font.clone())
                .font_weight(FontWeight::SEMIBOLD),
        )
}

fn paths(t: &Theme, props: &ConfirmProps) -> Div {
    let hidden = props.hidden();
    let lines = props.shown().iter().enumerate().map(|(index, line)| {
        spoken_label(
            ("line", index),
            elide::middle(line, LINE_CHARS).into(),
            line.clone(),
        )
    });
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.xxs)
        .py(t.spacing.sm)
        .px(t.spacing.md)
        .rounded(t.radii.md)
        .bg(t.colors.rail)
        .font_family(t.typography.mono_font.clone())
        .text_size(t.typography.small)
        .children(lines)
        .when(hidden > 0, |list| {
            list.child(
                label("more", format!("and {hidden} more").into())
                    .font_family(t.typography.ui_font.clone())
                    .text_color(t.colors.muted),
            )
        })
}

/// The summary: its first line beside the mode label, cut to fit; text after a line break
/// wraps whole on its own line below.
fn summary_row(t: &Theme, props: &ConfirmProps) -> Div {
    let (first, detail) = match props.summary.split_once('\n') {
        Some((first, detail)) => (first.to_owned().into(), Some(detail.to_owned())),
        None => (props.summary.clone(), None),
    };
    let line = div()
        .flex()
        .items_center()
        .justify_between()
        .gap(t.spacing.md)
        .child(label("summary", first).text_color(t.colors.muted))
        .when_some(props.mode.clone(), |row, mode| {
            row.child(
                label("mode", mode)
                    .flex_none()
                    .px(t.spacing.sm)
                    .py(t.spacing.xxs)
                    .rounded(t.radii.sm)
                    .border_1()
                    .border_color(t.colors.line)
                    .text_size(t.typography.small)
                    .text_color(t.colors.muted),
            )
        });
    let detail = detail.map(|text| {
        div()
            .id("summary-detail")
            .role(Role::Label)
            .aria_label(text.clone())
            .test_support()
            .text_color(t.colors.muted)
            .child(text)
    });
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.xxs)
        .child(line)
        .children(detail)
}

fn buttons(t: &Theme, props: &ConfirmProps, keys: &Rc<Keys>, window: &Window) -> Div {
    let variant = if props.danger {
        ButtonVariant::Danger
    } else {
        ButtonVariant::Primary
    };
    let confirm = keys.clone();
    let cancel = keys.clone();
    let confirm_button = Button::new("confirm-button", props.confirm_label.clone())
        .variant(variant)
        .disabled(!keys.can_confirm)
        .on_click(move |_, window, cx| confirm.confirm(window, cx));
    let cancel_button = Button::new("cancel-button", "Cancel")
        .on_click(move |_, window, cx| cancel.cancel(window, cx));
    div()
        .flex()
        .justify_end()
        .gap(t.spacing.xs)
        .child(focus_ring(
            t,
            Stop {
                id: "confirm",
                context: keys::BUTTON,
                handle: &keys.confirm,
                button: confirm_button,
            },
            window,
        ))
        .child(focus_ring(
            t,
            Stop {
                id: "cancel",
                context: keys::BUTTON,
                handle: &keys.cancel,
                button: cancel_button,
            },
            window,
        ))
}

impl RenderOnce for ConfirmDialog {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let props = self.props.clone();
        let state = window.use_keyed_state(self.id.clone(), cx, |window, cx| {
            DialogState::new(props.typed.as_ref(), window, cx)
        });
        start_focus(&state, &props, window, cx);
        let id = self.id.clone();
        let current = state.read(cx);
        let can_confirm = is_typed_ok(&props, current, cx);
        let field = current.field.clone();
        let root = current.root.clone();
        let switch = self.switch.clone();
        let options = self.options.clone();
        let keys = Rc::new(keys_for(self, current, can_confirm, cx));
        keys::register(div().id(id).track_focus(&root), &keys)
            .role(Role::AlertDialog)
            .aria_label(props.title.clone())
            .test_support()
            .flex()
            .flex_col()
            .gap(t.spacing.xl)
            .w(px(460.))
            .p(t.spacing.xxl)
            .rounded(t.radii.lg)
            .border_1()
            .border_color(t.colors.line)
            .bg(t.colors.surface)
            .text_color(t.colors.text)
            .font_family(t.typography.ui_font.clone())
            .text_size(t.typography.body)
            .child(heading(&t, &props))
            .child(paths(&t, &props))
            .child(summary_row(&t, &props))
            .children(options.and_then(|options| options::options_row(&t, options, &keys, window)))
            .children(rows::check_rows(
                &t,
                (props.choice.clone(), switch),
                &keys,
                window,
            ))
            .when_some(field, |dialog, field| {
                dialog.child(TextField::new("typed", &field, "Confirmation word"))
            })
            .child(buttons(&t, &props, &keys, window))
    }
}
