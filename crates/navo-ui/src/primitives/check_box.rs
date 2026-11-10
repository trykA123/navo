//! Check box primitive: a labelled box with a check mark, used by the setup wizard.
//! Click, Enter and Space toggle it; gpui turns the keys into a click on the focused element.

use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Animation, AnimationExt, AnyElement, App, ClickEvent, Div, ElementId, InteractiveElement,
    IntoElement, ParentElement, RenderOnce, Role, SharedString, StatefulInteractiveElement, Styled,
    Toggled, Window, div, px,
};

use super::access_label::{access_label, plain_text};
use super::icon;
use crate::tokens::{self, Theme};

type ToggleHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

const BOX_SIZE: f32 = 16.;
const MARK_SIZE: f32 = 12.;
/// The box's opacity when a state change starts its fade.
const FADE_FROM: f32 = 0.3;

/// A check box whose label may hold one `&` access-key marker.
#[derive(IntoElement)]
pub struct CheckBox {
    id: ElementId,
    label: SharedString,
    checked: bool,
    disabled: bool,
    show_keys: bool,
    on_toggle: Option<ToggleHandler>,
}

impl CheckBox {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            checked: false,
            disabled: false,
            show_keys: false,
            on_toggle: None,
        }
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Underlines the access key of the label while Alt is held.
    pub fn show_keys(mut self, show_keys: bool) -> Self {
        self.show_keys = show_keys;
        self
    }

    /// Sets the handler for a click or an Enter/Space press; the owner flips the state.
    pub fn on_toggle(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_toggle = Some(Rc::new(handler));
        self
    }
}

/// Draws the 16 px box: accent fill and a check when on, a quiet outline otherwise.
fn mark_box(t: &Theme, checked: bool, disabled: bool) -> Div {
    let (edge, fill) = match (disabled, checked) {
        (true, _) => (t.colors.line, t.colors.rail),
        (false, true) => (t.colors.accent, t.colors.accent),
        (false, false) => (t.colors.faint, t.surfaces.field),
    };
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(BOX_SIZE))
        .rounded(t.radii.sm)
        .border_1()
        .border_color(edge)
        .bg(fill)
        .when(checked, |mark| {
            mark.child(icon(IconName::Check, px(MARK_SIZE), t.colors.background))
        })
}

/// Fades the box in over the theme's fade time when its state changes; still under reduced motion.
fn fade_on_change(mark: Div, checked: bool, t: &Theme, reduce_motion: bool) -> AnyElement {
    if reduce_motion {
        return mark.into_any_element();
    }
    // A new id per state restarts the fade on every change (fade only, IW21).
    let id = ("check-mark", usize::from(checked));
    mark.with_animation(id, Animation::new(t.motion.fade), |mark, delta| {
        mark.opacity(FADE_FROM + (1. - FADE_FROM) * delta)
    })
    .into_any_element()
}

impl RenderOnce for CheckBox {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx);
        let text = if self.disabled {
            t.colors.faint
        } else {
            t.colors.text
        };
        let mark = mark_box(t, self.checked, self.disabled);
        let mark = fade_on_change(mark, self.checked, t, cx.reduce_motion());
        let toggled = if self.checked {
            Toggled::True
        } else {
            Toggled::False
        };
        div()
            .id(self.id)
            .role(Role::CheckBox)
            .aria_label(plain_text(&self.label))
            .aria_toggled(toggled)
            .test_support()
            .flex()
            .items_center()
            .gap(t.spacing.sm)
            .px(t.spacing.xs)
            .py(t.spacing.xxs)
            .rounded(t.radii.md)
            .border_2()
            .border_color(gpui_kit::transparent_black())
            .text_color(text)
            .text_size(t.typography.body)
            .when(!self.disabled, |row| {
                row.focusable()
                    .tab_stop(true)
                    .cursor_pointer()
                    .focus_visible(|style| style.border_color(t.colors.accent))
            })
            .when_some(self.on_toggle.filter(|_| !self.disabled), |row, handler| {
                row.on_click(move |event, window, cx| handler(event, window, cx))
            })
            .child(mark)
            .child(access_label(&self.label, self.show_keys))
    }
}
