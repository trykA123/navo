use gpui_kit::base::TestSupportExt;
use gpui_kit::{Role, SharedString};
use navo_l10n::t;

use crate::multi_rename::Summary;
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant};
use crate::tokens::Theme;

pub(super) fn summary_text(summary: &Summary) -> String {
    let base = t!(
        "rename-summary",
        changes = summary.rename,
        unchanged = summary.unchanged,
        problems = summary.problems
    );
    if summary.excluded == 0 {
        return base;
    }
    format!(
        "{base} · {}",
        t!("rename-summary-excluded", count = summary.excluded)
    )
}

/// The footer key hints; a command with no key leaves its hint out.
pub(super) fn hints_text(cx: &App) -> String {
    let keys = crate::commands::active(cx);
    let hint = |command: &str, text: fn(String) -> String| {
        let key = keys.shown_in(command, "MultiRename")?;
        Some(text(key.replace("Escape", "Esc")))
    };
    [
        hint("multi_rename.rename", |key| {
            t!("rename-hint-rename", key = key)
        }),
        hint("multi_rename.close", |key| {
            t!("rename-hint-close", key = key)
        }),
        hint("multi_rename.token_menu", |key| {
            t!("rename-hint-tokens", key = key)
        }),
        hint("multi_rename.next_problem", |key| {
            t!("rename-hint-next-problem", key = key)
        }),
        hint("multi_rename.save_preset", |key| {
            t!("rename-hint-save-preset", key = key)
        }),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ")
}

pub(super) struct Footer<Run, Cancel> {
    pub summary: Summary,
    pub hints: String,
    pub can_run: bool,
    pub run: Run,
    pub cancel: Cancel,
}

fn line(id: &'static str, text: SharedString, look: (Hsla, Pixels)) -> impl IntoElement {
    let (color, size) = look;
    div()
        .id(id)
        .role(Role::Label)
        .aria_label(text.clone())
        .test_support()
        .min_w_0()
        .truncate()
        .text_color(color)
        .text_size(size)
        .child(text)
}

fn text_column(t: &Theme, summary: &Summary, hints: String) -> Div {
    let color = if summary.problems > 0 {
        t.colors.mark
    } else {
        t.colors.text
    };
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .child(line(
            "summary",
            summary_text(summary).into(),
            (color, t.typography.body),
        ))
        .child(line(
            "hints",
            hints.into(),
            (t.colors.muted, t.typography.tiny),
        ))
}

pub(super) fn footer<Run, Cancel>(t: &Theme, footer: Footer<Run, Cancel>) -> Div
where
    Run: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    Cancel: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    let Footer {
        summary,
        hints,
        can_run,
        run,
        cancel,
    } = footer;
    let text = text_column(t, &summary, hints);
    let buttons = div()
        .flex()
        .flex_none()
        .gap(t.spacing.xs)
        .child(Button::new("rename-cancel", t!("rename-cancel")).on_click(cancel))
        .child(
            Button::new("rename-run", t!("rename-run"))
                .variant(ButtonVariant::Primary)
                .disabled(!can_run)
                .on_click(run),
        );
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.md)
        .px(t.spacing.md)
        .py(t.spacing.sm)
        .border_t_1()
        .border_color(t.colors.line)
        .bg(t.colors.rail)
        .text_size(t.typography.body)
        .child(text)
        .child(buttons)
}
