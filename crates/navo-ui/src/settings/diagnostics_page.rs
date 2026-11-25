//! The Storage group of the About page: summary card, collapsible details fold,
//! per-store rows, and the storage store inspector.

use gpui_kit::Role;
use gpui_kit::base::{TestSupportExt, TestTextExt};

use super::row_state::GUTTER;
use super::view::{Look, row_frame, section_label, send_on_click};
use super::{Handler, Section, SheetEvent};
use crate::prelude::*;
#[cfg(feature = "tester")]
use crate::primitives::ButtonVariant;
use crate::primitives::{Button, IconName, icon};
use crate::tokens::Theme;

#[derive(Clone, Debug)]
pub struct DiagnosticRow {
    pub name: SharedString,
    pub path: SharedString,
    pub bytes: u64,
    pub at_least: bool,
    pub entries: Option<u64>,
    pub budget: Option<u64>,
}

fn locations_text(count: usize) -> SharedString {
    navo_l10n::t!("settings-storage-locations", count = count).into()
}

fn quiet_meta(row: &DiagnosticRow) -> Option<String> {
    let mut parts = Vec::new();
    if row.at_least {
        parts.push(navo_l10n::tr("settings-diagnostics-at-least"));
    }
    if let Some(entries) = row.entries {
        parts.push(format!(
            "{}: {entries}",
            navo_l10n::tr("settings-diagnostics-entries")
        ));
    }
    if let Some(budget) = row.budget {
        parts.push(format!(
            "{}: {}",
            navo_l10n::tr("settings-diagnostics-budget"),
            crate::format::size(budget)
        ));
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

fn details(row: &DiagnosticRow) -> String {
    match quiet_meta(row) {
        Some(meta) => format!("{} · {meta}", crate::format::size(row.bytes)),
        None => crate::format::size(row.bytes),
    }
}

/// Splits a path into a truncatable prefix and a pinned tail of the last 2 path segments.
fn split_path_tail(path: &str) -> (Option<&str>, &str) {
    let trimmed = path.trim_end_matches(['/', '\\']);
    let mut seps = trimmed.match_indices(['/', '\\']).map(|(idx, _)| idx);
    let Some(_) = seps.next_back() else {
        return (None, path);
    };
    let Some(prev) = seps.next_back() else {
        return (None, path);
    };
    if prev == 0 {
        return (None, path);
    }
    let (prefix, tail) = path.split_at(prev);
    (Some(prefix), tail)
}

fn action_button(
    id: &'static str,
    label: impl Into<SharedString>,
    glyph: IconName,
    event: SheetEvent,
    send: Option<&Handler>,
) -> Button {
    let button = Button::new(id, label).icon(glyph);
    let Some(send) = send.cloned() else {
        return button;
    };
    button.on_click(move |_, window, cx| send(&event, window, cx))
}

fn summary_left(t: &Theme, rows: &[DiagnosticRow]) -> Div {
    let total: u64 = rows.iter().map(|row| row.bytes).sum();
    let title: SharedString = navo_l10n::t!(
        "settings-storage-summary",
        size = crate::format::size(total)
    )
    .into();
    let subtitle = locations_text(rows.len());
    div()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .min_w_0()
        .child(icon(IconName::HardDrive, t.spacing.icon, t.colors.muted))
        .child(
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.colors.text)
                        .truncate()
                        .child(title),
                )
                .child(
                    div()
                        .text_size(t.typography.small)
                        .text_color(t.colors.muted)
                        .truncate()
                        .child(subtitle),
                ),
        )
}

fn summary_card(t: &Theme, rows: &[DiagnosticRow], send: Option<&Handler>) -> AnyElement {
    let open_folder = action_button(
        "settings-storage-open-folder",
        navo_l10n::tr("settings-storage-open-folder"),
        IconName::FolderOpen,
        SheetEvent::OpenDataFolder,
        send,
    );
    let refresh = action_button(
        "settings-diagnostics-refresh",
        navo_l10n::tr("settings-diagnostics-refresh"),
        IconName::RefreshCw,
        SheetEvent::RefreshDiagnostics,
        send,
    );
    let header = div()
        .flex()
        .flex_wrap()
        .items_center()
        .justify_between()
        .gap(t.spacing.md)
        .p(t.spacing.lg)
        .child(summary_left(t, rows))
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(t.spacing.sm)
                .child(open_folder)
                .child(refresh),
        );
    div()
        .id("settings-storage-summary")
        .test_support()
        .flex()
        .flex_col()
        .mx(t.spacing.xl)
        .mt(t.spacing.xs)
        .rounded(t.radii.md)
        .border_1()
        .border_color(t.colors.line)
        .bg(t.surfaces.card)
        .child(header)
        .children(save_strip(t))
        .into_any_element()
}

#[cfg(feature = "tester")]
fn save_strip(t: &Theme) -> Option<AnyElement> {
    let save = Button::new(
        "settings-diagnostics-save",
        navo_l10n::tr("cmd-app-save-diagnostic-report"),
    )
    .variant(ButtonVariant::Ghost)
    .icon(IconName::FileText)
    .on_click(|_, window, cx| window.dispatch_action(Box::new(super::SaveDiagnosticReport), cx));
    Some(
        div()
            .flex()
            .items_center()
            .px(t.spacing.lg)
            .py(t.spacing.sm)
            .border_t_1()
            .border_color(t.colors.line)
            .child(save)
            .into_any_element(),
    )
}

#[cfg(not(feature = "tester"))]
fn save_strip(_: &Theme) -> Option<AnyElement> {
    None
}

fn fold_row(t: &Theme, count: usize, is_open: bool, send: Option<&Handler>) -> AnyElement {
    let label = navo_l10n::tr(if is_open {
        "settings-storage-hide-details"
    } else {
        "settings-storage-show-details"
    });
    let count_label = locations_text(count);
    let glyph = if is_open {
        IconName::ChevronDown
    } else {
        IconName::ChevronRight
    };
    let chevron = div()
        .flex()
        .flex_none()
        .justify_center()
        .w(px(GUTTER))
        .child(icon(glyph, t.spacing.icon, t.colors.muted));
    let row = row_frame(t, false)
        .id("settings-storage-fold")
        .role(Role::ListItem)
        .aria_label(label.clone())
        .aria_expanded(is_open)
        .aria_value(count_label.clone())
        .test_support()
        .flex()
        .items_center()
        .gap_x(t.spacing.xl)
        .cursor_pointer()
        .child(chevron)
        .child(div().flex_1().min_w_0().truncate().child(label))
        .child(
            div()
                .flex_none()
                .text_size(t.typography.small)
                .text_color(t.colors.muted)
                .child(count_label),
        );
    send_on_click(row, send, SheetEvent::ToggleDiagnosticsFold).into_any_element()
}

fn path_line(t: &Theme, color: gpui_kit::Hsla, path: &str) -> AnyElement {
    let (prefix, tail) = split_path_tail(path);
    let tail_text = SharedString::from(tail.to_owned());
    let tail_el = div()
        .flex_none()
        .test_text(tail_text.clone())
        .child(tail_text.clone());
    let line = div()
        .flex()
        .items_center()
        .min_w_0()
        .overflow_hidden()
        .font_family(t.typography.mono_font.clone())
        .text_size(t.typography.tiny)
        .text_color(color);
    match prefix {
        Some(prefix) => {
            let prefix_text = SharedString::from(prefix.to_owned());
            line.child(
                div()
                    .min_w_0()
                    .truncate()
                    .test_text(prefix_text.clone())
                    .child(prefix_text),
            )
            .child(tail_el)
            .into_any_element()
        }
        None => line
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .test_text(tail_text.clone())
                    .child(tail_text),
            )
            .into_any_element(),
    }
}

fn store_row(
    t: &Theme,
    index: usize,
    row: &DiagnosticRow,
    is_selected: bool,
    send: Option<&Handler>,
) -> AnyElement {
    let content = t.selected_content(is_selected);
    let left = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .gap(px(2.))
        .child(
            div()
                .font_weight(FontWeight::MEDIUM)
                .text_color(content.colors.text)
                .truncate()
                .child(row.name.clone()),
        )
        .child(path_line(t, content.colors.muted, row.path.as_ref()));
    let right = div()
        .flex()
        .flex_col()
        .flex_none()
        .items_end()
        .gap(px(2.))
        .child(
            div()
                .font_family(t.typography.numbers_font.clone())
                .text_size(t.typography.small)
                .font_weight(FontWeight::MEDIUM)
                .text_color(content.colors.text)
                .child(crate::format::size(row.bytes)),
        )
        .children(quiet_meta(row).map(|meta| {
            div()
                .text_size(t.typography.tiny)
                .text_color(content.colors.muted)
                .child(meta)
        }));
    let frame = row_frame(t, is_selected)
        .id(("settings-diagnostic-row", index))
        .role(Role::ListItem)
        .aria_label(row.name.clone())
        .aria_value(details(row))
        .aria_selected(is_selected)
        .test_support()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .px(t.spacing.md)
        .py(t.spacing.sm)
        .cursor_pointer()
        .child(left)
        .child(right);
    let indented = super::nested::indented(t, frame, true);
    send_on_click(indented, send, SheetEvent::SelectDiagnostic(index)).into_any_element()
}

/// The Storage group of the About page: summary card, details fold, and optional store rows.
pub(super) fn render(look: &Look) -> Vec<AnyElement> {
    let t = look.theme;
    let send = look.send.as_ref();
    let mut rows = vec![
        super::help_page::group_title(t, "settings-diagnostics"),
        summary_card(t, look.diagnostics, send),
    ];
    if look.diagnostics.is_empty() {
        let text = navo_l10n::tr("settings-diagnostics-empty");
        rows.push(
            div()
                .id("settings-diagnostics-empty")
                .role(Role::Status)
                .aria_label(text.clone())
                .test_support()
                .px(t.spacing.xl)
                .py(t.spacing.md)
                .text_color(t.colors.muted)
                .child(text)
                .into_any_element(),
        );
        return rows;
    }
    rows.push(fold_row(
        t,
        look.diagnostics.len(),
        look.diagnostics_open,
        send,
    ));
    if look.diagnostics_open {
        rows.extend(look.diagnostics.iter().enumerate().map(|(index, item)| {
            store_row(
                t,
                index,
                item,
                look.selected_diagnostic == Some(index),
                send,
            )
        }));
    }
    rows
}

fn inspector_stats(t: &Theme, row: &DiagnosticRow) -> Div {
    let size_value = if row.at_least {
        format!(
            "{} ({})",
            crate::format::size(row.bytes),
            navo_l10n::tr("settings-diagnostics-at-least")
        )
    } else {
        crate::format::size(row.bytes)
    };
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.xs)
        .text_size(t.typography.small)
        .child(super::inspector::line(
            t,
            navo_l10n::tr("settings-storage-size"),
            div().font_weight(FontWeight::MEDIUM).child(size_value),
        ))
        .children(row.entries.map(|entries| {
            super::inspector::line(
                t,
                navo_l10n::tr("settings-diagnostics-entries"),
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .child(entries.to_string()),
            )
        }))
        .children(row.budget.map(|budget| {
            super::inspector::line(
                t,
                navo_l10n::tr("settings-diagnostics-budget"),
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .child(crate::format::size(budget)),
            )
        }))
}

fn inspector_location(t: &Theme, path: &SharedString, send: Option<&Handler>) -> Div {
    let open = action_button(
        "settings-diagnostic-open",
        navo_l10n::tr("settings-storage-open-store"),
        IconName::FolderOpen,
        SheetEvent::OpenDiagnosticPath(path.clone()),
        send,
    );
    let copy = action_button(
        "settings-diagnostic-copy",
        navo_l10n::tr("settings-storage-copy-path"),
        IconName::Copy,
        SheetEvent::CopyDiagnosticPath(path.clone()),
        send,
    );
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.xs)
        .pt(t.spacing.xl)
        .border_t_1()
        .border_color(t.colors.line)
        .text_size(t.typography.small)
        .child(
            div()
                .text_color(t.colors.muted)
                .child(navo_l10n::tr("settings-storage-location")),
        )
        .child(
            div()
                .id("settings-diagnostic-path")
                .role(Role::Label)
                .aria_label(path.clone())
                .test_support()
                .font_family(t.typography.mono_font.clone())
                .text_color(t.colors.text)
                .child(path.clone()),
        )
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap(t.spacing.sm)
                .pt(t.spacing.sm)
                .child(open)
                .child(copy),
        )
}

/// The inspector body for a selected storage location: full path, size, counts, and actions.
pub(super) fn inspector(look: &Look, row: &DiagnosticRow) -> AnyElement {
    let t = look.theme;
    let eyebrow = format!(
        "{} · {}",
        section_label(Section::About),
        navo_l10n::tr("settings-diagnostics")
    );
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.xl)
        .p(t.spacing.xl)
        .child(
            div()
                .text_size(t.typography.tiny)
                .font_weight(FontWeight::MEDIUM)
                .text_color(t.colors.muted)
                .child(eyebrow),
        )
        .child(
            div()
                .id("settings-inspector-name")
                .role(Role::Heading)
                .aria_label(row.name.clone())
                .test_support()
                .font_weight(FontWeight::SEMIBOLD)
                .child(row.name.clone()),
        )
        .child(inspector_stats(t, row))
        .child(inspector_location(t, &row.path, look.send.as_ref()))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::split_path_tail;

    #[test]
    fn splits_unix_and_windows_paths_keeping_last_two_segments() {
        assert_eq!(
            split_path_tail("/home/admin/.local/share/navo/cache.db"),
            (Some("/home/admin/.local/share"), "/navo/cache.db")
        );
        assert_eq!(
            split_path_tail(r"C:\Users\admin\AppData\Local\Navo\cache.db"),
            (Some(r"C:\Users\admin\AppData\Local"), r"\Navo\cache.db")
        );
        assert_eq!(split_path_tail("redacted-store"), (None, "redacted-store"));
        assert_eq!(split_path_tail("/navo/cache.db"), (None, "/navo/cache.db"));
    }
}
