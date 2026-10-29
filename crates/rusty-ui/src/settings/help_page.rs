use gpui_kit::Role;
use gpui_kit::base::TestSupportExt;

use super::view::Look;
use super::{Handler, Section, SheetEvent};
use crate::prelude::*;
use crate::primitives::{Button, Kbd};
use crate::tokens::Theme;

pub(super) const WORDS: &str = "help user guide manual keyboard shortcuts keys version";

fn button(id: &'static str, text_key: &str, event: SheetEvent, send: Option<&Handler>) -> Button {
    let send = send.cloned();
    Button::new(id, rusty_l10n::tr(text_key)).on_click(move |_, window, cx| {
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
    let text = SharedString::from(rusty_l10n::t!(
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

pub(super) fn render(look: &Look) -> Vec<AnyElement> {
    let t = look.theme;
    vec![
        super::view::section_heading(t, Section::Help),
        controls(t, look.send.as_ref()),
        version(t, look.version),
    ]
}
