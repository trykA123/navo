//! Standalone Settings content: title row, body and page-specific key-hint footer.

use gpui_kit::base::TestSupportExt;
use gpui_kit::{RenderOnce, Role, Toggled};

use super::{
    CONTEXT, CloseSettings, FindSetting, Handler, SettingsBody, SheetEvent, UndoChange, row_state,
    send,
};
use crate::prelude::*;
use crate::primitives::Kbd;
use crate::side_sheet::SideSheet;
use crate::tokens;

struct ContentHint {
    keys: &'static [&'static str],
    label: SharedString,
}

/// The Settings header row, body and key-hint footer, independent of the sheet frame.
#[derive(IntoElement)]
pub struct SettingsContent {
    title: SheetTitle,
    body: SettingsBody,
    hints: Vec<ContentHint>,
}

/// Builds the Settings header row, `body` and the key hints that fit the shown page.
pub fn content(body: SettingsBody) -> SettingsContent {
    let title = SheetTitle {
        changed_count: body.values.changed_count(),
        is_changed_only: body.filter.is_changed_only(),
        on_event: body.on_event.clone(),
    };
    let hints = hints_for(&body);
    SettingsContent { title, body, hints }
}

impl SettingsContent {
    /// Places the title row, body and key hints into a `SideSheet` frame.
    pub(super) fn into_sheet(self, sheet: SideSheet) -> SideSheet {
        let mut sheet = sheet.header(self.title).body(self.body);
        for hint in self.hints {
            sheet = sheet.hint(hint.keys, hint.label);
        }
        sheet
    }
}

fn hints_for(body: &SettingsBody) -> Vec<ContentHint> {
    let hint = |keys, id| ContentHint {
        keys,
        label: navo_l10n::tr(id).into(),
    };
    if body.shortcuts.is_some() {
        return vec![
            hint(&["↑", "↓"], "settings-hint-move"),
            hint(&["enter"], "settings-hint-record"),
            hint(&["ctrl-r"], "settings-hint-reset"),
            hint(&["ctrl-tab"], "settings-hint-page"),
        ];
    }
    let mut hints = vec![
        hint(&["↑", "↓"], "settings-hint-move"),
        hint(&["←", "→"], "settings-hint-change"),
        hint(&["ctrl-tab"], "settings-hint-page"),
        hint(&["f3"], "settings-hint-toml-line"),
    ];
    if body.reset_event().is_some() {
        hints.push(hint(&["f8"], "settings-hint-reset"));
    }
    hints.push(hint(&["ctrl-z"], "settings-hint-undo"));
    hints.push(hint(&["ctrl-f"], "settings-hint-find"));
    hints
}

impl RenderOnce for SettingsContent {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx);
        let header = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.lg)
            .min_w_0()
            .py(t.spacing.md)
            .px(t.spacing.xl)
            .border_b_1()
            .border_color(t.surfaces.card_edge)
            .child(self.title);
        let body = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_hidden()
            .child(self.body);
        div()
            .id("settings-content")
            .role(Role::Group)
            .aria_label(navo_l10n::tr("settings-title"))
            .test_support()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .text_color(t.colors.text)
            .child(header)
            .child(body)
            .child(footer_strip(t, self.hints))
    }
}

fn footer_strip(t: &tokens::Theme, hints: Vec<ContentHint>) -> Div {
    let chips = hints.into_iter().enumerate().map(|(index, entry)| {
        let keys: Vec<Kbd> = entry
            .keys
            .iter()
            .enumerate()
            .filter_map(|(at, key)| Kbd::new(("key", at), key))
            .collect();
        div()
            .id(("settings-hint", index))
            .role(Role::Label)
            .aria_label(entry.label.clone())
            .test_support()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.xs)
            .children(keys)
            .child(entry.label)
    });
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.xl)
        .px(t.spacing.xl)
        .py(t.spacing.sm)
        .border_t_1()
        .border_color(t.surfaces.card_edge)
        .bg(t.surfaces.field)
        .text_color(t.colors.muted)
        .text_size(t.typography.small)
        .whitespace_nowrap()
        .overflow_hidden()
        .children(chips)
}

#[derive(IntoElement)]
struct SheetTitle {
    changed_count: usize,
    is_changed_only: bool,
    on_event: Option<Handler>,
}

fn changed_chip(
    t: &tokens::Theme,
    count: usize,
    is_active: bool,
    handler: Option<&Handler>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let mut args = navo_l10n::FluentArgs::new();
    args.set("count", count as i64);
    let label: SharedString = navo_l10n::tr_args("settings-changed-chip", &args).into();
    let focus = window
        .use_keyed_state("settings-changed-chip-focus", cx, |_, cx| {
            cx.focus_handle().tab_stop(true).tab_index(1)
        })
        .read(cx)
        .clone();
    let theme = t.selected_content(is_active);
    let (bg, edge, text_color) = if is_active {
        (
            theme.colors.selection,
            theme.colors.accent,
            theme.colors.accent_text,
        )
    } else {
        (t.surfaces.field, t.surfaces.card_edge, t.colors.text)
    };
    let click_send = handler.cloned();
    div()
        .id("settings-changed-chip")
        .role(Role::Button)
        .aria_label(label.clone())
        .aria_selected(is_active)
        .aria_toggled(if is_active {
            Toggled::True
        } else {
            Toggled::False
        })
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.xs)
        .px(t.spacing.md)
        .py(t.spacing.xxs)
        .rounded(t.radii.pill)
        .border_1()
        .border_color(edge)
        .bg(bg)
        .text_size(t.typography.small)
        .text_color(text_color)
        .when(is_active, |chip| chip.font_weight(FontWeight::MEDIUM))
        .cursor_pointer()
        .track_focus(&focus)
        .focus_visible(|chip| chip.border_color(t.colors.accent))
        .when(!is_active, |chip| {
            chip.hover(|chip| chip.bg(t.colors.hover))
        })
        .on_click(move |_, window, cx| {
            send(
                click_send.as_ref(),
                Some(SheetEvent::ToggleChangedFilter),
                window,
                cx,
            );
        })
        .child(row_state::dot(&theme))
        .child(label)
        .into_any_element()
}

impl RenderOnce for SheetTitle {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let changed = (self.changed_count > 0).then(|| {
            changed_chip(
                &t,
                self.changed_count,
                self.is_changed_only,
                self.on_event.as_ref(),
                window,
                cx,
            )
        });
        let close_send = self.on_event.clone();
        let find_send = self.on_event.clone();
        let undo_send = self.on_event.clone();
        div()
            .key_context(CONTEXT)
            .on_action(move |_: &CloseSettings, window, cx| {
                send(close_send.as_ref(), Some(SheetEvent::Close), window, cx);
            })
            .on_action(move |_: &FindSetting, window, cx| {
                send(find_send.as_ref(), Some(SheetEvent::Find), window, cx);
            })
            .on_action(move |_: &UndoChange, window, cx| {
                send(undo_send.as_ref(), Some(SheetEvent::Undo), window, cx);
            })
            .flex()
            .items_center()
            .gap(t.spacing.lg)
            .child(
                div()
                    .font_family(t.typography.brand_font.clone())
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(navo_l10n::tr("settings-title")),
            )
            .child(row_state::live_chip(&t))
            .children(changed)
    }
}
