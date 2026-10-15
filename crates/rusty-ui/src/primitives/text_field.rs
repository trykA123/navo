use gpui_kit::component::Sizable;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::{
    App, AppContext, ElementId, Entity, IntoElement, RenderOnce, SharedString, Styled, Window,
};

use crate::tokens;

pub type TextFieldState = InputState;

pub fn text_field_state(
    placeholder: impl Into<SharedString>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<TextFieldState> {
    let placeholder = placeholder.into();
    cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
}

#[derive(IntoElement)]
pub struct TextField {
    id: ElementId,
    state: Entity<TextFieldState>,
    label: SharedString,
}

impl TextField {
    pub fn new(
        id: impl Into<ElementId>,
        state: &Entity<TextFieldState>,
        label: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            state: state.clone(),
            label: label.into(),
        }
    }
}

impl RenderOnce for TextField {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx);
        Input::new(&self.state)
            .id(self.id)
            .aria_label(self.label)
            .small()
            .font_family(t.typography.mono_font.clone())
            .text_size(t.typography.small)
    }
}
