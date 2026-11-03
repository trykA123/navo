use gpui_kit::base::TestSupportExt;
use gpui_kit::{Role, ScrollHandle};

use super::controls::{self, Picks};
use super::fields::{Control, FIELDS, Field, FieldSpec, Look as ChoiceLook, Section};
use super::find::{Filter, Line, lines};
use super::git_page::{self, GitPage};
use super::index_page;
use super::row_state;
use super::search_page::{self, SearchPage};
use super::values::{Value, Values};
use super::{Handler, SheetEvent};
use crate::prelude::*;
use crate::tokens::Theme;
use crate::window_view::UpdateView;

const ROW_MIN: f32 = 48.;
const NO_MATCH: &str = "No setting matches";
pub(super) const DIMMED: f32 = 0.45;

pub(super) struct Look<'a> {
    pub theme: &'a Theme,
    pub values: &'a Values,
    pub row: usize,
    pub filter: &'a Filter,
    pub diagnostics: &'a [super::DiagnosticRow],
    pub version: &'a SharedString,
    pub update: &'a UpdateView,
    pub page: &'a SearchPage,
    pub git: &'a GitPage,
    pub index: &'a super::IndexProgress,
    pub send: Option<Handler>,
    pub summon_key_taken: bool,
    pub clipboard_key_taken: bool,
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

fn spoken(spec: &FieldSpec, value: &Value) -> SharedString {
    match (spec.control, value) {
        (Control::Choices(options, _), Value::Pick(at)) => options
            .get(*at)
            .map_or_else(SharedString::default, |option| option.text()),
        (_, Value::Flag(on)) => if *on { "On" } else { "Off" }.into(),
        (_, Value::Text(text)) => text.clone(),
        (_, Value::Number(number)) => number.to_string().into(),
        _ => SharedString::default(),
    }
}

fn control(look: &Look, spec: &FieldSpec) -> Option<AnyElement> {
    let t = look.theme;
    let value = look.values.get(spec.field);
    let send = look.send.as_ref();
    match (spec.control, value) {
        (Control::Choices(options, ChoiceLook::Segments), Value::Pick(at)) => {
            let picks = Picks {
                t,
                field: spec.field,
                options,
                at: *at,
                send,
            };
            Some(controls::segments(&picks).into_any_element())
        }
        (Control::Toggle, Value::Flag(on)) => Some(controls::toggle(t, *on, send, spec.field)),
        (Control::Text | Control::Number, value) => {
            Some(controls::value_box(t, &spoken(spec, value)))
        }
        _ => None,
    }
}

fn pictures(look: &Look, spec: &FieldSpec) -> Option<AnyElement> {
    if spec.control == Control::Tool {
        let current = look.values.git_tool();
        return Some(git_page::tool_list(
            look.theme,
            look.git,
            current,
            look.send.as_ref(),
        ));
    }
    let (Control::Choices(options, choice_look), Value::Pick(at)) =
        (spec.control, look.values.get(spec.field))
    else {
        return None;
    };
    if choice_look == ChoiceLook::Segments {
        return None;
    }
    let picks = Picks {
        t: look.theme,
        field: spec.field,
        options,
        at: *at,
        send: look.send.as_ref(),
    };
    Some(controls::pictures(&picks, choice_look).into_any_element())
}

fn key_line(t: &Theme, spec: &FieldSpec, is_current: bool) -> impl IntoElement + use<> {
    let line = div()
        .font_family(t.typography.mono_font.clone())
        .text_size(t.typography.small)
        .child(spec.key);
    if !is_current {
        return line
            .text_color(gpui_kit::transparent_black())
            .into_any_element();
    }
    line.id("settings-key")
        .role(Role::Label)
        .aria_label(spec.key)
        .test_support()
        .text_color(t.colors.accent_text)
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

fn title(look: &Look, index: usize, spec: &FieldSpec) -> Div {
    let t = look.theme;
    let name = div()
        .flex()
        .items_baseline()
        .gap(t.spacing.md)
        .child(div().font_weight(FontWeight::MEDIUM).child(spec.label()))
        .child(row_state::tag(t, index, look.values.is_changed(spec.field)));
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .child(name)
        .child(
            div()
                .text_size(t.typography.small)
                .text_color(t.colors.muted)
                .child(spec.help()),
        )
        .children(row_state::notes(
            t,
            spec.field,
            (look.summon_key_taken, look.clipboard_key_taken),
        ))
        .children(command_problem(look, spec.field))
        .child(key_line(t, spec, index == look.row))
}

pub(super) fn row(look: &Look, index: usize, spec: &FieldSpec) -> AnyElement {
    let t = look.theme;
    let is_current = index == look.row;
    let theme = t.selected_content(is_current);
    let content_look = Look {
        theme: &theme,
        send: look.send.clone(),
        ..*look
    };
    let value = look.values.get(spec.field);
    let reset = look
        .values
        .is_changed(spec.field)
        .then(|| row_state::reset(index, spec.field, look.send.as_ref()));
    let line = div()
        .flex()
        .items_center()
        .gap(t.spacing.xl)
        .child(title(&content_look, index, spec))
        .children(reset)
        .children(control(look, spec));
    let row = div()
        .id(("settings-row", index))
        .role(Role::ListItem)
        .aria_label(spec.label())
        .aria_value(spoken(spec, value))
        .aria_selected(is_current)
        .test_support()
        .flex()
        .flex_col()
        .flex_none()
        .min_h(px(ROW_MIN))
        .when(!look.values.is_enabled(spec.field), |row| {
            row.opacity(DIMMED)
        })
        .px(t.spacing.xl)
        .py(t.spacing.md)
        .rounded(t.radii.md)
        .border_1()
        .border_color(if is_current {
            t.colors.accent
        } else {
            gpui_kit::transparent_black()
        })
        .when(is_current, |row| {
            row.bg(t.colors.selection).text_color(t.colors.on_selection)
        })
        .child(line)
        .children(pictures(&content_look, spec));
    select_on_click(row, look.send.as_ref(), index).into_any_element()
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
        .px(t.spacing.xl)
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
        .px(t.spacing.xl)
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

pub(super) fn content(
    look: &Look,
    section: Section,
    scroll: Option<&ScrollHandle>,
) -> impl IntoElement + use<> {
    let t = look.theme;
    let send = look.send.as_ref();
    let mut rows: Vec<AnyElement> = lines(look.filter, section)
        .into_iter()
        .flat_map(|line| match line {
            Line::Section(section) => vec![section_heading(t, section)],
            Line::Group(label) => vec![group_heading(t, label)],
            Line::Row(index) => FIELDS
                .get(index)
                .map(|spec| row(look, index, spec))
                .into_iter()
                .collect(),
            Line::Progress => index_page::progress_block(t, look.index),
            Line::Service => search_page::service_block(t, *look.page, send),
            Line::History => search_page::history_block(t, *look.page, send),
        })
        .collect();
    if is_page_shown(look, section, Section::Diagnostics) {
        rows.extend(super::diagnostics_page::render(look));
    }
    if is_page_shown(look, section, Section::Help) {
        rows.extend(super::help_page::render(look));
    }
    if rows.is_empty() {
        rows.push(no_match(t));
    }
    div()
        .id("settings-rows")
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .gap(t.spacing.xxs)
        .px(t.spacing.md)
        .pb(t.spacing.md)
        .overflow_y_scroll()
        .when_some(scroll, |list, scroll| list.track_scroll(scroll))
        .children(rows)
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
