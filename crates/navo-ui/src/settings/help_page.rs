use gpui_kit::Role;
use gpui_kit::base::TestSupportExt;

use super::view::Look;
use super::{Field, HELP_ROW, Handler, Section, SheetEvent};
use crate::prelude::*;
use crate::primitives::{Button, Kbd};
use crate::tokens::Theme;
use crate::window_view::{UpdateFailure, UpdateView};

const STATUS_MIN: f32 = 140.;

pub(super) const WORDS: &str =
    "help user guide manual keyboard shortcuts keys version updates update automatically check";

fn button(id: &'static str, text_key: &str, event: SheetEvent, send: Option<&Handler>) -> Button {
    let send = send.cloned();
    Button::new(id, navo_l10n::tr(text_key)).on_click(move |_, window, cx| {
        if let Some(send) = &send {
            send(&event, window, cx);
        }
    })
}

fn controls(t: &Theme, send: Option<&Handler>) -> AnyElement {
    let guide = button(
        "settings-help-guide",
        "settings-help-guide",
        SheetEvent::OpenGuide,
        send,
    );
    let keys = button(
        "settings-help-keys",
        "settings-help-keys",
        SheetEvent::Shortcuts,
        send,
    );
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(t.spacing.md)
        .px(t.spacing.xl)
        .py(t.spacing.md)
        .child(guide)
        .child(keys)
        .children(Kbd::new("settings-help-keys-hint", "f1"))
        .into_any_element()
}

fn version(t: &Theme, version: &SharedString) -> AnyElement {
    let text = SharedString::from(navo_l10n::t!(
        "settings-help-version",
        version = version.as_ref()
    ));
    div()
        .id("settings-help-version")
        .role(Role::Label)
        .aria_label(text.clone())
        .test_support()
        .px(t.spacing.xl)
        .py(t.spacing.md)
        .text_size(t.typography.small)
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
            "settings-update-ready",
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
    }
}

fn is_busy(view: &UpdateView) -> bool {
    matches!(view, UpdateView::Checking | UpdateView::Downloading { .. })
}

fn update_line(look: &Look) -> AnyElement {
    let t = look.theme;
    let text = update_text(look.update);
    let is_failed = matches!(look.update, UpdateView::Failed(_));
    let status = div()
        .id("settings-update-status")
        .role(Role::Status)
        .aria_label(text.clone())
        .test_support()
        .flex_1()
        .min_w(px(STATUS_MIN))
        .text_size(t.typography.small)
        .text_color(if is_failed {
            t.colors.mark
        } else {
            t.colors.muted
        })
        .child(text);
    let check = (!matches!(look.update, UpdateView::Hidden)).then(|| {
        let send = look.send.clone();
        Button::new(
            "settings-update-check",
            navo_l10n::tr("settings-update-check"),
        )
        .disabled(is_busy(look.update))
        .on_click(move |_, window, cx| {
            if let Some(send) = &send {
                send(&SheetEvent::CheckUpdates, window, cx);
            }
        })
    });
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(t.spacing.md)
        .px(t.spacing.xl)
        .py(t.spacing.md)
        .child(status)
        .children(check)
        .into_any_element()
}

pub(super) fn render(look: &Look) -> Vec<AnyElement> {
    let t = look.theme;
    let toggle = super::view::row(look, HELP_ROW, Field::CheckUpdates.spec());
    vec![
        super::view::section_heading(t, Section::Help),
        controls(t, look.send.as_ref()),
        version(t, look.version),
        update_line(look),
        toggle,
    ]
}
