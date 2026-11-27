//! Optional downloadable icon-set dialog inside the button bar editor's icon pane.

use std::collections::BTreeMap;

use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, AppContext, Context, FontWeight, InteractiveElement, IntoElement, ParentElement,
    Role, StatefulInteractiveElement, Styled, Window, div,
};
use navo_l10n::tr;

use crate::format;
use crate::primitives::{Button, ButtonVariant};
use crate::tokens::Theme;
use crate::window_view::NavoWindow;
use crate::window_view::button_bar::{SetAction, SetNews, SetRow};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowBusy {
    Getting,
    Removing,
}

/// Persistent list, busy flags, and per-row errors for the Get-more icon sets view.
pub(crate) struct GetMoreState {
    loading: bool,
    rows: Vec<SetRow>,
    list_error: Option<String>,
    busy: BTreeMap<String, RowBusy>,
    errors: BTreeMap<String, String>,
}

impl GetMoreState {
    fn loading() -> Self {
        Self {
            loading: true,
            rows: Vec::new(),
            list_error: None,
            busy: BTreeMap::new(),
            errors: BTreeMap::new(),
        }
    }
}

impl NavoWindow {
    /// Opens the Get-more icon sets view and loads set status on a background task.
    pub(super) fn open_get_more(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.button_bar.editor.as_mut() else {
            return;
        };
        editor.picker.show_licences = false;
        editor.picker.get_more = Some(GetMoreState::loading());

        self.button_bar.get_more_generation = self.button_bar.get_more_generation.wrapping_add(1);
        let generation = self.button_bar.get_more_generation;

        let Some(sets_fn) = self.button_bar.hooks.as_ref().map(|h| h.sets.clone()) else {
            if let Some(get_more) = self
                .button_bar
                .editor
                .as_mut()
                .and_then(|e| e.picker.get_more.as_mut())
            {
                get_more.loading = false;
                get_more.list_error = Some(tr("button-bar-download-unsupported"));
            }
            cx.notify();
            return;
        };

        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move { sets_fn(SetAction::List) })
                .await;
            let _ = this.update_in(cx, |view, _, cx| {
                view.finish_get_more_list(generation, result, cx);
            });
        })
        .detach();
    }

    fn close_get_more(&mut self, cx: &mut Context<Self>) {
        self.button_bar.get_more_generation = self.button_bar.get_more_generation.wrapping_add(1);
        if let Some(editor) = self.button_bar.editor.as_mut() {
            editor.picker.get_more = None;
            cx.notify();
        }
    }

    fn finish_get_more_list(
        &mut self,
        generation: u64,
        result: Result<SetNews, String>,
        cx: &mut Context<Self>,
    ) {
        if self.button_bar.get_more_generation != generation {
            return;
        }
        let Some(get_more) = self
            .button_bar
            .editor
            .as_mut()
            .and_then(|e| e.picker.get_more.as_mut())
        else {
            return;
        };
        get_more.loading = false;
        match result {
            Ok(SetNews::Listed(rows)) => {
                get_more.rows = rows;
                get_more.list_error = None;
            }
            Ok(_) => {}
            Err(message) => {
                get_more.list_error = Some(message);
            }
        }
        cx.notify();
    }

    fn trigger_get_more_row(
        &mut self,
        id: String,
        kind: RowBusy,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(sets_fn) = self.button_bar.hooks.as_ref().map(|h| h.sets.clone()) else {
            return;
        };
        let Some(get_more) = self
            .button_bar
            .editor
            .as_mut()
            .and_then(|e| e.picker.get_more.as_mut())
        else {
            return;
        };
        if get_more.busy.contains_key(&id) {
            return;
        }
        get_more.errors.remove(&id);
        get_more.busy.insert(id.clone(), kind);

        let generation = self.button_bar.get_more_generation;
        let action = match kind {
            RowBusy::Getting => SetAction::Get(id.clone()),
            RowBusy::Removing => SetAction::Remove(id.clone()),
        };

        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_spawn(async move { sets_fn(action) }).await;
            let _ = this.update_in(cx, |view, window, cx| {
                view.finish_get_more_action(generation, id, result, window, cx);
            });
        })
        .detach();
    }

    fn finish_get_more_action(
        &mut self,
        generation: u64,
        id: String,
        result: Result<SetNews, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.button_bar.get_more_generation != generation {
            if matches!(result, Ok(SetNews::Installed(_) | SetNews::Removed(_)))
                && self.button_bar.editor.is_some()
            {
                self.start_picker_load(window, cx);
            }
            return;
        }
        let Some(get_more) = self
            .button_bar
            .editor
            .as_mut()
            .and_then(|e| e.picker.get_more.as_mut())
        else {
            return;
        };
        get_more.busy.remove(&id);

        let mut reload_index = false;
        match result {
            Ok(SetNews::Installed(done_id)) => {
                set_row_installed(&mut get_more.rows, &done_id, true);
                get_more.errors.remove(&id);
                reload_index = true;
            }
            Ok(SetNews::Removed(done_id)) => {
                set_row_installed(&mut get_more.rows, &done_id, false);
                get_more.errors.remove(&id);
                reload_index = true;
            }
            Ok(SetNews::Listed(rows)) => {
                get_more.rows = rows;
            }
            Err(message) => {
                get_more.errors.insert(id, message);
            }
        }
        if reload_index {
            self.start_picker_load(window, cx);
        }
        cx.notify();
    }

    /// Renders the Get-more optional icon sets view inside the picker pane.
    pub(super) fn get_more_view(
        &self,
        get_more: &GetMoreState,
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let intro = tr("button-bar-get-more-intro");
        let back = Button::new("button-bar-get-more-back", tr("button-bar-get-more-back"))
            .variant(ButtonVariant::Secondary)
            .on_click(cx.listener(|view, _, _, cx| {
                view.close_get_more(cx);
            }));

        div()
            .id("button-bar-get-more-view")
            .test_support()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .gap(t.spacing.sm)
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_between()
                    .gap(t.spacing.md)
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(t.typography.small)
                            .text_color(t.colors.text)
                            .child(tr("button-bar-get-more-title")),
                    )
                    .child(back),
            )
            .child(
                div()
                    .id("button-bar-get-more-intro")
                    .aria_label(intro.clone())
                    .test_support()
                    .flex_none()
                    .text_size(t.typography.small)
                    .text_color(t.colors.muted)
                    .child(intro),
            )
            .child(self.get_more_list(get_more, t, cx))
            .into_any_element()
    }

    fn get_more_list(
        &self,
        get_more: &GetMoreState,
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut list = div()
            .id("button-bar-get-more-list")
            .role(Role::List)
            .aria_label(tr("button-bar-get-more-list-label"))
            .test_support()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_y_scroll()
            .rounded(t.radii.md)
            .border_1()
            .border_color(t.surfaces.card_edge)
            .bg(t.surfaces.field)
            .p(t.spacing.sm)
            .gap(t.spacing.sm);

        if get_more.loading {
            list = list.child(hint_text(tr("button-bar-get-more-loading"), t));
        } else if let Some(err) = get_more.list_error.as_deref() {
            list = list.child(error_text("button-bar-get-more-list-error", err, t));
        } else if get_more.rows.is_empty() {
            list = list.child(hint_text(tr("button-bar-get-more-empty"), t));
        } else {
            for (idx, row) in get_more.rows.iter().enumerate() {
                list = list.child(self.get_more_row(idx, row, get_more, t, cx));
            }
        }
        list.into_any_element()
    }

    fn get_more_row(
        &self,
        idx: usize,
        row: &SetRow,
        get_more: &GetMoreState,
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let size_text = format::size(row.size);
        let busy = get_more.busy.get(&row.id).copied();
        let error = get_more.errors.get(&row.id).map(String::as_str);
        let button = self.get_more_row_button(idx, row, busy, cx);
        let name_color = if busy.is_some() {
            t.colors.muted
        } else {
            t.colors.text
        };
        let aria = format!("{} · {} · {size_text}", row.name, row.licence);

        div()
            .id(("button-bar-get-more-row", idx))
            .role(Role::ListItem)
            .aria_label(aria)
            .test_support()
            .flex()
            .flex_col()
            .flex_none()
            .gap(t.spacing.xs)
            .px(t.spacing.md)
            .py(t.spacing.sm)
            .rounded(t.radii.sm)
            .border_1()
            .border_color(t.surfaces.card_edge)
            .bg(t.surfaces.card)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(t.spacing.md)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .gap(t.spacing.xxs)
                            .child(
                                div()
                                    .truncate()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_size(t.typography.small)
                                    .text_color(name_color)
                                    .child(row.name.clone()),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(t.typography.tiny)
                                    .text_color(t.colors.muted)
                                    .child(format!("{} · {size_text}", row.licence)),
                            ),
                    )
                    .child(div().flex_none().child(button)),
            )
            .when_some(error, |col, msg| {
                col.child(error_text(("button-bar-get-more-error", idx), msg, t))
            })
            .into_any_element()
    }

    fn get_more_row_button(
        &self,
        idx: usize,
        row: &SetRow,
        busy: Option<RowBusy>,
        cx: &mut Context<Self>,
    ) -> Button {
        let (label, variant) = match busy {
            Some(RowBusy::Getting) => (
                tr("button-bar-get-more-downloading"),
                ButtonVariant::Secondary,
            ),
            Some(RowBusy::Removing) => {
                (tr("button-bar-get-more-removing"), ButtonVariant::Secondary)
            }
            None if row.installed => (tr("button-bar-get-more-remove"), ButtonVariant::Secondary),
            None => (tr("button-bar-get-more-get"), ButtonVariant::Primary),
        };
        let id = row.id.clone();
        let kind = if row.installed {
            RowBusy::Removing
        } else {
            RowBusy::Getting
        };
        Button::new(("button-bar-get-more-action", idx), label)
            .variant(variant)
            .disabled(busy.is_some())
            .on_click(cx.listener(move |view, _, window, cx| {
                view.trigger_get_more_row(id.clone(), kind, window, cx);
            }))
    }
}

fn set_row_installed(rows: &mut [SetRow], id: &str, installed: bool) {
    if let Some(row) = rows.iter_mut().find(|row| row.id == id) {
        row.installed = installed;
    }
}

fn hint_text(text: String, t: &Theme) -> gpui_kit::Div {
    div()
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(text)
}

fn error_text(id: impl Into<gpui_kit::ElementId>, message: &str, t: &Theme) -> AnyElement {
    div()
        .id(id)
        .role(Role::Alert)
        .aria_label(message.to_owned())
        .test_support()
        .text_size(t.typography.small)
        .text_color(t.colors.mark)
        .child(message.to_owned())
        .into_any_element()
}
