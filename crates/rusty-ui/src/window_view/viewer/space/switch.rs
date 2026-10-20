use std::rc::Rc;

use gpui_kit::Role;
use gpui_kit::base::TestSupportExt;

use crate::prelude::*;
use crate::primitives::Tooltip;
use crate::tokens::Theme;

pub(in crate::window_view::viewer) struct Choice {
    pub label: String,
    pub is_on: bool,
    pub blocked: Option<String>,
}

type OnPick = Rc<dyn Fn(usize, &mut Window, &mut App)>;

pub(in crate::window_view::viewer) fn switch(
    t: &Theme,
    id: &'static str,
    choices: Vec<Choice>,
    on_pick: OnPick,
) -> Div {
    let buttons = choices.into_iter().enumerate().map(|(at, choice)| {
        let on_pick = on_pick.clone();
        let blocked = choice.blocked.clone();
        div()
            .id((id, at))
            .role(Role::Button)
            .aria_label(choice.label.clone())
            .aria_selected(choice.is_on)
            .test_support()
            .flex()
            .items_center()
            .h(t.spacing.tab_height - t.spacing.md)
            .px(t.spacing.md)
            .rounded(t.radii.sm)
            .whitespace_nowrap()
            .text_size(t.typography.small)
            .when(choice.is_on, |button| {
                button
                    .bg(t.colors.selection)
                    .text_color(t.colors.text)
                    .font_weight(FontWeight::SEMIBOLD)
            })
            .when(!choice.is_on && blocked.is_none(), |button| {
                button
                    .text_color(t.colors.muted)
                    .cursor_pointer()
                    .hover(|style| style.bg(t.colors.hover))
                    .on_click(move |_, window, cx| on_pick(at, window, cx))
            })
            .when_some(blocked, |button, why| {
                button
                    .text_color(t.colors.faint)
                    .tooltip(move |window, cx| Tooltip::new(why.clone()).build(window, cx))
            })
            .child(choice.label)
    });
    div()
        .flex()
        .flex_none()
        .gap(t.spacing.xxs)
        .p(t.spacing.xxs)
        .rounded(t.radii.md)
        .border_1()
        .border_color(t.colors.line)
        .children(buttons)
}
