use gpui_kit::Role;
use gpui_kit::base::TestSupportExt;

use super::{Handler, Section, SheetEvent};
use crate::prelude::*;
use crate::primitives::Button;
use crate::tokens::Theme;

#[derive(Clone, Debug)]
pub struct DiagnosticRow {
    pub name: SharedString,
    pub path: SharedString,
    pub bytes: u64,
    pub at_least: bool,
    pub entries: Option<u64>,
    pub budget: Option<u64>,
}

fn details(row: &DiagnosticRow) -> String {
    let mut details = vec![crate::format::size(row.bytes)];
    if row.at_least {
        details.push(rusty_l10n::tr("settings-diagnostics-at-least"));
    }
    if let Some(entries) = row.entries {
        details.push(format!(
            "{}: {entries}",
            rusty_l10n::tr("settings-diagnostics-entries")
        ));
    }
    if let Some(budget) = row.budget {
        details.push(format!(
            "{}: {}",
            rusty_l10n::tr("settings-diagnostics-budget"),
            crate::format::size(budget)
        ));
    }
    details.join(" · ")
}

fn row(t: &Theme, index: usize, row: &DiagnosticRow) -> AnyElement {
    let details = details(row);
    div()
        .id(("settings-diagnostic-row", index))
        .role(Role::ListItem)
        .aria_label(row.name.clone())
        .aria_value(details.clone())
        .test_support()
        .flex()
        .flex_col()
        .flex_none()
        .px(t.spacing.xl)
        .py(t.spacing.md)
        .child(
            div()
                .font_weight(FontWeight::MEDIUM)
                .child(row.name.clone()),
        )
        .child(
            div()
                .text_size(t.typography.small)
                .text_color(t.colors.muted)
                .child(row.path.clone()),
        )
        .child(details)
        .into_any_element()
}

fn controls(t: &Theme, send: Option<&Handler>) -> AnyElement {
    let send = send.cloned();
    let refresh = Button::new(
        "settings-diagnostics-refresh",
        rusty_l10n::tr("settings-diagnostics-refresh"),
    )
    .on_click(move |_, window, cx| {
        if let Some(send) = &send {
            send(&SheetEvent::RefreshDiagnostics, window, cx);
        }
    });
    div()
        .flex()
        .flex_wrap()
        .gap(t.spacing.md)
        .px(t.spacing.xl)
        .py(t.spacing.md)
        .child(refresh)
        .children(save_button())
        .into_any_element()
}

#[cfg(feature = "tester")]
fn save_button() -> Option<Button> {
    Some(
        Button::new(
            "settings-diagnostics-save",
            rusty_l10n::tr("cmd-app-save-diagnostic-report"),
        )
        .on_click(|_, window, cx| {
            window.dispatch_action(Box::new(super::SaveDiagnosticReport), cx)
        }),
    )
}

#[cfg(not(feature = "tester"))]
fn save_button() -> Option<Button> {
    None
}

pub(super) fn render(look: &super::view::Look) -> Vec<AnyElement> {
    let t = look.theme;
    let mut rows = vec![super::view::section_heading(t, Section::Diagnostics)];
    rows.extend(
        look.diagnostics
            .iter()
            .enumerate()
            .map(|(index, item)| row(t, index, item)),
    );
    if look.diagnostics.is_empty() {
        let text = rusty_l10n::tr("settings-diagnostics-empty");
        rows.push(
            div()
                .id("settings-diagnostics-empty")
                .role(Role::Status)
                .aria_label(text.clone())
                .test_support()
                .px(t.spacing.xl)
                .py(t.spacing.md)
                .text_color(t.colors.muted)
                .child(text)
                .into_any_element(),
        );
    }
    rows.push(controls(t, look.send.as_ref()));
    rows
}
