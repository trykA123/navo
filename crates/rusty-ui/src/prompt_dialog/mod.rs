#[cfg(test)]
mod tests;

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::component::input::InputEvent;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, AppContext, Context, ElementId, Entity, Focusable, FontWeight, InteractiveElement,
    IntoElement, KeyBinding, ParentElement, RenderOnce, Role, SharedString,
    StatefulInteractiveElement, Styled, Subscription, Window, div, px,
};

use crate::primitives::{Button, ButtonVariant, TextField, TextFieldState};
use crate::tokens;

const CONTEXT: &str = "PromptDialog";

gpui_kit::actions!(prompt_dialog, [Dismiss]);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("escape", Dismiss, Some(CONTEXT))]);
}

type Submit = Rc<dyn Fn(&SharedString, &mut Window, &mut App)>;
type Cancel = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(Clone, Debug, PartialEq)]
pub struct PromptProps {
    pub title: SharedString,
    pub label: SharedString,
    pub value: SharedString,
    pub select: Range<usize>,
    pub confirm_label: SharedString,
    pub error: Option<SharedString>,
}

#[derive(IntoElement)]
pub struct PromptDialog {
    id: ElementId,
    props: PromptProps,
    on_submit: Option<Submit>,
    on_cancel: Option<Cancel>,
}

impl PromptDialog {
    pub fn new(id: impl Into<ElementId>, props: PromptProps) -> Self {
        Self {
            id: id.into(),
            props,
            on_submit: None,
            on_cancel: None,
        }
    }

    pub fn on_submit(
        mut self,
        handler: impl Fn(&SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_submit = Some(Rc::new(handler));
        self
    }

    pub fn on_cancel(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_cancel = Some(Rc::new(handler));
        self
    }
}

struct PromptState {
    field: Entity<TextFieldState>,
    submit: Rc<RefCell<Option<Submit>>>,
    has_started: bool,
    _enter: Subscription,
}

impl PromptState {
    fn new(value: SharedString, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let field = cx.new(|cx| TextFieldState::new(window, cx).default_value(value));
        let submit: Rc<RefCell<Option<Submit>>> = Rc::default();
        let on_enter = submit.clone();
        let enter = cx.subscribe_in(&field, window, move |state, _, event, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                let value = state.field.read(cx).value();
                fire(&on_enter, &value, window, cx);
            }
        });
        Self {
            field,
            submit,
            has_started: false,
            _enter: enter,
        }
    }
}

fn fire(submit: &RefCell<Option<Submit>>, value: &SharedString, window: &mut Window, cx: &mut App) {
    let handler = submit.borrow().clone();
    if let Some(handler) = handler {
        handler(value, window, cx);
    }
}

fn error_line(t: &tokens::Theme, error: SharedString) -> impl IntoElement {
    div()
        .id("error")
        .role(Role::Label)
        .aria_label(error.clone())
        .test_support()
        .text_color(t.colors.mark)
        .text_size(t.typography.small)
        .child(error)
}

fn start(state: &Entity<PromptState>, select: Range<usize>, window: &mut Window, cx: &mut App) {
    let field = state.update(cx, |state, _| {
        let is_first = !state.has_started;
        state.has_started = true;
        is_first.then(|| state.field.clone())
    });
    let Some(field) = field else {
        return;
    };
    field.update(cx, |field, cx| field.set_selected_range(select, cx));
    let handle = field.read(cx).focus_handle(cx);
    window.defer(cx, move |window, cx| window.focus(&handle, cx));
}

fn buttons(
    props: &PromptProps,
    submit: Option<Submit>,
    cancel: Option<Cancel>,
    field: Entity<TextFieldState>,
) -> impl IntoElement {
    let confirm = Button::new("confirm-button", props.confirm_label.clone())
        .variant(ButtonVariant::Primary)
        .on_click(move |_, window, cx| {
            let value = field.read(cx).value();
            if let Some(handler) = &submit {
                handler(&value, window, cx);
            }
        });
    let dismiss = Button::new("cancel-button", "Cancel").on_click(move |_, window, cx| {
        if let Some(handler) = &cancel {
            handler(window, cx);
        }
    });
    div()
        .flex()
        .justify_end()
        .gap(px(6.))
        .child(confirm)
        .child(dismiss)
}

impl RenderOnce for PromptDialog {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let value = self.props.value.clone();
        let state = window.use_keyed_state(self.id.clone(), cx, |window, cx| {
            PromptState::new(value, window, cx)
        });
        start(&state, self.props.select.clone(), window, cx);
        let field = state.read(cx).field.clone();
        *state.read(cx).submit.borrow_mut() = self.on_submit.clone();
        let cancel = self.on_cancel.clone();
        let props = self.props;
        div()
            .id(self.id)
            .key_context(CONTEXT)
            .on_action(move |_: &Dismiss, window, cx| {
                if let Some(handler) = &cancel {
                    handler(window, cx);
                }
            })
            .role(Role::Dialog)
            .aria_label(props.title.clone())
            .test_support()
            .flex()
            .flex_col()
            .gap(t.spacing.lg)
            .w(px(420.))
            .p(t.spacing.xxl)
            .rounded(t.radii.lg)
            .border_1()
            .border_color(t.colors.line)
            .bg(t.colors.surface)
            .text_color(t.colors.text)
            .font_family(t.typography.ui_font.clone())
            .text_size(t.typography.body)
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(props.title.clone()),
            )
            .child(TextField::new("field", &field, props.label.clone()))
            .when_some(props.error.clone(), |dialog, error| {
                dialog.child(error_line(&t, error))
            })
            .child(buttons(&props, self.on_submit, self.on_cancel, field))
    }
}
