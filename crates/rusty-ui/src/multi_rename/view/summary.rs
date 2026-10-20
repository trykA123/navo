use gpui_kit::base::TestSupportExt;
use gpui_kit::{Role, SharedString};
use rusty_l10n::t;

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

pub(super) struct Footer<Run, Cancel> {
    pub summary: Summary,
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

pub(super) fn footer<Run, Cancel>(t: &Theme, footer: Footer<Run, Cancel>) -> Div
where
    Run: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    Cancel: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    let Footer {
        summary,
        can_run,
        run,
        cancel,
    } = footer;
    let color = if summary.problems > 0 {
        t.colors.mark
    } else {
        t.colors.text
    };
    let text = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .child(line(
            "summary",
            summary_text(&summary).into(),
            (color, t.typography.body),
        ))
        .child(line(
            "hints",
            t!("rename-hints").into(),
            (t.colors.muted, t.typography.tiny),
        ));
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
