//! Skipped-version status and previous-version rollback fold inside the Navo Updates card.

use gpui_kit::Role;
use gpui_kit::base::TestSupportExt;

use super::view::send_on_click;
use super::{Handler, SheetEvent};
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant, IconName, icon};
use crate::tokens::Theme;
use crate::window_view::{RollbackCompatibilityView, RollbackTargetView};

const MAX_SHOWN: usize = 3;

/// UI state for the skipped-version banner and previous-version rollback fold.
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct RollbackState {
    pub skipped_version: Option<SharedString>,
    pub targets: Vec<RollbackTargetView>,
    pub is_open: bool,
    pub confirming: Option<SharedString>,
    pub error: Option<SharedString>,
}

fn clean_version(version: &str) -> &str {
    version.strip_prefix('v').unwrap_or(version)
}

/// Renders the calm skipped-version status line with a quiet "Clear skip" button when set.
pub(super) fn skipped_banner(
    t: &Theme,
    skipped_version: Option<&SharedString>,
    send: Option<&Handler>,
) -> Option<AnyElement> {
    let version = skipped_version.filter(|v| !v.is_empty())?;
    let text: SharedString = navo_l10n::t!(
        "settings-update-skipped",
        version = clean_version(version.as_ref())
    )
    .into();
    let clear = {
        let send = send.cloned();
        Button::new(
            "settings-update-clear-skip",
            navo_l10n::tr("settings-update-clear-skip"),
        )
        .variant(ButtonVariant::Ghost)
        .on_click(move |_, window, cx| {
            if let Some(send) = &send {
                send(&SheetEvent::ClearSkippedVersion, window, cx);
            }
        })
    };
    let status = div()
        .id("settings-update-skipped")
        .role(Role::Status)
        .aria_label(text.clone())
        .test_support()
        .flex_1()
        .min_w_0()
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(text);
    let left = div()
        .flex()
        .flex_1()
        .items_center()
        .gap(t.spacing.sm)
        .min_w_0()
        .child(icon(IconName::Info, t.spacing.icon, t.colors.muted))
        .child(status);
    Some(
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(t.spacing.sm)
            .px(t.spacing.md)
            .py(t.spacing.xs)
            .rounded(t.radii.sm)
            .bg(t.surfaces.field)
            .border_1()
            .border_color(t.colors.line)
            .child(left)
            .child(clear)
            .into_any_element(),
    )
}

/// Renders the collapsed/expanded "Previous versions" rollback section when targets exist.
pub(super) fn rollback_section(
    t: &Theme,
    state: &RollbackState,
    send: Option<&Handler>,
) -> Option<AnyElement> {
    if state.targets.is_empty() && state.error.is_none() {
        return None;
    }
    let shown_count = state.targets.len().min(MAX_SHOWN);
    let header = fold_header(t, shown_count, state.is_open, send);
    let body = state.is_open.then(|| expanded_list(t, state, send));
    Some(
        div()
            .flex()
            .flex_col()
            .border_t_1()
            .border_color(t.colors.line)
            .child(header)
            .children(body)
            .into_any_element(),
    )
}

fn fold_header(t: &Theme, count: usize, is_open: bool, send: Option<&Handler>) -> AnyElement {
    let title = SharedString::from(navo_l10n::tr("settings-update-rollback-title"));
    let count_text = SharedString::from(navo_l10n::t!(
        "settings-update-rollback-count",
        count = count
    ));
    let chevron = if is_open {
        IconName::ChevronDown
    } else {
        IconName::ChevronRight
    };
    let left = div()
        .flex()
        .flex_1()
        .items_center()
        .gap(t.spacing.sm)
        .min_w_0()
        .child(icon(chevron, t.spacing.icon, t.colors.muted))
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_size(t.typography.small)
                .font_weight(FontWeight::MEDIUM)
                .text_color(t.colors.text)
                .child(title.clone()),
        );
    let row = div()
        .id("settings-update-rollback-fold")
        .role(Role::Button)
        .aria_label(title)
        .aria_expanded(is_open)
        .test_support()
        .flex()
        .flex_wrap()
        .items_center()
        .justify_between()
        .gap(t.spacing.sm)
        .px(t.spacing.lg)
        .py(t.spacing.sm)
        .cursor_pointer()
        .hover(|el| el.bg(t.colors.hover))
        .child(left)
        .child(
            div()
                .flex_none()
                .text_size(t.typography.tiny)
                .text_color(t.colors.muted)
                .child(count_text),
        );
    send_on_click(row, send, SheetEvent::ToggleRollbackFold).into_any_element()
}

fn expanded_list(t: &Theme, state: &RollbackState, send: Option<&Handler>) -> AnyElement {
    let error = state.error.clone().map(|msg| {
        div()
            .id("settings-update-rollback-error")
            .role(Role::Alert)
            .aria_label(msg.clone())
            .test_support()
            .text_size(t.typography.small)
            .text_color(t.colors.mark)
            .child(msg)
    });
    let rows = state
        .targets
        .iter()
        .take(MAX_SHOWN)
        .enumerate()
        .map(|(idx, target)| target_row(t, idx, target, state.confirming.as_ref(), send));
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.sm)
        .px(t.spacing.lg)
        .pb(t.spacing.md)
        .children(error)
        .children(rows)
        .into_any_element()
}

fn target_row(
    t: &Theme,
    idx: usize,
    target: &RollbackTargetView,
    confirming: Option<&SharedString>,
    send: Option<&Handler>,
) -> AnyElement {
    let version_label: SharedString = navo_l10n::t!(
        "settings-update-rollback-version",
        version = clean_version(target.version.as_ref())
    )
    .into();
    let is_confirming = confirming == Some(&target.version);
    let info = div()
        .flex()
        .flex_col()
        .flex_1()
        .gap(t.spacing.xxs)
        .min_w_0()
        .child(
            div()
                .text_size(t.typography.small)
                .font_weight(FontWeight::MEDIUM)
                .text_color(t.colors.text)
                .child(version_label.clone()),
        )
        .child(status_note(t, idx, &target.status));
    let actions = if is_confirming {
        confirm_controls(t, idx, &target.version, send)
    } else {
        rollback_button(idx, target, send)
    };
    div()
        .id(("settings-update-rollback-row", idx))
        .role(Role::ListItem)
        .aria_label(version_label)
        .test_support()
        .flex()
        .flex_wrap()
        .items_center()
        .justify_between()
        .gap(t.spacing.sm)
        .px(t.spacing.md)
        .py(t.spacing.sm)
        .rounded(t.radii.sm)
        .bg(t.surfaces.field)
        .border_1()
        .border_color(t.colors.line)
        .child(info)
        .child(actions)
        .into_any_element()
}

fn status_note(t: &Theme, idx: usize, status: &RollbackCompatibilityView) -> AnyElement {
    let text: SharedString = match status {
        RollbackCompatibilityView::Compatible => {
            navo_l10n::tr("settings-update-rollback-compatible").into()
        }
        RollbackCompatibilityView::CompatibleWithBackup => {
            navo_l10n::tr("settings-update-rollback-backup").into()
        }
        RollbackCompatibilityView::Incompatible { reason } => reason.clone(),
    };
    div()
        .id(("settings-update-rollback-reason", idx))
        .role(Role::Note)
        .aria_label(text.clone())
        .test_support()
        .min_w_0()
        .text_size(t.typography.tiny)
        .text_color(t.colors.muted)
        .child(text)
        .into_any_element()
}

fn rollback_button(idx: usize, target: &RollbackTargetView, send: Option<&Handler>) -> AnyElement {
    let is_compatible = !matches!(
        target.status,
        RollbackCompatibilityView::Incompatible { .. }
    );
    let mut button = Button::new(
        ("settings-update-rollback", idx),
        navo_l10n::tr("settings-update-rollback-action"),
    )
    .variant(ButtonVariant::Secondary)
    .disabled(!is_compatible);
    if is_compatible && let Some(send) = send.cloned() {
        let version = target.version.clone();
        button = button.on_click(move |_, window, cx| {
            send(&SheetEvent::PromptRollback(version.clone()), window, cx);
        });
    }
    button.into_any_element()
}

fn confirm_controls(
    t: &Theme,
    idx: usize,
    version: &SharedString,
    send: Option<&Handler>,
) -> AnyElement {
    let prompt: SharedString = navo_l10n::t!(
        "settings-update-rollback-confirm",
        version = clean_version(version.as_ref())
    )
    .into();
    let confirm = {
        let send = send.cloned();
        let version = version.clone();
        Button::new(
            ("settings-update-rollback-confirm", idx),
            navo_l10n::tr("settings-update-rollback-confirm-action"),
        )
        .variant(ButtonVariant::Primary)
        .on_click(move |_, window, cx| {
            if let Some(send) = &send {
                send(&SheetEvent::ConfirmRollback(version.clone()), window, cx);
            }
        })
    };
    let cancel = {
        let send = send.cloned();
        Button::new(
            ("settings-update-rollback-cancel", idx),
            navo_l10n::tr("settings-update-rollback-cancel"),
        )
        .variant(ButtonVariant::Ghost)
        .on_click(move |_, window, cx| {
            if let Some(send) = &send {
                send(&SheetEvent::CancelRollback, window, cx);
            }
        })
    };
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(t.spacing.xs)
        .min_w_0()
        .child(
            div()
                .id(("settings-update-rollback-confirm-text", idx))
                .role(Role::Status)
                .aria_label(prompt.clone())
                .test_support()
                .min_w_0()
                .text_size(t.typography.tiny)
                .font_weight(FontWeight::MEDIUM)
                .text_color(t.colors.text)
                .child(prompt),
        )
        .child(confirm)
        .child(cancel)
        .into_any_element()
}
