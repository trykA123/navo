//! Setup pages Blocked, Same, Newer, Ambiguous, Elevated and NoPayload; `render_page` calls them.

use gpui_kit::base::TestSupportExt;
use gpui_kit::{InteractiveElement, Role, SharedString, StatefulInteractiveElement};

use super::SetupAction;
use super::pages_a::{Ctx, column, paragraph};
use crate::primitives::Button;
use crate::primitives::access_label::plain_text;
use crate::{AnyElement, Div, IntoElement, ParentElement, Styled, div};

/// Builds a named text block; refusals use `Role::Alert` so screen readers announce them.
fn text_block(c: &Ctx, id: &'static str, role: Role, text: String) -> impl IntoElement {
    div()
        .id(id)
        .role(role)
        .aria_label(text.clone())
        .test_support()
        .text_size(c.t.typography.body)
        .text_color(c.t.colors.text)
        .child(text)
}

/// Builds one button whose click sends `action`; its label drops the `&` marker.
fn action_button(
    c: &Ctx,
    id: &'static str,
    label: &SharedString,
    (action, disabled): (SetupAction, bool),
) -> Button {
    let send = c.on_action.clone();
    Button::new(id, plain_text(label))
        .disabled(disabled)
        .on_click(move |_, window, cx| send(action, window, cx))
}

/// Builds the Close button every page ends with.
fn close_button(c: &Ctx, id: &'static str) -> Button {
    action_button(c, id, &c.strings.close, (SetupAction::Close, false))
}

/// Lays the given buttons in a row.
fn buttons(c: &Ctx, row: impl IntoIterator<Item = Button>) -> Div {
    div()
        .flex()
        .gap(c.t.spacing.sm)
        .children(row.into_iter().map(IntoElement::into_any_element))
}

/// Fills `{installed}` and `{path}` in a template from the view.
fn fill(c: &Ctx, template: &SharedString) -> String {
    let installed = c.view.installed_version.as_deref().unwrap_or_default();
    template
        .replace("{installed}", installed)
        .replace("{path}", &c.view.folder_text)
}

/// Builds a page of a heading, one text block and buttons.
fn simple(
    c: &Ctx,
    heading: &SharedString,
    text: (&'static str, Role, String),
    row: Vec<Button>,
) -> Div {
    column(c, heading)
        .child(text_block(c, text.0, text.1, text.2))
        .child(buttons(c, row))
}

/// Blocked: Navo runs from the install folder; Try again (when allowed) and Close.
pub(super) fn blocked(c: &Ctx) -> AnyElement {
    let s = c.strings;
    let text = ("blocked-text", Role::Alert, fill(c, &s.blocked_text));
    let retry = action_button(
        c,
        "blocked-retry",
        &s.blocked_retry,
        (SetupAction::TryAgain, !c.view.can_retry),
    );
    column(c, &s.blocked_title)
        .child(text_block(c, text.0, text.1, text.2))
        .child(paragraph(c, s.blocked_note.to_string()))
        .child(buttons(c, [retry, close_button(c, "blocked-close")]))
        .into_any_element()
}

/// Same: this version is installed; Open Navo and Close.
pub(super) fn same(c: &Ctx) -> AnyElement {
    let s = c.strings;
    let open = action_button(c, "same-open", &s.same_open, (SetupAction::Primary, false));
    let text = ("same-text", Role::Label, fill(c, &s.same_text));
    simple(
        c,
        &s.same_title,
        text,
        vec![open, close_button(c, "same-close")],
    )
    .into_any_element()
}

/// Newer: a newer version is installed; Close only.
pub(super) fn newer(c: &Ctx) -> AnyElement {
    let s = c.strings;
    let text = ("newer-text", Role::Label, fill(c, &s.newer_text));
    simple(
        c,
        &s.newer_title,
        text,
        vec![close_button(c, "newer-close")],
    )
    .into_any_element()
}

/// Ambiguous: the repair text, or the shell's own text (UnsafeName never suggests uninstalling).
pub(super) fn ambiguous(c: &Ctx) -> AnyElement {
    let s = c.strings;
    let own = c.view.notice.clone();
    let body = own.unwrap_or_else(|| s.ambiguous_text.to_string());
    let text = ("ambiguous-text", Role::Alert, body);
    simple(
        c,
        &s.ambiguous_title,
        text,
        vec![close_button(c, "ambiguous-close")],
    )
    .into_any_element()
}

/// Elevated: Setup refuses to run as administrator; Close only.
pub(super) fn elevated(c: &Ctx) -> AnyElement {
    let s = c.strings;
    let text = ("elevated-text", Role::Alert, s.elevated_text.to_string());
    simple(
        c,
        &s.elevated_title,
        text,
        vec![close_button(c, "elevated-close")],
    )
    .into_any_element()
}

/// NoPayload: this Setup carries no Navo; Close only.
pub(super) fn no_payload(c: &Ctx) -> AnyElement {
    let s = c.strings;
    let text = ("nopayload-text", Role::Alert, s.no_payload_text.to_string());
    simple(
        c,
        &s.no_payload_title,
        text,
        vec![close_button(c, "nopayload-close")],
    )
    .into_any_element()
}
