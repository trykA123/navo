//! The Help and Navo Updates section of the About page.

use gpui_kit::Role;
use gpui_kit::base::TestSupportExt;

use super::view::Look;
use super::{Field, HELP_ROW, Handler, SheetEvent};
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant, IconName, Kbd, icon};
use crate::tokens::Theme;
use crate::window_view::{UpdateFailure, UpdateView};

const PROGRESS_HEIGHT: f32 = 6.;
const PROGRESS_MIN_W: f32 = 80.;
const STATUS_MIN_W: f32 = 140.;

pub(super) const WORDS: &str = "help user guide manual keyboard shortcuts keys version updates update automatically check rollback previous versions skipped";

fn action_button(
    id: &'static str,
    text_key: &str,
    glyph: Option<IconName>,
    event: SheetEvent,
    send: Option<&Handler>,
) -> Button {
    let send = send.cloned();
    let button = Button::new(id, navo_l10n::tr(text_key)).on_click(move |_, window, cx| {
        if let Some(send) = &send {
            send(&event, window, cx);
        }
    });
    match glyph {
        Some(glyph) => button.icon(glyph),
        None => button,
    }
}

fn check_button(look: &Look, text_key: &str, disabled: bool) -> Button {
    let send = look.send.clone();
    Button::new("settings-update-check", navo_l10n::tr(text_key))
        .icon(IconName::RefreshCw)
        .disabled(disabled)
        .on_click(move |_, window, cx| {
            if let Some(send) = &send {
                send(&SheetEvent::CheckUpdates, window, cx);
            }
        })
}

fn installer_button(text_key: &str, variant: ButtonVariant, send: Option<&Handler>) -> Button {
    action_button(
        "settings-update-installer",
        text_key,
        Some(IconName::ExternalLink),
        SheetEvent::OpenReleases,
        send,
    )
    .variant(variant)
}

fn restart_button(look: &Look) -> Option<Button> {
    (look.restart_offered && matches!(look.update, UpdateView::Ready { .. })).then(|| {
        action_button(
            "settings-update-restart",
            "settings-update-restart",
            Some(IconName::RotateCw),
            SheetEvent::RestartUpdate,
            look.send.as_ref(),
        )
        .variant(ButtonVariant::Primary)
    })
}

fn controls(t: &Theme, send: Option<&Handler>) -> AnyElement {
    let guide = action_button(
        "settings-help-guide",
        "settings-help-guide",
        None,
        SheetEvent::OpenGuide,
        send,
    );
    let keys = action_button(
        "settings-help-keys",
        "settings-help-keys",
        None,
        SheetEvent::Shortcuts,
        send,
    );
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(t.spacing.md)
        .px(t.spacing.xl)
        .py(t.spacing.xs)
        .child(guide)
        .child(keys)
        .children(Kbd::new("settings-help-keys-hint", "f1"))
        .into_any_element()
}

fn version_label(t: &Theme, version: &SharedString) -> AnyElement {
    let text = SharedString::from(navo_l10n::t!(
        "settings-help-version",
        version = version.as_ref()
    ));
    div()
        .id("settings-help-version")
        .role(Role::Label)
        .aria_label(text.clone())
        .test_support()
        .px(t.spacing.sm)
        .py(t.spacing.xxs)
        .rounded(t.radii.sm)
        .bg(t.surfaces.field)
        .border_1()
        .border_color(t.colors.line)
        .text_size(t.typography.tiny)
        .text_color(t.colors.muted)
        .child(text)
        .into_any_element()
}

/// The D22g and D22h sentence for one update state.
pub(super) fn update_text(view: &UpdateView) -> SharedString {
    let key = |key: &str| SharedString::from(navo_l10n::tr(key));
    match view {
        UpdateView::Hidden => key("settings-update-hidden"),
        UpdateView::Off => key("settings-update-off"),
        UpdateView::Idle => key("settings-update-idle"),
        UpdateView::Checking => key("settings-update-checking"),
        UpdateView::UpToDate => key("settings-update-current"),
        UpdateView::Downloading { version, percent } => SharedString::from(navo_l10n::t!(
            "settings-update-downloading",
            version = version.as_ref(),
            percent = i64::from(*percent)
        )),
        UpdateView::Ready { version } => SharedString::from(navo_l10n::t!(
            "settings-update-ready-restart",
            version = version.as_ref()
        )),
        UpdateView::Failed(failure) => failure_text(failure),
    }
}

fn failure_text(failure: &UpdateFailure) -> SharedString {
    match failure {
        UpdateFailure::Offline => navo_l10n::tr("settings-update-offline").into(),
        UpdateFailure::RateLimited { next_try } => SharedString::from(navo_l10n::t!(
            "settings-update-rate-limited",
            time = next_try.as_ref()
        )),
        UpdateFailure::BadFeed => navo_l10n::tr("settings-update-bad-feed").into(),
        UpdateFailure::Interrupted => navo_l10n::tr("settings-update-interrupted").into(),
        UpdateFailure::Damaged => navo_l10n::tr("settings-update-damaged").into(),
        UpdateFailure::Busy => navo_l10n::tr("settings-update-busy").into(),
        UpdateFailure::Apply => navo_l10n::tr("settings-update-apply").into(),
        UpdateFailure::OthersRunning => navo_l10n::tr("settings-update-others-running").into(),
    }
}

fn status_badge(t: &Theme, view: &UpdateView) -> AnyElement {
    let (key, glyph, fg, border, bg) = match view {
        UpdateView::UpToDate => (
            "settings-update-badge-current",
            Some(IconName::CircleCheck),
            t.colors.ok,
            t.colors.ok,
            t.surfaces.card,
        ),
        UpdateView::Idle => (
            "settings-update-badge-idle",
            None,
            t.colors.muted,
            t.colors.line,
            t.surfaces.field,
        ),
        UpdateView::Checking => (
            "settings-update-badge-checking",
            Some(IconName::LoaderCircle),
            t.colors.muted,
            t.colors.line,
            t.surfaces.field,
        ),
        UpdateView::Downloading { .. } => (
            "settings-update-badge-downloading",
            Some(IconName::LoaderCircle),
            t.colors.accent_text,
            t.colors.accent,
            t.colors.selection,
        ),
        UpdateView::Ready { .. } => (
            "settings-update-badge-ready",
            Some(IconName::CircleCheck),
            t.colors.accent_text,
            t.colors.accent,
            t.surfaces.card,
        ),
        UpdateView::Failed(_) => (
            "settings-update-badge-failed",
            Some(IconName::TriangleAlert),
            t.colors.mark,
            t.colors.mark,
            t.surfaces.card,
        ),
        UpdateView::Hidden | UpdateView::Off => (
            "settings-update-badge-manual",
            Some(IconName::Info),
            t.colors.muted,
            t.colors.line,
            t.surfaces.field,
        ),
    };
    let label = SharedString::from(navo_l10n::tr(key));
    div()
        .id("settings-update-badge")
        .role(Role::Status)
        .aria_label(label.clone())
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.xs)
        .px(t.spacing.md)
        .py(t.spacing.xxs)
        .rounded(t.radii.full)
        .border_1()
        .border_color(border)
        .bg(bg)
        .text_size(t.typography.tiny)
        .font_weight(FontWeight::MEDIUM)
        .text_color(fg)
        .when_some(glyph, |badge, glyph| {
            badge.child(icon(glyph, t.typography.small, fg))
        })
        .child(label)
        .into_any_element()
}

fn card_header(t: &Theme, version: &SharedString, view: &UpdateView) -> AnyElement {
    let left = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(t.spacing.sm)
        .min_w_0()
        .child(icon(IconName::Info, t.spacing.icon, t.colors.muted))
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(t.colors.text)
                .child(navo_l10n::tr("settings-updates-title")),
        )
        .child(version_label(t, version));
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .justify_between()
        .gap(t.spacing.md)
        .px(t.spacing.lg)
        .py(t.spacing.sm)
        .child(left)
        .child(status_badge(t, view))
        .into_any_element()
}

fn status_text(t: &Theme, view: &UpdateView, strong: bool) -> AnyElement {
    let text = update_text(view);
    let color = match view {
        UpdateView::Failed(_) => t.colors.mark,
        UpdateView::Ready { .. } => t.colors.text,
        _ => t.colors.muted,
    };
    div()
        .id("settings-update-status")
        .role(Role::Status)
        .aria_label(text.clone())
        .test_support()
        .min_w(px(STATUS_MIN_W))
        .text_size(t.typography.small)
        .text_color(color)
        .when(strong, |el| el.font_weight(FontWeight::MEDIUM))
        .child(text)
        .into_any_element()
}

fn auto_unavailable(t: &Theme) -> AnyElement {
    let text = SharedString::from(navo_l10n::tr("settings-update-auto-unavailable"));
    div()
        .id("settings-update-auto-unavailable")
        .role(Role::Status)
        .aria_label(text.clone())
        .test_support()
        .text_size(t.typography.tiny)
        .text_color(t.colors.muted)
        .child(text)
        .into_any_element()
}

fn note_line(t: &Theme, key: &str) -> AnyElement {
    div()
        .text_size(t.typography.tiny)
        .text_color(t.colors.muted)
        .child(navo_l10n::tr(key))
        .into_any_element()
}

fn state_row(
    t: &Theme,
    lead_icon: AnyElement,
    details: AnyElement,
    actions: Vec<AnyElement>,
) -> AnyElement {
    let left = div()
        .flex()
        .flex_1()
        .items_start()
        .gap(t.spacing.md)
        .min_w(px(STATUS_MIN_W))
        .child(div().flex_none().pt(t.spacing.xxs).child(lead_icon))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(t.spacing.xxs)
                .min_w_0()
                .child(details),
        );
    let right = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(t.spacing.sm)
        .children(actions);
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .justify_between()
        .gap(t.spacing.md)
        .child(left)
        .child(right)
        .into_any_element()
}

fn progress_bar(t: &Theme, percent: u8) -> AnyElement {
    let clamped = percent.min(100);
    let pct_text = SharedString::from(navo_l10n::t!(
        "settings-update-progress-percent",
        percent = i64::from(clamped)
    ));
    let track = div()
        .id("settings-update-progress")
        .role(Role::ProgressIndicator)
        .aria_label(pct_text.clone())
        .test_support()
        .flex_1()
        .min_w(px(PROGRESS_MIN_W))
        .h(px(PROGRESS_HEIGHT))
        .rounded(t.radii.full)
        .overflow_hidden()
        .bg(t.surfaces.field)
        .border_1()
        .border_color(t.colors.line)
        .child(
            div()
                .h_full()
                .w(relative(f32::from(clamped) / 100.))
                .rounded(t.radii.full)
                .bg(t.colors.accent),
        );
    div()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .child(track)
        .child(
            div()
                .flex_none()
                .text_size(t.typography.tiny)
                .font_weight(FontWeight::MEDIUM)
                .text_color(t.colors.text)
                .child(pct_text),
        )
        .into_any_element()
}

fn failed_body(look: &Look) -> AnyElement {
    let t = look.theme;
    let row = state_row(
        t,
        icon(IconName::TriangleAlert, t.spacing.icon, t.colors.mark).into_any_element(),
        status_text(t, look.update, true),
        vec![
            check_button(look, "settings-update-retry", false).into_any_element(),
            installer_button(
                "settings-update-download-manual",
                ButtonVariant::Ghost,
                look.send.as_ref(),
            )
            .into_any_element(),
        ],
    );
    div()
        .p(t.spacing.md)
        .rounded(t.radii.md)
        .border_1()
        .border_color(t.colors.mark)
        .bg(t.surfaces.field)
        .child(row)
        .into_any_element()
}

enum IconTone {
    Ok,
    Muted,
    Accent,
}

fn simple_row(look: &Look, glyph: IconName, tone: IconTone, disabled: bool) -> AnyElement {
    let t = look.theme;
    let fg = match tone {
        IconTone::Ok => t.colors.ok,
        IconTone::Muted => t.colors.muted,
        IconTone::Accent => t.colors.accent_text,
    };
    state_row(
        t,
        icon(glyph, t.spacing.icon, fg).into_any_element(),
        status_text(t, look.update, false),
        vec![check_button(look, "settings-update-check", disabled).into_any_element()],
    )
}

fn details_col(t: &Theme, top: AnyElement, note_key: &str) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.xxs)
        .child(top)
        .child(note_line(t, note_key))
        .into_any_element()
}

fn ready_body(look: &Look) -> AnyElement {
    let t = look.theme;
    let details = details_col(
        t,
        status_text(t, look.update, true),
        "settings-update-ready-note",
    );
    let actions = match restart_button(look) {
        Some(restart) => vec![
            restart.into_any_element(),
            check_button(look, "settings-update-check", false)
                .variant(ButtonVariant::Ghost)
                .into_any_element(),
        ],
        None => vec![check_button(look, "settings-update-check", false).into_any_element()],
    };
    state_row(
        t,
        icon(IconName::CircleCheck, t.spacing.icon, t.colors.accent_text).into_any_element(),
        details,
        actions,
    )
}

fn hidden_body(look: &Look) -> AnyElement {
    let t = look.theme;
    let row = state_row(
        t,
        icon(IconName::Info, t.spacing.icon, t.colors.muted).into_any_element(),
        status_text(t, look.update, false),
        vec![
            installer_button(
                "settings-update-download-installer",
                ButtonVariant::Secondary,
                look.send.as_ref(),
            )
            .into_any_element(),
        ],
    );
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.xs)
        .child(row)
        .child(auto_unavailable(t))
        .into_any_element()
}

fn off_body(look: &Look) -> AnyElement {
    let t = look.theme;
    let details = details_col(
        t,
        status_text(t, look.update, false),
        "settings-update-off-note",
    );
    state_row(
        t,
        icon(IconName::Info, t.spacing.icon, t.colors.muted).into_any_element(),
        details,
        vec![
            check_button(look, "settings-update-check", false).into_any_element(),
            installer_button(
                "settings-update-download-installer",
                ButtonVariant::Ghost,
                look.send.as_ref(),
            )
            .into_any_element(),
        ],
    )
}

fn card_body(look: &Look) -> AnyElement {
    let t = look.theme;
    let content = match look.update {
        UpdateView::UpToDate => simple_row(look, IconName::CircleCheck, IconTone::Ok, false),
        UpdateView::Idle => simple_row(look, IconName::Info, IconTone::Muted, false),
        UpdateView::Checking => simple_row(look, IconName::LoaderCircle, IconTone::Muted, true),
        UpdateView::Downloading { percent, .. } => div()
            .flex()
            .flex_col()
            .gap(t.spacing.md)
            .child(simple_row(
                look,
                IconName::LoaderCircle,
                IconTone::Accent,
                true,
            ))
            .child(progress_bar(t, *percent))
            .into_any_element(),
        UpdateView::Ready { .. } => ready_body(look),
        UpdateView::Failed(_) => failed_body(look),
        UpdateView::Hidden => hidden_body(look),
        UpdateView::Off => off_body(look),
    };
    let skipped = (*look.update != UpdateView::Hidden)
        .then(|| {
            super::rollback_section::skipped_banner(
                t,
                look.rollback.skipped_version.as_ref(),
                look.send.as_ref(),
            )
        })
        .flatten();
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.md)
        .px(t.spacing.lg)
        .pb(t.spacing.sm)
        .child(content)
        .children(skipped)
        .into_any_element()
}

fn update_card(look: &Look, cx: &App) -> AnyElement {
    let t = look.theme;
    let is_ready = matches!(look.update, UpdateView::Ready { .. });
    let toggle = super::view::row(look, HELP_ROW, Field::CheckUpdates.spec(), cx);
    let footer = div()
        .border_t_1()
        .border_color(t.colors.line)
        .px(t.spacing.sm)
        .child(toggle);
    let rollback = (*look.update != UpdateView::Hidden)
        .then(|| super::rollback_section::rollback_section(t, look.rollback, look.send.as_ref()))
        .flatten();
    div()
        .id("settings-update-card")
        .test_support()
        .flex()
        .flex_col()
        .mx(t.spacing.xl)
        .mt(t.spacing.xs)
        .rounded(t.radii.md)
        .border_1()
        .border_color(if is_ready {
            t.colors.accent
        } else {
            t.colors.line
        })
        .bg(if is_ready {
            t.colors.selection
        } else {
            t.surfaces.card
        })
        .child(card_header(t, look.version, look.update))
        .child(card_body(look))
        .child(footer)
        .children(rollback)
        .into_any_element()
}

/// A small titled group heading on the About page; `id` is the ftl message of the title.
pub(super) fn group_title(t: &Theme, id: &'static str) -> AnyElement {
    let label = navo_l10n::tr(id);
    div()
        .id(id)
        .role(Role::Heading)
        .aria_label(label.clone())
        .test_support()
        .flex_none()
        .px(t.spacing.xl)
        .pt(t.spacing.md)
        .text_size(t.typography.tiny)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(t.colors.muted)
        .child(label)
        .into_any_element()
}

/// The Help and Navo Updates group of the About page, above Storage.
pub(super) fn render(look: &Look, cx: &App) -> Vec<AnyElement> {
    let t = look.theme;
    vec![
        group_title(t, "settings-help"),
        controls(t, look.send.as_ref()),
        update_card(look, cx),
    ]
}
