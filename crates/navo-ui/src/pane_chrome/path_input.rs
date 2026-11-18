use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::component::input::InputEvent;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, AppContext, Context, ElementId, Entity, FocusOutEvent, Focusable, InteractiveElement,
    IntoElement, ParentElement, RenderOnce, Role, SharedString, StatefulInteractiveElement, Styled,
    Subscription, Window, div,
};

use crate::primitives::{TextField, TextFieldState};
use crate::tokens;

const CONTEXT: &str = "PathInput";

gpui_kit::actions!(path_input, [CancelPath]);

pub(super) fn bind_keys(cx: &mut App) {
    crate::commands::bind_namespace(cx, "path_input");
}

/// Tells whether focus is still in the field or its context menu.
pub type InFocus = Rc<dyn Fn(&Window, &App) -> bool>;

type Submit = Rc<dyn Fn(&SharedString, InFocus, &mut Window, &mut App)>;
type Cancel = Rc<dyn Fn(&mut Window, &mut App)>;
type Focused = Rc<dyn Fn(InFocus, &mut App)>;

#[derive(IntoElement)]
pub struct PathInput {
    id: ElementId,
    value: SharedString,
    error: Option<SharedString>,
    on_submit: Option<Submit>,
    on_cancel: Option<Cancel>,
    on_blur: Option<Cancel>,
    on_focused: Option<Focused>,
}

impl PathInput {
    pub fn new(id: impl Into<ElementId>, value: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            value: value.into(),
            error: None,
            on_submit: None,
            on_cancel: None,
            on_blur: None,
            on_focused: None,
        }
    }

    pub fn error(mut self, error: Option<SharedString>) -> Self {
        self.error = error;
        self
    }

    /// Sets the Enter handler; it gets the text and a probe of the field's focus.
    pub fn on_submit(
        mut self,
        handler: impl Fn(&SharedString, InFocus, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_submit = Some(Rc::new(handler));
        self
    }

    pub fn on_cancel(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_cancel = Some(Rc::new(handler));
        self
    }

    /// Sets the handler that runs when focus leaves both the field and its context menu.
    pub fn on_blur(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_blur = Some(Rc::new(handler));
        self
    }

    /// Sets the handler that gets the field's focus probe once the field first takes focus.
    pub fn on_focused(mut self, handler: impl Fn(InFocus, &mut App) + 'static) -> Self {
        self.on_focused = Some(Rc::new(handler));
        self
    }
}

struct InputState {
    field: Entity<TextFieldState>,
    submit: Rc<RefCell<Option<Submit>>>,
    blur: Rc<RefCell<Option<Cancel>>>,
    has_started: bool,
    _enter: Subscription,
    _popup: Option<Subscription>,
}

impl InputState {
    fn new(value: SharedString, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let field = cx.new(|cx| TextFieldState::new(window, cx).default_value(value));
        let submit: Rc<RefCell<Option<Submit>>> = Rc::default();
        let on_enter = submit.clone();
        let enter = cx.subscribe_in(
            &field,
            window,
            move |state, _, event, window, cx| match event {
                InputEvent::PressEnter { .. } => {
                    let value = state.field.read(cx).value();
                    fire(&on_enter, (&value, in_focus(&state.field)), window, cx);
                }
                InputEvent::Blur => state.focus_left(window, cx),
                _ => {}
            },
        );
        Self {
            field,
            submit,
            blur: Rc::default(),
            has_started: false,
            _enter: enter,
            _popup: None,
        }
    }

    /// Rechecks the focus once it leaves the field's context menu.
    fn menu_left(&mut self, _: FocusOutEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_left(window, cx);
    }

    /// Runs the blur handler once focus is outside the field and its context menu.
    /// While the menu holds focus, it waits for focus to leave the menu instead.
    fn focus_left(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let field = self.field.read(cx);
        let (is_inside, own) = (
            field.has_selection_focus(window, cx),
            field.focus_handle(cx),
        );
        if is_inside {
            self._popup = window
                .focused(cx)
                .filter(|focus| *focus != own)
                .map(|menu| cx.on_focus_out(&menu, window, Self::menu_left));
            return;
        }
        self._popup = None;
        let handler = self.blur.borrow().clone();
        if let Some(handler) = handler {
            handler(window, cx);
        }
    }
}

/// Builds a probe that tells whether `field` or its context menu has focus; false once it is gone.
fn in_focus(field: &Entity<TextFieldState>) -> InFocus {
    let field = field.downgrade();
    Rc::new(move |window, cx| {
        field
            .upgrade()
            .is_some_and(|field| field.read(cx).has_selection_focus(window, cx))
    })
}

fn fire(
    submit: &RefCell<Option<Submit>>,
    (value, probe): (&SharedString, InFocus),
    window: &mut Window,
    cx: &mut App,
) {
    let handler = submit.borrow().clone();
    if let Some(handler) = handler {
        handler(value, probe, window, cx);
    }
}

/// Selects the text and focuses the field on its first render, then hands out its focus probe.
fn start(state: &Entity<InputState>, focused: Option<Focused>, window: &mut Window, cx: &mut App) {
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
    let probe = in_focus(&field);
    window.defer(cx, move |window, cx| {
        window.focus(&handle, cx);
        if let Some(focused) = focused {
            focused(probe, cx);
        }
    });
}

impl RenderOnce for PathInput {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let value = self.value.clone();
        let state = window.use_keyed_state(self.id.clone(), cx, |window, cx| {
            InputState::new(value, window, cx)
        });
        start(&state, self.on_focused, window, cx);
        let field = state.read(cx).field.clone();
        (*state.read(cx).submit.borrow_mut()).clone_from(&self.on_submit);
        (*state.read(cx).blur.borrow_mut()).clone_from(&self.on_blur);
        let cancel = self.on_cancel;
        div()
            .id(self.id)
            .test_support()
            .key_context(CONTEXT)
            .on_action(move |_: &CancelPath, window, cx| {
                if let Some(handler) = &cancel {
                    handler(window, cx);
                }
            })
            .flex()
            .flex_col()
            .flex_none()
            .mx(t.spacing.md)
            .mb(t.spacing.sm)
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
