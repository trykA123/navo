//! Setup pages Welcome, Licence and Folder; `render_page` in `mod.rs` calls them with the theme.

use gpui_kit::base::TestSupportExt;
use gpui_kit::{Entity, FontWeight, InteractiveElement, Role, StatefulInteractiveElement};

use super::{OnAction, SetupAction, SetupCheck, SetupView, Strings};
use crate::format;
use crate::primitives::access_label::{access_label, plain_text};
use crate::primitives::{Button, CheckBox, TextField, TextFieldState};
use crate::tokens::Theme;
use crate::{AnyElement, App, Div, IntoElement, ParentElement, Styled, div, px};

const LICENCE_HEIGHT: f32 = 220.;

/// What every page needs: the theme, the view, the texts and the action sink.
pub(super) struct Ctx<'a> {
    pub t: &'a Theme,
    pub view: &'a SetupView,
    pub strings: &'a Strings,
    pub on_action: &'a OnAction,
    pub reduce_motion: bool,
    pub field: Option<&'a Entity<TextFieldState>>,
}

/// Builds a page column with its heading.
pub(super) fn column(c: &Ctx, heading: &str) -> Div {
    div().flex().flex_col().gap(c.t.spacing.md).child(
        div()
            .id("setup-heading")
            .role(Role::Heading)
            .aria_label(heading.to_owned())
            .test_support()
            .text_size(c.t.typography.body * 1.4)
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(c.t.colors.text)
            .child(heading.to_owned()),
    )
}

/// Builds a muted paragraph.
pub(super) fn paragraph(c: &Ctx, text: String) -> Div {
    div()
        .text_size(c.t.typography.body)
        .text_color(c.t.colors.muted)
        .child(text)
}

/// Welcome: "Install Navo X", or the installed-version text when Setup updates in place.
pub(super) fn welcome(c: &Ctx) -> AnyElement {
    let heading = c
        .strings
        .welcome_install
        .replace("{version}", &c.view.version);
    let body = match &c.view.installed_version {
        Some(installed) => c
            .strings
            .welcome_installed
            .replace("{installed}", installed)
            .replace("{path}", &c.view.folder_text),
        None => c.strings.welcome_blurb.to_string(),
    };
    column(c, &heading)
        .child(paragraph(c, body))
        .into_any_element()
}

/// Licence: the MIT text in a scroll view, the Apache link and the "I accept" check box.
pub(super) fn licence(c: &Ctx) -> AnyElement {
    let title = c.strings.licence_title.to_string();
    let text = div()
        .id("licence-text")
        .role(Role::Document)
        .aria_label(title.clone())
        .test_support()
        .h(px(LICENCE_HEIGHT))
        .p(c.t.spacing.md)
        .overflow_y_scroll()
        .rounded(c.t.radii.md)
        .border_1()
        .border_color(c.t.colors.line)
        .bg(c.t.surfaces.field)
        .font_family(c.t.typography.mono_font.clone())
        .text_size(c.t.typography.small)
        .text_color(c.t.colors.text)
        .child(c.strings.licence_text.clone());
    let url = c.strings.apache_url.clone();
    let link = div()
        .id("licence-apache")
        .role(Role::Link)
        .aria_label(plain_text(&c.strings.licence_apache))
        .test_support()
        .focusable()
        .tab_stop(true)
        .cursor_pointer()
        .rounded(c.t.radii.sm)
        .border_2()
        .border_color(gpui_kit::transparent_black())
        .focus_visible(|style| style.border_color(c.t.colors.accent))
        .text_size(c.t.typography.small)
        .text_color(c.t.colors.accent_text)
        .on_click(move |_, _, cx: &mut App| cx.open_url(&url))
        .child(access_label(&c.strings.licence_apache, c.view.show_keys));
    let send = c.on_action.clone();
    let accept = CheckBox::new("licence-accept", c.strings.licence_accept.clone())
        .checked(c.view.licence_accepted)
        .show_keys(c.view.show_keys)
        .on_toggle(move |_, window, cx| {
            send(SetupAction::Toggle(SetupCheck::AcceptLicence), window, cx)
        });
    column(c, &title)
        .child(text)
        .child(link)
        .child(accept)
        .into_any_element()
}

/// Builds the "needs X, Y free" line, or the unknown-space text.
fn space_line(c: &Ctx) -> String {
    match (c.view.space_needed, c.view.space_free) {
        (Some(needed), Some(free)) => c
            .strings
            .folder_space
            .replace("{needed}", &format::size(needed))
            .replace("{free}", &format::size(free)),
        _ => c.strings.folder_space_unknown.to_string(),
    }
}

/// Builds the path field: editable when the shell gives one, else read-only (dimmed when locked).
fn path_field(c: &Ctx) -> AnyElement {
    let t = c.t;
    let locked = c.view.folder_locked;
    if let Some(field) = c.field.filter(|_| !locked) {
        let label = plain_text(&c.strings.folder_label);
        let input = TextField::new("folder-field", field, label);
        return div().flex_1().min_w_0().child(input).into_any_element();
    }
    div()
        .id("folder-field")
        .role(Role::TextInput)
        .aria_label(plain_text(&c.strings.folder_label))
        .aria_value(c.view.folder_text.clone())
        .test_support()
        .flex_1()
        .min_w_0()
        .px(t.spacing.sm)
        .py(t.spacing.xs)
        .rounded(t.radii.md)
        .border_1()
        .border_color(t.colors.line)
        .bg(t.surfaces.field)
        .font_family(t.typography.mono_font.clone())
        .text_size(t.typography.small)
        .text_color(if locked {
            t.colors.muted
        } else {
            t.colors.text
        })
        .text_ellipsis()
        .child(c.view.folder_text.clone())
        .into_any_element()
}

/// Builds the space line and, when set, the problem line under the field.
fn status_lines(c: &Ctx) -> Div {
    let t = c.t;
    let space = space_line(c);
    let lines = div().flex().flex_col().gap(t.spacing.xs).child(
        div()
            .id("folder-space")
            .role(Role::Status)
            .aria_label(space.clone())
            .test_support()
            .text_size(t.typography.small)
            .text_color(t.colors.muted)
            .child(space),
    );
    let Some(problem) = &c.view.problem else {
        return lines;
    };
    lines.child(
        div()
            .id("folder-problem")
            .role(Role::Alert)
            .aria_label(problem.clone())
            .test_support()
            .text_size(t.typography.small)
            .text_color(t.colors.warn)
            .child(problem.clone()),
    )
}

/// Folder: the path field, Browse (off when locked), space, problem and the uninstall note.
pub(super) fn folder(c: &Ctx) -> AnyElement {
    let send = c.on_action.clone();
    let browse = Button::new("folder-browse", c.strings.folder_browse.clone())
        .access_key(c.view.show_keys)
        .disabled(c.view.folder_locked)
        .on_click(move |_, window, cx| send(SetupAction::Browse, window, cx));
    let row = div()
        .flex()
        .items_center()
        .gap(c.t.spacing.sm)
        .child(path_field(c))
        .child(browse);
    let mut page = column(c, &c.strings.folder_title)
        .child(row)
        .child(status_lines(c));
    if !c.view.folder_locked {
        page = page.child(
            paragraph(c, c.strings.folder_warning.to_string())
                .id("folder-warning")
                .test_support(),
        );
    }
    page.into_any_element()
}
