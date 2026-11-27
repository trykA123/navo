//! Action and metadata form view for the selected button bar item.

use std::rc::Rc;

use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, Context, Div, FontWeight, InteractiveElement, IntoElement, ParentElement,
    Role, Stateful, StatefulInteractiveElement, Styled, Window, div, px,
};
use navo_l10n::tr;

use super::editor_tab_button;
use super::form_state::{FormKind, FormState, args_takes_files, filter_navo_commands};
use crate::commands;
use crate::primitives::{ListScroll, RowSemantics, TextField, VirtualList};
use crate::tokens::{self, Theme};
use crate::window_view::NavoWindow;
use crate::window_view::button_bar::ButtonAction;
use crate::window_view::button_bar::state::ButtonBarEditorState;

const ROW_HEIGHT: f32 = 28.;
const LIST_MIN_HEIGHT: f32 = 120.;

type PickRow = Rc<dyn Fn(usize, &mut Window, &mut App)>;

struct CommandRow {
    label: String,
    secondary: Option<String>,
    selected: bool,
}

struct ListSpec<'a> {
    list_id: &'static str,
    row_prefix: &'static str,
    aria_label: String,
    scroll: &'a ListScroll,
    rows: Rc<[CommandRow]>,
    on_pick: PickRow,
}

impl NavoWindow {
    /// Renders the button form pane (or a hint when empty or a separator is selected).
    pub(super) fn editor_button_pane(
        &self,
        editor: &ButtonBarEditorState,
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> ObservedElement<Stateful<Div>> {
        let pane = div()
            .id("button-bar-editor-button-pane")
            .test_support()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .p(t.spacing.xl);

        let Some(item) = self.selected_item(editor.selected) else {
            return pane.child(muted_hint(tr("button-bar-editor-select-hint"), t));
        };
        match &item.action {
            ButtonAction::Separator => {
                pane.child(muted_hint(tr("button-bar-form-separator-hint"), t))
            }
            ButtonAction::Menu(_) => menu_name_form(&editor.form, t, pane),
            ButtonAction::Navo(_) | ButtonAction::User(_) | ButtonAction::Program { .. } => {
                pane.child(self.button_form_body(&editor.form, t, cx))
            }
        }
    }

    fn button_form_body(&self, form: &FormState, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let body = match form.kind {
            FormKind::User => self.user_command_tab(form, t, cx),
            FormKind::Navo => self.navo_command_tab(form, t, cx),
            FormKind::Program => self.program_tab(form, t, cx),
        };
        div()
            .id("button-bar-form")
            .test_support()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .gap(t.spacing.md)
            .child(labeled_field(
                t,
                tr("button-bar-form-name"),
                TextField::new(
                    "button-bar-form-name",
                    &form.name_input,
                    tr("button-bar-form-name"),
                ),
            ))
            .child(self.kind_tabs(form.kind, t, cx))
            .child(body)
            .when_some(form.error.as_deref(), |el, msg| {
                el.child(form_error_line(msg, t))
            })
            .into_any_element()
    }

    fn kind_tabs(&self, active: FormKind, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let tabs = [
            (
                FormKind::User,
                "button-bar-form-kind-user",
                "button-bar-form-kind-user",
            ),
            (
                FormKind::Navo,
                "button-bar-form-kind-navo",
                "button-bar-form-kind-navo",
            ),
            (
                FormKind::Program,
                "button-bar-form-kind-program",
                "button-bar-form-kind-program",
            ),
        ];
        let mut row = div()
            .id("button-bar-form-kind-tabs")
            .role(Role::TabList)
            .aria_label(tr("button-bar-form-kind-label"))
            .test_support()
            .flex()
            .flex_none()
            .flex_wrap()
            .items_center()
            .gap(t.spacing.xs);

        for (kind, id, msg) in tabs {
            let tab = editor_tab_button(id, tr(msg), active == kind, t).on_click(cx.listener(
                move |view, _, window, cx| {
                    view.select_form_kind(kind, window, cx);
                },
            ));
            row = row.child(tab);
        }
        row.into_any_element()
    }

    fn user_command_tab(&self, form: &FormState, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let names: Rc<[String]> = self
            .user_command_list()
            .iter()
            .map(|info| info.name.clone())
            .collect();
        if names.is_empty() {
            return div()
                .flex_1()
                .min_h_0()
                .child(muted_hint(tr("button-bar-form-no-user-commands"), t))
                .into_any_element();
        }
        let rows: Rc<[CommandRow]> = self
            .user_command_list()
            .iter()
            .map(|info| CommandRow {
                label: info.name.clone(),
                secondary: info
                    .key
                    .as_deref()
                    .map(|k| commands::shown(k).unwrap_or_else(|| k.to_owned())),
                selected: info.name == form.user_name,
            })
            .collect();
        let view = cx.entity();
        command_list(
            ListSpec {
                list_id: "button-bar-form-user-list",
                row_prefix: "button-bar-form-user",
                aria_label: tr("button-bar-form-user-list-label"),
                scroll: &form.user_scroll,
                rows,
                on_pick: Rc::new(move |idx, window, cx| {
                    let name = names[idx].clone();
                    view.update(cx, |view, cx| view.pick_user_command(name, window, cx));
                }),
            },
            t,
        )
    }

    fn navo_command_tab(&self, form: &FormState, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let query = form.navo_filter.read(cx).value();
        let filtered = filter_navo_commands(&query);
        let filter = TextField::new(
            "button-bar-form-navo-filter",
            &form.navo_filter,
            tr("button-bar-form-navo-filter"),
        );

        let list_area = if filtered.is_empty() {
            div()
                .flex_1()
                .min_h_0()
                .child(muted_hint(tr("button-bar-form-no-navo-commands"), t))
                .into_any_element()
        } else {
            let ids: Rc<[&'static str]> = filtered.iter().map(|(id, _)| *id).collect();
            let rows: Rc<[CommandRow]> = filtered
                .into_iter()
                .map(|(id, label)| CommandRow {
                    label,
                    secondary: Some(id.to_owned()),
                    selected: id == form.navo_id,
                })
                .collect();
            let view = cx.entity();
            command_list(
                ListSpec {
                    list_id: "button-bar-form-navo-list",
                    row_prefix: "button-bar-form-navo",
                    aria_label: tr("button-bar-form-navo-list-label"),
                    scroll: &form.navo_scroll,
                    rows,
                    on_pick: Rc::new(move |idx, window, cx| {
                        let id = ids[idx];
                        view.update(cx, |view, cx| view.pick_navo_command(id, window, cx));
                    }),
                },
                t,
            )
        };

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .gap(t.spacing.sm)
            .child(filter)
            .child(list_area)
            .into_any_element()
    }

    fn program_tab(&self, form: &FormState, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let args_value = form.args_input.read(cx).value();
        let show_drop_note = args_takes_files(&args_value);

        div()
            .id("button-bar-form-program-fields")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .gap(t.spacing.sm)
            .child(labeled_field(
                t,
                tr("button-bar-form-program"),
                TextField::new(
                    "button-bar-form-program",
                    &form.program_input,
                    tr("button-bar-form-program"),
                ),
            ))
            .child(labeled_field(
                t,
                tr("button-bar-form-args"),
                TextField::new(
                    "button-bar-form-args",
                    &form.args_input,
                    tr("button-bar-form-args"),
                ),
            ))
            .when(show_drop_note, |col| {
                col.child(
                    div()
                        .id("button-bar-form-drop-note")
                        .test_support()
                        .text_size(t.typography.small)
                        .text_color(t.colors.muted)
                        .child(tr("button-bar-form-drop-note")),
                )
            })
            .child(labeled_field(
                t,
                tr("button-bar-form-start-in"),
                TextField::new(
                    "button-bar-form-start-in",
                    &form.start_in_input,
                    tr("button-bar-form-start-in"),
                ),
            ))
            .child(self.console_toggle_row(form.hidden_console, t, cx))
            .into_any_element()
    }

    fn console_toggle_row(&self, hidden: bool, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let show = editor_tab_button(
            "button-bar-form-console-show",
            tr("button-bar-form-console-show"),
            !hidden,
            t,
        )
        .on_click(cx.listener(|view, _, window, cx| {
            view.set_form_hidden_console(false, window, cx);
        }));
        let hide = editor_tab_button(
            "button-bar-form-console-hide",
            tr("button-bar-form-console-hide"),
            hidden,
            t,
        )
        .on_click(cx.listener(|view, _, window, cx| {
            view.set_form_hidden_console(true, window, cx);
        }));

        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .gap(t.spacing.md)
            .pt(t.spacing.xs)
            .child(
                div()
                    .text_size(t.typography.small)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(t.colors.muted)
                    .child(tr("button-bar-form-console")),
            )
            .child(
                div()
                    .id("button-bar-form-console-group")
                    .role(Role::RadioGroup)
                    .aria_label(tr("button-bar-form-console"))
                    .flex()
                    .items_center()
                    .gap(t.spacing.xs)
                    .child(show)
                    .child(hide),
            )
            .into_any_element()
    }
}

fn menu_name_form(
    form: &FormState,
    t: &Theme,
    pane: ObservedElement<Stateful<Div>>,
) -> ObservedElement<Stateful<Div>> {
    pane.gap(t.spacing.md)
        .child(labeled_field(
            t,
            tr("button-bar-form-name"),
            TextField::new(
                "button-bar-form-name",
                &form.name_input,
                tr("button-bar-form-name"),
            ),
        ))
        .child(muted_hint(tr("button-bar-form-menu-hint"), t))
        .when_some(form.error.as_deref(), |el, msg| {
            el.child(form_error_line(msg, t))
        })
}

fn command_list(spec: ListSpec<'_>, t: &Theme) -> AnyElement {
    let rows_render = Rc::clone(&spec.rows);
    let rows_meta = Rc::clone(&spec.rows);
    let prefix = spec.row_prefix;
    let on_pick = spec.on_pick;
    let list = VirtualList::new(
        spec.list_id,
        spec.rows.len(),
        spec.scroll,
        move |range, _, cx| {
            let t = tokens::current(cx);
            range
                .map(|idx| command_row(prefix, idx, &rows_render[idx], Rc::clone(&on_pick), t))
                .collect()
        },
    )
    .row_height(px(ROW_HEIGHT))
    .label(spec.aria_label)
    .describe_rows(move |idx| RowSemantics {
        label: Some(rows_meta[idx].label.clone().into()),
        is_selected: Some(rows_meta[idx].selected),
        is_active_descendant: false,
    });

    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(LIST_MIN_HEIGHT))
        .min_w_0()
        .rounded(t.radii.md)
        .border_1()
        .border_color(t.surfaces.card_edge)
        .bg(t.surfaces.field)
        .p(t.spacing.xs)
        .child(list)
        .into_any_element()
}

fn command_row(
    prefix: &'static str,
    index: usize,
    row: &CommandRow,
    on_pick: PickRow,
    t: &Theme,
) -> AnyElement {
    let hover = t.colors.hover;
    let secondary_color = if row.selected {
        t.colors.on_selection
    } else {
        t.colors.muted
    };
    div()
        .id((prefix, index))
        .role(Role::Button)
        .aria_label(row.label.clone())
        .aria_selected(row.selected)
        .test_support()
        .w_full()
        .h(px(ROW_HEIGHT))
        .px(t.spacing.sm)
        .rounded(t.radii.sm)
        .flex()
        .items_center()
        .justify_between()
        .gap(t.spacing.md)
        .cursor_pointer()
        .text_size(t.typography.small)
        .when(row.selected, |d| {
            d.bg(t.colors.selection).text_color(t.colors.on_selection)
        })
        .when(!row.selected, |d| {
            d.text_color(t.colors.text).hover(move |s| s.bg(hover))
        })
        .on_click(move |_, window, cx| on_pick(index, window, cx))
        .child(div().flex_1().min_w_0().truncate().child(row.label.clone()))
        .when_some(row.secondary.clone(), |el, extra| {
            el.child(div().flex_none().text_color(secondary_color).child(extra))
        })
        .into_any_element()
}

fn labeled_field(t: &Theme, label: String, field: TextField) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_none()
        .gap(t.spacing.xs)
        .child(
            div()
                .text_size(t.typography.small)
                .font_weight(FontWeight::MEDIUM)
                .text_color(t.colors.muted)
                .child(label),
        )
        .child(field)
}

fn muted_hint(text: String, t: &Theme) -> Div {
    div()
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(text)
}

fn form_error_line(message: &str, t: &Theme) -> AnyElement {
    div()
        .id("button-bar-form-error")
        .role(Role::Alert)
        .aria_label(message.to_owned())
        .test_support()
        .flex_none()
        .text_size(t.typography.small)
        .text_color(t.colors.mark)
        .child(message.to_owned())
        .into_any_element()
}
