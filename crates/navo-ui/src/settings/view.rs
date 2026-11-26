//! Row, control and status-line rendering for the settings sheet.

use gpui_kit::base::TestSupportExt;
use gpui_kit::{Entity, HighlightStyle, Role, ScrollHandle, StyledText};

use super::controls::{self, Picks};
use super::dropdown;
use super::fields::{Control, FIELDS, Field, FieldSpec, Look as ChoiceLook, Section};
use super::find::{Filter, Line, lines};
use super::git_page::{self, GitPage};
use super::index_page;
use super::inspector;
use super::nested;
use super::row_state;
use super::search_page::{self, SearchPage};
use super::values::{Value, Values};
use super::{Handler, SheetEvent};
use crate::commands::{CommandSpec, Keymap};
use crate::prelude::*;
use crate::primitives::Kbd;
use crate::primitives::popup_menu::PopupMenu;
use crate::shortcuts;
use crate::tokens::Theme;
use crate::window_view::UpdateView;

/// Height of a setting row in px (README `SET_ROW`).
const ROW: f32 = 36.;
/// The label asks for this much (px).
const LABEL_BASIS: f32 = 150.;
/// The label shrinks to this width (px) with an ellipsis before the row runs out of room.
const LABEL_MIN: f32 = 112.;
const NO_MATCH: &str = "No setting matches";
pub(super) const DIMMED: f32 = 0.45;

pub(super) struct Look<'a> {
    pub theme: &'a Theme,
    pub values: &'a Values,
    pub row: usize,
    pub filter: &'a Filter,
    pub diagnostics: &'a [super::DiagnosticRow],
    pub diagnostics_open: bool,
    pub selected_diagnostic: Option<usize>,
    pub version: &'a SharedString,
    pub update: &'a UpdateView,
    pub restart_offered: bool,
    pub rollback: &'a super::RollbackState,
    pub page: &'a SearchPage,
    pub git: &'a GitPage,
    pub index: &'a super::IndexProgress,
    pub keymap: &'a Keymap,
    pub send: Option<Handler>,
    pub summon_key_taken: bool,
    pub clipboard_key_taken: bool,
    pub menus: &'a [(Field, Entity<PopupMenu>)],
}

/// False for a row whose controlling setting is off, or the update check on a copy that
/// cannot update.
pub(super) fn is_row_enabled(values: &Values, update: &UpdateView, field: Field) -> bool {
    values.is_enabled(field) && !(field == Field::CheckUpdates && *update == UpdateView::Hidden)
}

impl Look<'_> {
    pub(super) fn is_enabled(&self, field: Field) -> bool {
        is_row_enabled(self.values, self.update, field)
    }

    pub(super) fn menu(&self, field: Field) -> Option<&Entity<PopupMenu>> {
        self.menus
            .iter()
            .find(|(item, _)| *item == field)
            .map(|(_, menu)| menu)
    }
}

pub(super) fn send_on_click<E: StatefulInteractiveElement>(
    element: E,
    send: Option<&Handler>,
    event: SheetEvent,
) -> E {
    let Some(send) = send.cloned() else {
        return element;
    };
    element.on_click(move |_, window, cx| send(&event, window, cx))
}

fn select_on_click<E: StatefulInteractiveElement>(
    element: E,
    send: Option<&Handler>,
    row: usize,
) -> E {
    send_on_click(element, send, SheetEvent::Select(row))
}

pub(super) fn error_line(t: &Theme, message: SharedString) -> impl IntoElement {
    div()
        .id("settings-error")
        .role(Role::Alert)
        .aria_label(message.clone())
        .test_support()
        .flex_none()
        .mx(t.spacing.xl)
        .mt(t.spacing.md)
        .text_size(t.typography.small)
        .text_color(t.colors.mark)
        .child(message)
}

/// A neutral notice line in the sheet: muted text, not the error alert slot.
pub(super) fn notice_line(t: &Theme, message: SharedString) -> impl IntoElement {
    div()
        .id("settings-notice")
        .role(Role::Status)
        .aria_label(message.clone())
        .test_support()
        .flex_none()
        .mx(t.spacing.xl)
        .mt(t.spacing.md)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(message)
}

pub(super) fn spoken(spec: &FieldSpec, value: &Value) -> SharedString {
    match (spec.control, value) {
        (Control::Choices(options, _), Value::Pick(at)) => options
            .get(*at)
            .map_or_else(SharedString::default, |option| option.text()),
        (_, Value::Flag(on)) => navo_l10n::tr(if *on {
            "settings-value-on"
        } else {
            "settings-value-off"
        })
        .into(),
        (_, Value::Text(text)) => text.clone(),
        (_, Value::Number(number)) => number.to_string().into(),
        _ => SharedString::default(),
    }
}

/// The row's control. Picture choices show segments (or the colour theme's name) here; their
/// pictures and the Git tool list live in the inspector.
fn control(look: &Look, spec: &FieldSpec, cx: &App) -> Option<AnyElement> {
    let t = look.theme;
    let value = look.values.get(spec.field);
    let send = look.send.as_ref();
    match (spec.control, value) {
        (Control::Choices(options, ChoiceLook::ColorPictures), Value::Pick(at)) => {
            let name = options.get(*at).map(super::fields::Choice::text);
            Some(quiet_value(t, name.unwrap_or_default()))
        }
        (Control::Choices(options, _), Value::Pick(at)) => {
            let picks = Picks {
                t,
                field: spec.field,
                options,
                at: *at,
                send,
            };
            if let Some(menu) = look.menu(spec.field) {
                Some(dropdown::dropdown(
                    &picks,
                    menu,
                    look.is_enabled(spec.field),
                    cx,
                ))
            } else {
                Some(controls::segments(&picks).into_any_element())
            }
        }
        (Control::Toggle, Value::Flag(on)) => {
            let send = send.filter(|_| look.is_enabled(spec.field));
            Some(controls::toggle(t, *on, send, spec.field))
        }
        (Control::Text | Control::Number, value) => {
            Some(controls::value_box(t, &spoken(spec, value)))
        }
        (Control::Tool, _) => {
            let current = look.values.git_tool();
            let name = look
                .git
                .options(current)
                .into_iter()
                .find(|tool| tool.id == current);
            Some(quiet_value(
                t,
                name.map(|tool| tool.label).unwrap_or_default().into(),
            ))
        }
        _ => None,
    }
}

/// A value shown as muted text; it changes with the arrows or in the inspector.
fn quiet_value(t: &Theme, text: SharedString) -> AnyElement {
    div()
        .flex_none()
        .whitespace_nowrap()
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(text)
        .into_any_element()
}

fn command_problem(look: &Look, field: Field) -> Option<AnyElement> {
    let problem = look
        .git
        .problem
        .as_ref()
        .filter(|_| field == Field::GitToolCommand)?;
    Some(git_page::problem_note(look.theme, problem))
}

/// Draws a setting label, highlighting matching Find words when the filter is active.
fn row_label(t: &Theme, filter: &Filter, text: SharedString) -> AnyElement {
    if filter.is_empty() {
        return text.into_any_element();
    }
    let found = HighlightStyle {
        color: Some(t.colors.accent_text),
        font_weight: Some(FontWeight::MEDIUM),
        ..HighlightStyle::default()
    };
    let runs: Vec<_> = filter
        .hits(&text)
        .into_iter()
        .map(|range| (range, found))
        .collect();
    StyledText::new(text)
        .with_highlights(runs)
        .into_any_element()
}

/// One line: the changed dot, the label and the control. On a narrow list the label shrinks
/// with an ellipsis; the control keeps its width and never wraps.
fn line(look: &Look, content_look: &Look, (index, spec): (usize, &FieldSpec), cx: &App) -> Div {
    let t = look.theme;
    let label = div()
        .id(("settings-label", index))
        .test_support()
        .flex_grow(1.)
        .flex_shrink(1.)
        .flex_basis(px(LABEL_BASIS))
        .min_w(px(LABEL_MIN))
        .truncate()
        .child(row_label(content_look.theme, look.filter, spec.label()));
    div()
        .flex()
        .items_center()
        .gap_x(t.spacing.xl)
        .child(row_state::gutter(
            t,
            index,
            look.values.is_changed(spec.field),
        ))
        .child(label)
        .children(
            (!look.filter.is_empty() && spec.field.is_advanced())
                .then(|| super::fold::advanced_tag(content_look.theme, index)),
        )
        .children(control(content_look, spec, cx).map(|control| div().flex_none().child(control)))
}

/// A setting row: one 36 px line, plus a red note under it while its value is refused.
pub(super) fn row(look: &Look, index: usize, spec: &FieldSpec, cx: &App) -> AnyElement {
    let t = look.theme;
    let is_current = index == look.row;
    let theme = t.selected_content(is_current);
    let content_look = Look {
        theme: &theme,
        send: look.send.clone(),
        ..*look
    };
    let value = look.values.get(spec.field);
    let notes: Vec<AnyElement> = row_state::notes(
        t,
        spec.field,
        (look.summon_key_taken, look.clipboard_key_taken),
    )
    .chain(command_problem(look, spec.field))
    .collect();
    let row = row_frame(t, is_current)
        .id(("settings-row", index))
        .role(Role::ListItem)
        .aria_label(spec.label())
        .aria_value(spoken(spec, value))
        .aria_selected(is_current)
        .test_support()
        .flex()
        .flex_col()
        .justify_center()
        .when(!look.is_enabled(spec.field), |row| row.opacity(DIMMED))
        .child(line(look, &content_look, (index, spec), cx))
        .children(under_line(t, notes));
    let is_nested = spec.field.is_nested() && look.filter.is_empty();
    let row = nested::indented(t, row, is_nested);
    select_on_click(row, look.send.as_ref(), index).into_any_element()
}

/// The 36 px frame a setting row and a fold row share: the accent ring and fill while
/// current, a hover tint otherwise.
pub(super) fn row_frame(t: &Theme, is_current: bool) -> Div {
    div()
        .flex_none()
        .min_h(px(ROW))
        .pr(t.spacing.md)
        .rounded(t.radii.md)
        .border_1()
        .border_color(t.colors.accent.opacity(if is_current { 1. } else { 0. }))
        .when(is_current, |row| {
            row.bg(t.colors.selection).text_color(t.colors.on_selection)
        })
        .when(!is_current, |row| row.hover(|row| row.bg(t.colors.hover)))
}

/// The red notes under a row's line, indented past the gutter; none when `notes` is empty.
fn under_line(t: &Theme, notes: Vec<AnyElement>) -> Option<Div> {
    (!notes.is_empty()).then(|| {
        div()
            .pl(px(row_state::GUTTER) + t.spacing.xl)
            .pb(t.spacing.sm)
            .children(notes)
    })
}

fn group_heading(t: &Theme, label: &'static str) -> AnyElement {
    div()
        .id(SharedString::from(format!(
            "settings-group-{}",
            label.to_lowercase()
        )))
        .role(Role::Heading)
        .aria_label(label)
        .test_support()
        .flex_none()
        .pl(px(row_state::GUTTER) + t.spacing.xl)
        .pt(t.spacing.md)
        .text_size(t.typography.tiny)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(t.colors.muted)
        .child(label)
        .into_any_element()
}

pub(super) fn section_heading(t: &Theme, section: Section) -> AnyElement {
    div()
        .flex_none()
        .pl(px(row_state::GUTTER) + t.spacing.xl)
        .pt(t.spacing.lg)
        .pb(t.spacing.xs)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .font_weight(FontWeight::SEMIBOLD)
        .child(section_label(section))
        .into_any_element()
}

fn no_match(t: &Theme) -> AnyElement {
    div()
        .id("settings-no-match")
        .role(Role::Status)
        .aria_label(NO_MATCH)
        .test_support()
        .flex_none()
        .px(t.spacing.xl)
        .py(t.spacing.lg)
        .text_color(t.colors.muted)
        .child(NO_MATCH)
        .into_any_element()
}

fn shortcut_line(look: &Look, rows: &[shortcuts::Row], spec: CommandSpec) -> AnyElement {
    let t = look.theme;
    let title: SharedString = navo_l10n::tr(spec.label).into();
    let kbd = rows
        .iter()
        .find(|row| row.spec.name == spec.name)
        .and_then(|row| row.keys.first())
        .and_then(|key| {
            Kbd::new(
                ElementId::Name(format!("settings-shortcut-key-{}", spec.name).into()),
                key,
            )
        });
    let line = row_frame(t, false)
        .id(ElementId::Name(
            format!("settings-shortcut-hit-{}", spec.name).into(),
        ))
        .role(Role::Button)
        .aria_label(title.clone())
        .test_support()
        .focusable()
        .cursor_pointer()
        .focus_visible(|row| row.border_color(t.colors.accent))
        .flex()
        .items_center()
        .gap_x(t.spacing.xl)
        .child(div().flex_none().w(px(row_state::GUTTER)))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .child(row_label(t, look.filter, title)),
        )
        .children(kbd);
    send_on_click(line, look.send.as_ref(), SheetEvent::Shortcut(spec.name)).into_any_element()
}

pub(super) fn content(
    look: &Look,
    section: Section,
    scroll: Option<&ScrollHandle>,
    cx: &App,
) -> impl IntoElement + use<> {
    let t = look.theme;
    let send = look.send.as_ref();
    let shown_lines = lines(look.filter, section);
    let shortcut_rows = if shown_lines
        .iter()
        .any(|line| matches!(line, Line::Shortcut(_)))
    {
        shortcuts::rows(look.keymap)
    } else {
        Vec::new()
    };
    let mut rows: Vec<AnyElement> = shown_lines
        .into_iter()
        .flat_map(|line| match line {
            Line::Section(section) => vec![section_heading(t, section)],
            Line::Group(label) => vec![group_heading(t, label)],
            Line::Row(index) => FIELDS
                .get(index)
                .map(|spec| row(look, index, spec, cx))
                .into_iter()
                .collect(),
            Line::Fold(fold) => vec![super::fold::fold_line(look, fold)],
            Line::Progress => index_page::progress_block(t, look.index),
            Line::Service => search_page::service_block(t, *look.page, send),
            Line::History => search_page::history_block(t, *look.page, send),
            Line::Shortcut(spec) => vec![shortcut_line(look, &shortcut_rows, spec)],
        })
        .collect();
    if is_page_shown(look, section, Section::Help) {
        rows.extend(super::help_page::render(look, cx));
    }
    if is_page_shown(look, section, Section::Diagnostics) {
        rows.extend(super::diagnostics_page::render(look));
    }
    if rows.is_empty() {
        rows.push(no_match(t));
    }
    let list = div()
        .id("settings-rows")
        .test_support()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .gap(t.spacing.xxs)
        .px(t.spacing.md)
        .pb(t.spacing.md)
        .overflow_y_scroll()
        .when_some(scroll, |list, scroll| list.track_scroll(scroll))
        .children(rows);
    inspector::Split {
        rows: list.into_any_element(),
        inspector: inspector::details(look),
    }
}

fn is_page_shown(look: &Look, section: Section, page: Section) -> bool {
    section == page || (!look.filter.is_empty() && look.filter.first_in(page).is_some())
}

pub(super) fn section_label(section: Section) -> SharedString {
    if section.is_translated() {
        navo_l10n::tr(section.label()).into()
    } else {
        section.label().into()
    }
}
