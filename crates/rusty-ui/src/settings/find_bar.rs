use crate::prelude::*;
use crate::primitives::{IconName, TextField, TextFieldState, icon};
use crate::tokens::Theme;

pub fn find_field(t: &Theme, input: &Entity<TextFieldState>) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.md)
        .px(t.spacing.xl)
        .pt(t.spacing.lg)
        .pb(t.spacing.md)
        .child(icon(IconName::Search, t.spacing.icon, t.colors.muted))
        .child(
            div()
                .flex_1()
                .child(TextField::new("settings-find", input, "Find a setting")),
        )
}
