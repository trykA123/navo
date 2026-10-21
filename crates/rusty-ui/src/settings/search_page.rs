use gpui_kit::base::TestSupportExt;
use gpui_kit::{Role, Stateful};

use super::{Handler, SheetEvent};
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant};
use crate::tokens::Theme;

const ROW_MIN: f32 = 48.;
const DOT: f32 = 8.;

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum ServiceStatus {
    NotInstalled,
    Running,
    Stopped,
    #[default]
    NotAvailable,
}

impl ServiceStatus {
    #[must_use]
    pub fn text(self) -> SharedString {
        let id = match self {
            Self::NotInstalled => "settings-search-service-not-installed",
            Self::Running => "settings-search-service-running",
            Self::Stopped => "settings-search-service-stopped",
            Self::NotAvailable => "settings-search-service-not-available",
        };
        rusty_l10n::tr(id).into()
    }

    #[must_use]
    pub fn can_install(self) -> bool {
        self == Self::NotInstalled
    }

    #[must_use]
    pub fn can_remove(self) -> bool {
        matches!(self, Self::Running | Self::Stopped)
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct SearchPage {
    pub status: ServiceStatus,
    pub is_windows: bool,
    pub is_cleared: bool,
}

pub(super) const SERVICE_WORDS: &str = "index service install remove";
pub(super) const HISTORY_WORDS: &str = "search history clear remember";

fn heading(t: &Theme, id: &'static str) -> AnyElement {
    let text = rusty_l10n::tr(id);
    div()
        .flex_none()
        .px(t.spacing.xl)
        .pt(t.spacing.md)
        .text_size(t.typography.tiny)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(t.colors.muted)
        .child(text)
        .into_any_element()
}

fn row(
    t: &Theme,
    id: &'static str,
    (title, help): (&'static str, &'static str),
    controls: Vec<AnyElement>,
) -> Stateful<Div> {
    let name = div()
        .font_weight(FontWeight::MEDIUM)
        .child(rusty_l10n::tr(title));
    let help = div()
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(rusty_l10n::tr(help));
    let line = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(t.spacing.lg)
        .children(controls);
    div()
        .id(id)
        .flex()
        .flex_col()
        .gap(t.spacing.sm)
        .flex_none()
        .min_h(px(ROW_MIN))
        .px(t.spacing.xl)
        .py(t.spacing.md)
        .child(div().child(name).child(help))
        .child(line)
}

fn status_chip(t: &Theme, status: ServiceStatus) -> impl IntoElement + use<> {
    let color = match status {
        ServiceStatus::Running => t.colors.ok,
        ServiceStatus::NotInstalled | ServiceStatus::Stopped => t.colors.accent,
        ServiceStatus::NotAvailable => t.colors.muted,
    };
    let text = status.text();
    div()
        .id("settings-service-status")
        .role(Role::Status)
        .aria_label(text.clone())
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.sm)
        .text_size(t.typography.small)
        .child(
            div()
                .flex_none()
                .size(px(DOT))
                .rounded(t.radii.full)
                .bg(color),
        )
        .child(text)
}

fn button(
    (id, label): (&'static str, &'static str),
    is_enabled: bool,
    event: SheetEvent,
    send: Option<&Handler>,
) -> Button {
    let send = send.cloned();
    Button::new(id, rusty_l10n::tr(label))
        .disabled(!is_enabled)
        .when_some(send, |button, send| {
            button.on_click(move |_, window, cx| send(&event, window, cx))
        })
}

fn service_row(t: &Theme, page: &SearchPage, send: Option<&Handler>) -> AnyElement {
    let status = page.status;
    let install = button(
        (
            "settings-service-install",
            "settings-search-service-install",
        ),
        status.can_install(),
        SheetEvent::InstallService,
        send,
    )
    .tooltip(rusty_l10n::tr("settings-search-service-install-tip"));
    let remove = button(
        ("settings-service-remove", "settings-search-service-remove"),
        status.can_remove(),
        SheetEvent::RemoveService,
        send,
    )
    .variant(ButtonVariant::Ghost)
    .tooltip(rusty_l10n::tr("settings-search-service-remove-tip"));
    let controls = vec![
        status_chip(t, status).into_any_element(),
        install.into_any_element(),
        remove.into_any_element(),
    ];
    row(
        t,
        "settings-service",
        ("settings-search-service", "settings-search-service-help"),
        controls,
    )
    .into_any_element()
}

fn windows_only_note(t: &Theme) -> AnyElement {
    div()
        .id("settings-service-note")
        .role(Role::Note)
        .aria_label(rusty_l10n::tr("settings-search-service-windows-only"))
        .test_support()
        .flex_none()
        .px(t.spacing.xl)
        .py(t.spacing.md)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(rusty_l10n::tr("settings-search-service-windows-only"))
        .into_any_element()
}

fn history_row(t: &Theme, page: &SearchPage, send: Option<&Handler>) -> AnyElement {
    let cleared = page.is_cleared.then(|| {
        div()
            .id("settings-history-cleared")
            .role(Role::Status)
            .aria_label(rusty_l10n::tr("settings-search-history-cleared"))
            .test_support()
            .text_size(t.typography.small)
            .text_color(t.colors.muted)
            .child(rusty_l10n::tr("settings-search-history-cleared"))
    });
    let clear = button(
        ("settings-clear-history", "settings-search-history-clear"),
        true,
        SheetEvent::ClearHistory,
        send,
    );
    let controls = [
        Some(clear.into_any_element()),
        cleared.map(IntoElement::into_any_element),
    ];
    row(
        t,
        "settings-history",
        ("settings-search-history", "settings-search-history-help"),
        controls.into_iter().flatten().collect(),
    )
    .into_any_element()
}

pub(super) fn service_block(
    t: &Theme,
    page: &SearchPage,
    send: Option<&Handler>,
) -> Vec<AnyElement> {
    let body = if page.is_windows {
        service_row(t, page, send)
    } else {
        windows_only_note(t)
    };
    vec![heading(t, "settings-search-service-heading"), body]
}

pub(super) fn history_block(
    t: &Theme,
    page: &SearchPage,
    send: Option<&Handler>,
) -> Vec<AnyElement> {
    vec![
        heading(t, "settings-search-history-heading"),
        history_row(t, page, send),
    ]
}
