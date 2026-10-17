use gpui_kit::Role;
use gpui_kit::base::TestSupportExt;

use super::page::{Pending, ShortcutsPage, Stage};
use crate::commands::shown;
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant, IconName, icon};
use crate::tokens::Theme;

pub(super) fn banner(t: &Theme, problem: &SharedString, cx: &Context<ShortcutsPage>) -> AnyElement {
    let open = cx.listener(|page, _: &ClickEvent, _, cx| page.open_file(cx));
    let text = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .gap(t.spacing.xxs)
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .child("Changes wait until keymap.toml is fixed"),
        )
        .child(
            div()
                .font_family(t.typography.mono_font.clone())
                .text_size(t.typography.small)
                .text_color(t.colors.muted)
                .child(problem.clone()),
        )
        .child(
            div().flex().pt(t.spacing.sm).child(
                Button::new("shortcuts-open-file", "Open keymap.toml")
                    .icon(IconName::FilePen)
                    .on_click(open),
            ),
        );
    div()
        .id("shortcuts-problem")
        .role(Role::Alert)
        .aria_label(problem.clone())
        .test_support()
        .flex()
        .flex_none()
        .items_start()
        .gap(t.spacing.lg)
        .mx(t.spacing.xl)
        .mt(t.spacing.md)
        .p(t.spacing.lg)
        .rounded(t.radii.md)
        .border_1()
        .border_color(t.colors.warn)
        .bg(t.surfaces.field)
        .child(icon(IconName::TriangleAlert, t.spacing.icon, t.colors.warn))
        .child(text)
        .into_any_element()
}

fn pending_text(pending: &Pending) -> String {
    let label = pending.spec.label;
    match pending.keys.as_deref() {
        None => format!("The reset of {label}"),
        Some([]) => format!("Removing the key of {label}"),
        Some(keys) => {
            let keys: Vec<String> = keys
                .iter()
                .map(|key| shown(key).unwrap_or_else(|| key.clone()))
                .collect();
            format!("{} for {label}", keys.join(", "))
        }
    }
}

fn bar(
    t: &Theme,
    id: &'static str,
    message: String,
) -> impl ParentElement + Styled + IntoElement + use<> {
    div()
        .id(id)
        .role(Role::Alert)
        .aria_label(message.clone())
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.lg)
        .px(t.spacing.xl)
        .py(t.spacing.lg)
        .border_t_1()
        .border_color(t.surfaces.card_edge)
        .text_size(t.typography.small)
        .child(div().flex_1().min_w_0().child(message))
}

fn choice(
    (id, label, variant): (&'static str, &'static str, ButtonVariant),
    cx: &Context<ShortcutsPage>,
    act: fn(&mut ShortcutsPage, &mut Context<ShortcutsPage>),
) -> Button {
    Button::new(id, label)
        .variant(variant)
        .on_click(cx.listener(move |page, _: &ClickEvent, _, cx| act(page, cx)))
}

pub(super) fn footer(t: &Theme, stage: &Stage, cx: &Context<ShortcutsPage>) -> Option<AnyElement> {
    let footer = match stage {
        Stage::Recording(spec) => bar(
            t,
            "shortcuts-recording",
            format!(
                "Press the new key for {}. Esc cancels, Backspace removes the key.",
                spec.label
            ),
        )
        .text_color(t.colors.muted),
        Stage::Conflict(_, conflict) => bar(t, "shortcuts-prompt", conflict.message())
            .child(choice(
                ("shortcuts-replace", "Replace", ButtonVariant::Primary),
                cx,
                ShortcutsPage::replace,
            ))
            .child(choice(
                ("shortcuts-cancel", "Cancel", ButtonVariant::Ghost),
                cx,
                ShortcutsPage::cancel,
            )),
        Stage::Failed(pending, error) => {
            let message = format!("{} was not saved: {error}", pending_text(pending));
            bar(t, "shortcuts-error", message)
                .text_color(t.colors.mark)
                .child(choice(
                    ("shortcuts-retry", "Try again", ButtonVariant::Primary),
                    cx,
                    ShortcutsPage::replace,
                ))
                .child(choice(
                    ("shortcuts-discard", "Discard", ButtonVariant::Ghost),
                    cx,
                    ShortcutsPage::cancel,
                ))
        }
        Stage::Browse | Stage::Saving(_) => return None,
    };
    Some(footer.into_any_element())
}
