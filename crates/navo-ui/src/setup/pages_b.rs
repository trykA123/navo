//! Setup pages Shortcuts, Finish and Failed; `render_page` in `mod.rs` calls them.

use gpui_kit::base::TestSupportExt;
use gpui_kit::{FontWeight, InteractiveElement, Role, SharedString, StatefulInteractiveElement};

use super::pages_a::{Ctx, column, paragraph};
use super::{SetupAction, SetupCheck};
use crate::primitives::access_label::{access_label, plain_text};
use crate::primitives::{Button, CheckBox};
use crate::{AnyElement, App, Div, IntoElement, ParentElement, Styled, div, px};

/// Builds one check box with a muted hint below it.
fn check_row(
    c: &Ctx,
    id: &'static str,
    (label, hint): (&SharedString, &SharedString),
    (on, check): (bool, SetupCheck),
) -> Div {
    let send = c.on_action.clone();
    let boxed = CheckBox::new(id, label.clone())
        .checked(on)
        .show_keys(c.view.show_keys)
        .on_toggle(move |_, window, cx| send(SetupAction::Toggle(check), window, cx));
    div()
        .flex()
        .flex_col()
        .gap(c.t.spacing.xs)
        .child(boxed)
        .child(
            div()
                .pl(px(24.))
                .text_size(c.t.typography.small)
                .text_color(c.t.colors.muted)
                .child(hint.clone()),
        )
}

/// Shortcuts: the Start menu and Desktop check boxes (Fresh install only).
pub(super) fn shortcuts(c: &Ctx) -> AnyElement {
    let s = c.strings;
    column(c, &s.shortcuts_title)
        .child(check_row(
            c,
            "shortcut-start-menu",
            (&s.shortcuts_start_menu, &s.shortcuts_start_menu_hint),
            (c.view.start_menu, SetupCheck::StartMenu),
        ))
        .child(check_row(
            c,
            "shortcut-desktop",
            (&s.shortcuts_desktop, &s.shortcuts_desktop_hint),
            (c.view.desktop, SetupCheck::Desktop),
        ))
        .into_any_element()
}

/// Builds a key name in a bordered mono chip; the Installing tips share it.
pub(super) fn key_chip(c: &Ctx, key: &SharedString) -> Div {
    let t = c.t;
    div()
        .flex_none()
        .px(t.spacing.xs)
        .rounded(t.radii.sm)
        .border_1()
        .border_color(t.colors.line)
        .bg(t.surfaces.field)
        .font_family(t.typography.mono_font.clone())
        .text_size(t.typography.key)
        .font_weight(FontWeight::MEDIUM)
        .child(key.clone())
}

/// Builds the three first keys under the greeting, each as one named row.
fn first_keys(c: &Ctx) -> Div {
    let rows = c
        .strings
        .finish_keys
        .iter()
        .enumerate()
        .map(|(index, (key, text))| {
            div()
                .id(SharedString::from(format!("finish-key-{index}")))
                .role(Role::Label)
                .aria_label(format!("{key} {text}"))
                .test_support()
                .flex()
                .items_center()
                .gap(c.t.spacing.sm)
                .text_size(c.t.typography.small)
                .text_color(c.t.colors.muted)
                .child(key_chip(c, key))
                .child(text.clone())
        });
    div().flex().flex_col().gap(c.t.spacing.xs).children(rows)
}

/// Builds the install-path line under the Finish heading: one line, cut with an ellipsis,
/// named by the whole path so a screen reader still reads it.
fn finish_path(c: &Ctx) -> impl IntoElement {
    let path = c.view.folder_text.clone();
    div()
        .id("finish-path")
        .role(Role::Label)
        .aria_label(path.clone())
        .test_support()
        .min_w_0()
        .whitespace_nowrap()
        .overflow_hidden()
        .text_ellipsis()
        .text_size(c.t.typography.body)
        .text_color(c.t.colors.muted)
        .child(path)
}

/// Finish: the folder, three first keys, "Start Navo", the Open folder link and the removal hint.
pub(super) fn finish(c: &Ctx) -> AnyElement {
    let send = c.on_action.clone();
    let start = CheckBox::new("finish-start", c.strings.finish_start.clone())
        .checked(c.view.start_navo)
        .show_keys(c.view.show_keys)
        .on_toggle(move |_, window, cx| {
            send(SetupAction::Toggle(SetupCheck::StartNavo), window, cx)
        });
    let send = c.on_action.clone();
    let open = div()
        .id("finish-open-folder")
        .role(Role::Link)
        .aria_label(plain_text(&c.strings.finish_open_folder))
        .test_support()
        .focusable()
        .tab_stop(true)
        .cursor_pointer()
        .text_size(c.t.typography.small)
        .text_color(c.t.colors.accent_text)
        .on_click(move |_, window, cx: &mut App| send(SetupAction::OpenFolder, window, cx))
        .child(access_label(
            &c.strings.finish_open_folder,
            c.view.show_keys,
        ));
    let mut page = column(c, &c.strings.finish_title)
        .child(finish_path(c))
        .child(first_keys(c))
        .child(start)
        .child(open);
    if let Some(notice) = &c.view.notice {
        page = page.child(paragraph(c, notice.clone()));
    }
    page.child(paragraph(c, c.strings.finish_remove.to_string()))
        .into_any_element()
}

/// Builds a labelled text line that screen readers announce at once.
fn alert(c: &Ctx, id: &'static str, text: String, strong: bool) -> impl IntoElement {
    div()
        .id(id)
        .role(Role::Alert)
        .aria_label(text.clone())
        .test_support()
        .text_size(c.t.typography.body)
        .text_color(if strong {
            c.t.colors.warn
        } else {
            c.t.colors.text
        })
        .child(text)
}

/// Builds the read-only log path field, one path per line.
fn log_field(c: &Ctx) -> impl IntoElement {
    let log = c
        .view
        .log_paths
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    div()
        .id("failed-log")
        .role(Role::TextInput)
        .aria_label(plain_text(&c.strings.failed_log))
        .aria_value(log.clone())
        .test_support()
        .px(c.t.spacing.sm)
        .py(c.t.spacing.xs)
        .rounded(c.t.radii.md)
        .border_1()
        .border_color(c.t.colors.line)
        .bg(c.t.surfaces.field)
        .font_family(c.t.typography.mono_font.clone())
        .text_size(c.t.typography.small)
        .text_color(c.t.colors.text)
        .child(log)
}

/// Failed: the problem, the log, a leftover-folder warning, Copy details and Retry (when allowed).
pub(super) fn failed(c: &Ctx) -> AnyElement {
    let mut page = column(c, &c.strings.failed_title);
    if let Some(problem) = &c.view.problem {
        page = page.child(alert(c, "failed-problem", problem.clone(), false));
    }
    page = page.child(log_field(c));
    let leftover = c.view.leftover_root.as_ref();
    if let Some(root) = leftover {
        let text = c
            .strings
            .failed_leftover
            .replace("{path}", &root.display().to_string());
        page = page.child(alert(c, "failed-leftover", text, true));
    }
    let copy = c.on_action.clone();
    let retry = c.on_action.clone();
    let buttons = div()
        .flex()
        .gap(c.t.spacing.sm)
        .child(
            Button::new("failed-copy", c.strings.failed_copy.clone())
                .access_key(c.view.show_keys)
                .on_click(move |_, window, cx| copy(SetupAction::CopyDetails, window, cx)),
        )
        .child(
            Button::new("failed-retry", c.strings.failed_retry.clone())
                .access_key(c.view.show_keys)
                .disabled(!c.view.retry_enabled())
                .on_click(move |_, window, cx| retry(SetupAction::TryAgain, window, cx)),
        );
    page.child(buttons).into_any_element()
}
