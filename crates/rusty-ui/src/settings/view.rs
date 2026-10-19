use gpui_kit::base::TestSupportExt;
use gpui_kit::{Role, ScrollHandle};

use super::controls::{self, Picks};
use super::fields::{Control, FieldSpec, Look as ChoiceLook, Section};
use super::row_state;
use super::steps::{first_row, rows_in};
use super::values::{Value, Values};
use super::{Handler, SheetEvent};
use crate::prelude::*;
use crate::primitives::{IconName, icon};
use crate::tokens::Theme;

const NAV_WIDTH: f32 = 148.;
const NAV_ITEM: f32 = 30.;
const MARKER: (f32, f32) = (3., 14.);
const ROW_MIN: f32 = 48.;
const NOT_APPLIED: &str = "Not applied yet";
const SHORTCUTS: &str = "Shortcuts";

pub(super) struct Look<'a> {
    pub theme: &'a Theme,
    pub values: &'a Values,
    pub row: usize,
    pub send: Option<Handler>,
}

fn send_on_click<E: StatefulInteractiveElement>(
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

struct NavItem {
    index: usize,
    label: &'static str,
    glyph: IconName,
    is_on: bool,
    event: SheetEvent,
}

fn nav_item(look: &Look, item: NavItem) -> AnyElement {
    let t = look.theme;
    let marker = div()
        .absolute()
        .left_0()
        .top((px(NAV_ITEM) - px(MARKER.1)) / 2.)
        .w(px(MARKER.0))
        .h(px(MARKER.1))
        .rounded(t.radii.full)
        .bg(t.colors.accent);
    let tint = if item.is_on {
        t.colors.accent_text
    } else {
        t.colors.muted
    };
    let element = div()
        .id(("settings-section", item.index))
        .role(Role::Tab)
        .aria_label(item.label)
        .aria_selected(item.is_on)
        .test_support()
        .relative()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .h(px(NAV_ITEM))
        .px(t.spacing.lg)
        .rounded(t.radii.md)
        .cursor_pointer()
        .whitespace_nowrap()
        .hover(|element| element.bg(t.colors.hover))
        .when(item.is_on, |element| {
            element
                .bg(t.colors.selection)
                .font_weight(FontWeight::SEMIBOLD)
                .child(marker)
        })
        .child(icon(item.glyph, t.spacing.icon, tint))
        .child(item.label);
    send_on_click(element, look.send.as_ref(), item.event).into_any_element()
}

pub(super) fn nav(look: &Look, current: Option<Section>) -> impl IntoElement + use<> {
    let t = look.theme;
    let sections = Section::ALL
        .iter()
        .enumerate()
        .map(|(index, section)| NavItem {
            index,
            label: section.label(),
            glyph: section.icon(),
            is_on: current == Some(*section),
            event: SheetEvent::Select(first_row(*section)),
        });
    let shortcuts = NavItem {
        index: Section::ALL.len(),
        label: SHORTCUTS,
        glyph: IconName::Keyboard,
        is_on: current.is_none(),
        event: SheetEvent::Shortcuts,
    };
    let items = sections.chain([shortcuts]).map(|item| nav_item(look, item));
    div()
        .id("settings-nav")
        .role(Role::TabList)
        .aria_label("Sections")
        .test_support()
        .flex()
        .flex_col()
        .flex_none()
        .gap(t.spacing.xxs)
        .w(px(NAV_WIDTH))
        .p(t.spacing.md)
        .border_r_1()
        .border_color(t.surfaces.card_edge)
        .children(items)
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
            .map_or_else(SharedString::default, |option| option.label.into()),
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
            Some(controls::value_box(t, spoken(spec, value)))
        }
        _ => None,
    }
}

fn pictures(look: &Look, spec: &FieldSpec) -> Option<AnyElement> {
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

fn title(look: &Look, index: usize, spec: &FieldSpec) -> Div {
    let t = look.theme;
    let pending = (!spec.field.is_applied()).then(|| {
        div()
            .id("settings-not-applied")
            .role(Role::Note)
            .aria_label(NOT_APPLIED)
            .test_support()
            .text_size(t.typography.small)
            .text_color(t.colors.muted)
            .child(NOT_APPLIED)
    });
    let name = div()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .child(div().font_weight(FontWeight::MEDIUM).child(spec.label))
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
                .child(spec.help),
        )
        .children(pending)
        .child(key_line(t, spec, index == look.row))
}

fn row(look: &Look, index: usize, spec: &FieldSpec) -> AnyElement {
    let t = look.theme;
    let is_current = index == look.row;
    let value = look.values.get(spec.field);
    let reset = look
        .values
        .is_changed(spec.field)
        .then(|| row_state::reset(index, spec.field, look.send.as_ref()));
    let line = div()
        .flex()
        .items_center()
        .gap(t.spacing.xl)
        .child(title(look, index, spec))
        .children(reset)
        .children(control(look, spec));
    let row = div()
        .id(("settings-row", index))
        .role(Role::ListItem)
        .aria_label(spec.label)
        .aria_value(spoken(spec, value))
        .aria_selected(is_current)
        .test_support()
        .flex()
        .flex_col()
        .flex_none()
        .min_h(px(ROW_MIN))
        .px(t.spacing.xl)
        .py(t.spacing.md)
        .rounded(t.radii.md)
        .border_1()
        .border_color(if is_current {
            t.colors.accent
        } else {
            gpui_kit::transparent_black()
        })
        .when(is_current, |row| row.bg(t.colors.selection))
        .child(line)
        .children(pictures(look, spec));
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

pub(super) fn content(
    look: &Look,
    section: Section,
    scroll: Option<&ScrollHandle>,
) -> impl IntoElement + use<> {
    let t = look.theme;
    let heading = div()
        .flex_none()
        .px(t.spacing.xl)
        .pt(t.spacing.lg)
        .pb(t.spacing.xs)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .font_weight(FontWeight::SEMIBOLD)
        .child(section.label());
    let mut group = None;
    let mut rows: Vec<AnyElement> = Vec::new();
    for (index, spec) in rows_in(section) {
        if section == Section::Display && spec.group() != group {
            group = spec.group();
            rows.extend(group.map(|label| group_heading(t, label)));
        }
        rows.push(row(look, index, spec));
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
        .child(heading)
        .children(rows)
}
