use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::component::input::InputEvent;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, AppContext, Context, ElementId, Entity, Focusable, InteractiveElement, IntoElement,
    KeyBinding, ParentElement, RenderOnce, Role, SharedString, StatefulInteractiveElement, Styled,
    Subscription, Window, div,
};

use crate::primitives::{TextField, TextFieldState};
use crate::tokens;

const CONTEXT: &str = "PathInput";

gpui_kit::actions!(path_input, [CancelPath]);

pub(super) fn bind_keys(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("escape", CancelPath, Some(CONTEXT))]);
}

type Submit = Rc<dyn Fn(&SharedString, &mut Window, &mut App)>;
type Cancel = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct PathInput {
    id: ElementId,
    value: SharedString,
    error: Option<SharedString>,
    on_submit: Option<Submit>,
    on_cancel: Option<Cancel>,
}

impl PathInput {
    pub fn new(id: impl Into<ElementId>, value: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            value: value.into(),
            error: None,
            on_submit: None,
            on_cancel: None,
        }
    }

    pub fn error(mut self, error: Option<SharedString>) -> Self {
        self.error = error;
        self
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

struct InputState {
    field: Entity<TextFieldState>,
    submit: Rc<RefCell<Option<Submit>>>,
    has_started: bool,
    _enter: Subscription,
}

impl InputState {
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

fn start(state: &Entity<InputState>, window: &mut Window, cx: &mut App) {
    let field = state.update(cx, |state, _| {
        let is_first = !state.has_started;
        state.has_started = true;
        is_first.then(|| state.field.clone())
    });
    let Some(field) = field else {
        return;
    };
    let length = field.read(cx).value().len();
    field.update(cx, |field, cx| field.set_selected_range(0..length, cx));
    let handle = field.read(cx).focus_handle(cx);
    window.defer(cx, move |window, cx| window.focus(&handle, cx));
}

impl RenderOnce for PathInput {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let value = self.value.clone();
        let state = window.use_keyed_state(self.id.clone(), cx, |window, cx| {
            InputState::new(value, window, cx)
        });
        start(&state, window, cx);
        let field = state.read(cx).field.clone();
        *state.read(cx).submit.borrow_mut() = self.on_submit.clone();
        let cancel = self.on_cancel;
        div()
            .id(self.id)
            .key_context(CONTEXT)
            .on_action(move |_: &CancelPath, window, cx| {
                if let Some(handler) = &cancel {
                    handler(window, cx);
                }
            })
            .flex()
            .flex_col()
            .flex_none()
            .m(t.spacing.sm)
            .gap(t.spacing.xs)
            .child(
                div().h(t.spacing.path_height).flex().items_center().child(
                    div()
                        .flex_1()
                        .child(TextField::new("path-field", &field, "Path")),
                ),
            )
            .when_some(self.error, |d, error| {
                d.child(
                    div()
                        .id("path-error")
                        .role(Role::Label)
                        .aria_label(error.clone())
                        .test_support()
                        .px(t.spacing.xs)
                        .text_size(t.typography.small)
                        .text_color(t.colors.mark)
                        .child(error),
                )
            })
    }
}
