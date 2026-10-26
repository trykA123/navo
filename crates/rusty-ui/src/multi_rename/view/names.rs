use gpui_kit::Action;
use gpui_kit::component::input::Textarea;
pub use gpui_kit::component::input::TextareaState;
use rusty_l10n::tr;

use crate::multi_rename::keys as mr;
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant, IconButton, IconName};
use crate::tokens;

fn command<A: Action>(id: &'static str, action: A) -> Button {
    Button::new(id, tr(id))
        .variant(ButtonVariant::Ghost)
        .on_click(move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx))
}

fn name_buttons() -> [Button; 4] {
    [
        command("rename-edit-names", mr::EditNames),
        command("rename-load-names", mr::LoadNames),
        command("rename-paste-names", mr::PasteNames),
        command("rename-copy-names", mr::CopyNames),
    ]
}

pub fn name_tools(per_folder: bool, cx: &App) -> Div {
    let t = tokens::current(cx);
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(t.spacing.xs)
        .px(t.spacing.xl)
        .pb(t.spacing.xs)
        .children(name_buttons())
        .child(
            IconButton::new(
                "rename-counter-per-folder",
                IconName::ListOrdered,
                tr("rename-counter-per-folder"),
            )
            .selected(per_folder)
            .on_click(|_, window, cx| window.dispatch_action(Box::new(mr::CounterPerFolder), cx)),
        )
        .child(
            div()
                .text_size(t.typography.small)
                .child(tr("rename-counter-per-folder")),
        )
}

pub fn name_editor(input: &Entity<TextareaState>, error: Option<String>, cx: &App) -> Div {
    let t = tokens::current(cx);
    div()
        .flex()
        .flex_col()
        .flex_none()
        .gap(t.spacing.xs)
        .px(t.spacing.md)
        .py(t.spacing.xs)
        .child(
            div()
                .text_size(t.typography.small)
                .child(tr("rename-names-label")),
        )
        .child(
            Textarea::new(input)
                .h(t.spacing.row_height * 4.)
                .aria_label(tr("rename-names-label")),
        )
        .children(error.map(|error| {
            div()
                .text_size(t.typography.small)
                .text_color(t.colors.mark)
                .child(error)
        }))
}
