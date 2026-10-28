use gpui_kit::base::TestSupportExt;
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Context, Div, FontWeight, InteractiveElement, IntoElement, KeyDownEvent, Modifiers,
    ParentElement, Role, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};
use rusty_l10n::tr;

use super::super::Mode;
use super::{Argument, CommandCenter, CommandCenterEvent, INPUT_ROW};
use crate::primitives::{IconName, icon};
use crate::tokens::Theme;

const FIELD: f32 = 32.;
const CHIP: f32 = 22.;

pub(super) fn chip(t: &Theme, argument: &Argument) -> impl IntoElement {
    let theme = t.selected_content(true);
    let t = &theme;
    div()
        .id("command-center-chip")
        .role(Role::Label)
        .aria_label(argument.label.clone())
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.xs)
        .h(px(CHIP))
        .px(t.spacing.md)
        .rounded(t.radii.full)
        .bg(t.surfaces.current)
        .text_color(t.colors.accent_text)
        .text_size(t.typography.tiny)
        .font_weight(FontWeight::SEMIBOLD)
        .child(icon(
            IconName::SquareChevronRight,
            t.typography.small,
            t.colors.accent_text,
        ))
        .child(argument.label.clone())
}

impl CommandCenter {
    pub(super) fn remove_scope(
        &mut self,
        event: &KeyDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.props.mode == Mode::Content
            && event.keystroke.modifiers
                == (Modifiers {
                    alt: true,
                    ..Modifiers::default()
                })
            && matches!(event.keystroke.key.as_str(), "c" | "r")
        {
            cx.emit(CommandCenterEvent::ToggleContentFlag(
                event.keystroke.key == "r",
            ));
            cx.stop_propagation();
            return;
        }
        if event.keystroke.key == "backspace"
            && event.keystroke.modifiers == Modifiers::default()
            && self.props.mode == Mode::Content
            && self.props.scope.is_some()
            && self.query(cx).is_empty()
        {
            cx.emit(CommandCenterEvent::ScopeRemoved);
            cx.stop_propagation();
        }
    }

    pub(super) fn input_row(&self, t: &Theme) -> Div {
        let input = Input::new(&self.field)
            .id("command-center-input")
            .aria_label(tr("command-center"))
            .appearance(false)
            .h(px(FIELD))
            .text_size(t.typography.body);
        let field = div()
            .flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .gap(t.spacing.md)
            .h(px(FIELD))
            .pl(t.spacing.xl)
            .pr(t.spacing.xs)
            .rounded(t.radii.full)
            .border_1()
            .border_color(t.colors.accent)
            .bg(t.surfaces.field)
            .child(icon(IconName::Search, t.spacing.icon, t.colors.muted))
            .when_some(self.props.argument.as_ref(), |field, argument| {
                field.child(chip(t, argument))
            })
            .when_some(
                self.props
                    .scope
                    .as_ref()
                    .filter(|_| self.props.mode == Mode::Content),
                |field, scope| field.child(scope_chip(t, scope)),
            )
            .child(div().flex_1().min_w_0().child(input))
            .children(self.escape_key());
        div()
            .flex()
            .flex_none()
            .items_center()
            .h(px(INPUT_ROW))
            .p(t.spacing.xs)
            .child(field)
    }
}

fn scope_chip(t: &Theme, scope: &rusty_core::Location) -> impl IntoElement {
    let theme = t.selected_content(true);
    let t = &theme;
    let label: SharedString = rusty_l10n::t!(
        "command-center-scope",
        folder = super::super::scope_name(scope)
    )
    .into();
    div()
        .id("command-center-scope")
        .role(Role::Label)
        .aria_label(label.clone())
        .test_support()
        .flex()
        .flex_shrink(1.)
        .min_w_0()
        .max_w(gpui_kit::relative(0.5))
        .items_center()
        .h(px(CHIP))
        .px(t.spacing.md)
        .rounded(t.radii.full)
        .bg(t.surfaces.current)
        .text_color(t.colors.accent_text)
        .text_size(t.typography.tiny)
        .line_height(t.typography.tiny)
        .child(div().min_w_0().truncate().child(label))
}
