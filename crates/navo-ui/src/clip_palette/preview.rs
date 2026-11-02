use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Div, FontWeight, InteractiveElement, ParentElement, Role, SharedString, Stateful,
    StatefulInteractiveElement, Styled, div, px, relative,
};
use navo_l10n::{t, tr};

use super::age::age;
use super::rows::{MASK, RowElement, is_mono, kind_icon};
use super::{ClipKind, ClipRow, Origin, Scope};
use crate::primitives::{IconName, icon};
use crate::tokens::Theme;

const PREVIEW_CHARS: usize = 8_000;
const INDENT: &str = "  ";

/// Where the preview sits and how it is read.
#[derive(Clone, Copy)]
pub(super) struct Look<'a> {
    pub now_ms: i64,
    pub is_docked: bool,
    /// The text of a secret that is on show.
    pub secret: Option<&'a str>,
}

fn kind_name(kind: ClipKind) -> String {
    tr(match kind {
        ClipKind::Text => "clip-kind-text",
        ClipKind::Link => "clip-kind-link",
        ClipKind::Path => "clip-kind-path",
        ClipKind::Files => "clip-kind-files",
        ClipKind::Image => "clip-kind-image",
        ClipKind::Secret => "clip-kind-secret",
    })
}

fn shorten(text: &str) -> String {
    match text.char_indices().nth(PREVIEW_CHARS) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_owned(),
    }
}

fn split(path: &str) -> Option<(&str, &str)> {
    path.rfind(['\\', '/']).map(|at| path.split_at(at))
}

/// Splits a file list into the shared folder and the file names; a mixed list stays whole.
pub(super) fn file_list(detail: &str) -> (Option<String>, Vec<String>) {
    let paths: Vec<&str> = detail.lines().filter(|line| !line.is_empty()).collect();
    let folders: Vec<Option<&str>> = paths.iter().map(|path| split(path).map(|s| s.0)).collect();
    let shared = folders.first().copied().flatten();
    if shared.is_none() || folders.iter().any(|folder| *folder != shared) {
        return (None, paths.iter().map(|path| (*path).to_owned()).collect());
    }
    let names = paths
        .iter()
        .filter_map(|path| split(path).map(|(_, name)| name[1..].to_owned()))
        .collect();
    (shared.map(str::to_owned), names)
}

pub(super) fn full_text(row: &ClipRow) -> &str {
    if row.detail.is_empty() {
        &row.title
    } else {
        &row.detail
    }
}

fn head_text(row: &ClipRow) -> String {
    match (row.from, &row.source_app) {
        (Origin::Clipboard, Some(app)) => {
            t!(
                "clip-preview-from",
                kind = kind_name(row.kind),
                app = app.clone()
            )
        }
        (Origin::Clipboard, None) => kind_name(row.kind),
        (Origin::Saved, _) => row.title.clone(),
    }
}

fn head(t: &Theme, row: &ClipRow) -> Div {
    let (name, color) = kind_icon(t, row.kind);
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.md)
        .font_weight(FontWeight::SEMIBOLD)
        .child(icon(name, t.spacing.icon, color))
        .child(div().min_w_0().truncate().child(head_text(row)))
}

fn content(t: &Theme, row: &ClipRow, secret: Option<&str>) -> Stateful<Div> {
    let is_mono = is_mono(row.kind) || row.kind == ClipKind::Secret;
    let body = div()
        .id("clip-palette-preview-text")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .text_color(t.colors.text)
        .line_height(relative(1.5))
        .when(is_mono, |body| {
            body.font_family(t.typography.mono_font.clone())
                .text_size(t.typography.small)
        });
    match row.kind {
        ClipKind::Secret => match secret {
            Some(text) => body.child(SharedString::from(text.to_owned())),
            None => body.child(MASK),
        },
        ClipKind::Files => {
            let (folder, names) = file_list(&shorten(full_text(row)));
            body.children(folder.map(|folder| div().child(SharedString::from(folder))))
                .children(
                    names
                        .into_iter()
                        .map(|name| div().child(SharedString::from(format!("{INDENT}{name}")))),
                )
        }
        _ => body.child(SharedString::from(shorten(full_text(row)))),
    }
}

fn note(t: &Theme) -> Div {
    div()
        .flex()
        .flex_none()
        .items_start()
        .gap(t.spacing.sm)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(
            div()
                .flex()
                .items_center()
                .h(t.typography.line_height)
                .child(icon(IconName::Shield, t.typography.small, t.colors.warn)),
        )
        .child(div().min_w_0().child(tr("clip-secret-note")))
}

fn size(row: &ClipRow) -> String {
    match row.kind {
        ClipKind::Secret => tr("clip-size-hidden"),
        ClipKind::Files => t!("clip-size-files", count = file_list(&row.detail).1.len()),
        _ => t!("clip-size-chars", count = full_text(row).chars().count()),
    }
}

fn facts(row: &ClipRow, now_ms: i64) -> Vec<(String, String, bool)> {
    let when = age(now_ms, row.when_ms);
    match (row.from, &row.scope) {
        (Origin::Clipboard, _) => vec![
            (tr("clip-meta-copied"), when, false),
            (tr("clip-meta-size"), size(row), false),
        ],
        (Origin::Saved, scope) => {
            let (folder, is_folder) = match scope {
                Scope::Everywhere => (tr("clip-scope-global"), false),
                Scope::Folder(location) => (location.as_str().to_owned(), true),
            };
            let mut facts = vec![
                (tr("clip-meta-kind"), kind_name(row.kind), false),
                (tr("clip-meta-folder"), folder, is_folder),
                (tr("clip-meta-used"), when, false),
            ];
            if row.kind != ClipKind::Secret {
                facts.push((tr("clip-meta-size"), size(row), false));
            }
            facts
        }
    }
}

fn details(t: &Theme, row: &ClipRow, now_ms: i64) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_none()
        .gap(t.spacing.xxs)
        .pt(t.spacing.md)
        .border_t_1()
        .border_color(t.colors.line)
        .text_size(t.typography.small)
        .children(
            facts(row, now_ms)
                .into_iter()
                .map(|(label, value, is_mono)| {
                    div()
                        .flex()
                        .gap(t.spacing.xxl)
                        .child(
                            div()
                                .flex_none()
                                .w(px(56.))
                                .text_color(t.colors.muted)
                                .child(label),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .when(is_mono, |value| {
                                    value.font_family(t.typography.mono_font.clone())
                                })
                                .child(SharedString::from(value)),
                        )
                }),
        )
}

/// The selected row's full value; empty when nothing is selected.
pub(super) fn preview(t: &Theme, row: Option<&ClipRow>, look: Look<'_>) -> RowElement {
    let pane = div()
        .id("clip-palette-preview")
        .role(Role::Group)
        .aria_label(tr("clip-preview-label"))
        .test_support()
        .flex()
        .flex_col()
        .min_w_0()
        .min_h_0()
        .overflow_hidden()
        .when(look.is_docked, |pane| {
            pane.gap(t.spacing.xxs).px(t.spacing.xl).py(t.spacing.md)
        })
        .when(!look.is_docked, |pane| {
            pane.flex_1()
                .gap(t.spacing.md)
                .px(t.spacing.xxl)
                .py(t.spacing.xl)
        });
    let Some(row) = row else {
        return pane;
    };
    pane.child(head(t, row))
        .child(content(t, row, look.secret))
        .when(!look.is_docked && row.kind == ClipKind::Secret, |pane| {
            pane.child(note(t))
        })
        .when(!look.is_docked, |pane| {
            pane.child(details(t, row, look.now_ms))
        })
}
