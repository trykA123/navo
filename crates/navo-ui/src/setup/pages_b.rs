//! Setup pages Shortcuts, Installing, Finish and Failed; `render_page` in `mod.rs` calls them.

use std::time::Duration;

use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::{
    Animation, AnimationExt, FontWeight, InteractiveElement, Role, SharedString, Stateful,
    StatefulInteractiveElement,
};

use super::pages_a::{Ctx, column, paragraph};
use super::{SetupAction, SetupCheck};
use crate::primitives::access_label::{access_label, plain_text};
use crate::primitives::{Button, CheckBox};
use crate::{AnyElement, App, Div, IntoElement, ParentElement, Styled, div, px, relative};

type Tip = ObservedElement<Stateful<Div>>;

const BAR_HEIGHT: f32 = 6.;
const TIP_CYCLE: Duration = Duration::from_secs(16);
const TIP_AREA_HEIGHT: f32 = 24.;
const BAR_PULSE: Duration = Duration::from_millis(2400);

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

/// Formats a duration as minutes and seconds, `3:12`.
fn clock(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// Builds the indeterminate bar; its fill fades slowly, and stands still under reduced motion.
fn bar(c: &Ctx) -> impl IntoElement {
    let fill = div()
        .h_full()
        .w(relative(0.4))
        .rounded(px(BAR_HEIGHT / 2.))
        .bg(c.t.surfaces.progress_fill);
    let fill: AnyElement = if c.reduce_motion {
        fill.into_any_element()
    } else {
        fill.with_animation(
            "installing-bar-pulse",
            Animation::new(BAR_PULSE).repeat(),
            |fill, delta| fill.opacity(0.55 + 0.45 * (delta * std::f32::consts::TAU).sin().abs()),
        )
        .into_any_element()
    };
    div()
        .id("installing-bar")
        .role(Role::ProgressIndicator)
        .aria_label(c.strings.installing_bar.to_string())
        .test_support()
        .w_full()
        .h(px(BAR_HEIGHT))
        .rounded(px(BAR_HEIGHT / 2.))
        .overflow_hidden()
        .bg(c.t.surfaces.progress_track)
        .child(fill)
}

/// Opacity of tip `index` at `delta` (0..1 of the cycle): a short fade in, a hold, a fade out.
fn tip_opacity(index: usize, tips: usize, delta: f32) -> f32 {
    const EDGE: f32 = 0.06;
    let local = (delta * tips as f32 - index as f32).rem_euclid(tips as f32);
    match local {
        l if l < EDGE => l / EDGE,
        l if l < 1. - EDGE => 1.,
        l if l < 1. => (1. - l) / EDGE,
        _ => 0.,
    }
}

/// Builds one tip line: the key in a small mono chip, then its text.
fn tip_line(c: &Ctx, id: SharedString, key: &SharedString, text: &SharedString) -> Tip {
    let t = c.t;
    div()
        .id(id)
        .role(Role::Label)
        .aria_label(format!("{key}: {text}"))
        .test_support()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .flex()
        .items_center()
        .gap(t.spacing.sm)
        .text_size(t.typography.small)
        .text_color(t.colors.text)
        .child(key_chip(c, key))
        .child(text.clone())
}

/// Builds a key name in a bordered mono chip.
fn key_chip(c: &Ctx, key: &SharedString) -> Div {
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

/// Builds "While you wait": the tips cross-fade every 4 s; reduced motion shows the first.
fn tips(c: &Ctx) -> impl IntoElement {
    let t = c.t;
    let all = &c.strings.installing_tips;
    let shown = if c.reduce_motion { 1 } else { all.len() };
    let count = all.len();
    let lines = all
        .iter()
        .take(shown)
        .enumerate()
        .map(|(index, (key, text))| {
            let id = SharedString::from(format!("installing-tip-{index}"));
            let line = tip_line(c, id.clone(), key, text);
            if c.reduce_motion {
                return line.into_any_element();
            }
            line.opacity(0.)
                .with_animation(
                    id,
                    Animation::new(TIP_CYCLE).repeat(),
                    move |line, delta| line.opacity(tip_opacity(index, count, delta)),
                )
                .into_any_element()
        });
    div()
        .id("installing-tips")
        .flex()
        .flex_col()
        .gap(t.spacing.sm)
        .pt(t.spacing.md)
        .border_t_1()
        .border_color(t.colors.line)
        .child(
            div()
                .text_size(t.typography.small)
                .text_color(t.colors.muted)
                .child(c.strings.installing_tips_title.clone()),
        )
        .child(div().relative().h(px(TIP_AREA_HEIGHT)).children(lines))
}

/// Installing: bar, elapsed time and tips; after 180 s the still-working text with the log. No button.
pub(super) fn installing(c: &Ctx) -> AnyElement {
    let (elapsed, still) = c
        .view
        .progress
        .map_or((Duration::ZERO, false), |p| (p.elapsed, p.still_working));
    let elapsed_text = c
        .strings
        .installing_elapsed
        .replace("{time}", &clock(elapsed));
    let mut page = column(c, &c.strings.installing_title).child(bar(c)).child(
        div()
            .id("installing-elapsed")
            .role(Role::Status)
            .aria_label(elapsed_text.clone())
            .test_support()
            .text_size(c.t.typography.body)
            .text_color(c.t.colors.text)
            .child(elapsed_text),
    );
    if still {
        let log = c.view.log_paths.first().map(|p| p.display().to_string());
        let text = c
            .strings
            .installing_still
            .replace("{log}", &log.unwrap_or_default());
        page = page.child(
            div()
                .id("installing-still")
                .role(Role::Status)
                .aria_label(text.clone())
                .test_support()
                .text_size(c.t.typography.body)
                .text_color(c.t.colors.muted)
                .child(text),
        );
    } else {
        page = page.child(tips(c));
    }
    page.into_any_element()
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

/// Finish: the folder, three first keys, "Start Navo", the Open folder link and the removal hint.
pub(super) fn finish(c: &Ctx) -> AnyElement {
    let heading = c
        .strings
        .finish_title
        .replace("{path}", &c.view.folder_text);
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
    let mut page = column(c, &heading)
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
