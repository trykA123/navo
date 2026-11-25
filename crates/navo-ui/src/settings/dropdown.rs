//! Dropdown control for long or multi-option choice rows in Settings.

use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Anchor, AnyElement, ClickEvent, Entity, InteractiveElement, Role, StatefulInteractiveElement,
    Window, anchored, deferred, point,
};

use super::SheetEvent;
use super::controls::{Picks, SEGMENT_HEIGHT};
use super::fields::{Choice, Control, FIELDS, Field, Look as ChoiceLook, Section};
use super::find::{Filter, Line, lines};
use super::values::Value;
use crate::prelude::*;
use crate::primitives::popup_menu::{
    self, ChooseHandler, Keys, PopupMenu, choose, toggle, wire_anchor, wire_keys,
};
use crate::primitives::{IconName, icon};
use crate::tokens::{self, Theme};

const SEGMENT_MAX_OPTIONS: usize = 3;
const SEGMENT_MAX_CHARS: usize = 28;
const TRIGGER_MAX_WIDTH: f32 = 220.;
const MENU_MIN_WIDTH: f32 = 210.;
const ROW_HEIGHT: f32 = 30.;
const MENU_GAP: f32 = 4.;
const WINDOW_MARGIN: f32 = 8.;

/// Whether a choice row uses a dropdown instead of inline segments.
pub(super) fn uses_dropdown(options: &[Choice]) -> bool {
    let total_chars: usize = options
        .iter()
        .map(|option| option.text().chars().count())
        .sum();
    options.len() > SEGMENT_MAX_OPTIONS || total_chars > SEGMENT_MAX_CHARS
}

/// Icon shown beside each dropdown option in the trigger and menu list.
pub(super) fn choice_icon(field: Field, toml: &str) -> IconName {
    match (field, toml) {
        (Field::TerminalProvider, "system") => IconName::SquareTerminal,
        (Field::TerminalProvider, "windows-terminal") => IconName::LayoutDashboard,
        (Field::TerminalProvider, "powershell") => IconName::ChevronRight,
        (Field::TerminalProvider, "cmd") => IconName::Frame,
        (Field::TerminalProvider, "custom") => IconName::Settings2,
        (Field::Sort, "name") => IconName::FileText,
        (Field::Sort, "size") => IconName::HardDrive,
        (Field::Sort, "date") => IconName::Calendar,
        (Field::Sort, "kind") => IconName::SortAscending,
        (Field::SearchIndex, "auto") => IconName::Search,
        (Field::SearchIndex, "service") => IconName::Cpu,
        (Field::SearchIndex, "walk") => IconName::Folder,
        (Field::SearchIndex, "off") => IconName::Ban,
        (Field::GitEngine, "auto") => IconName::Globe,
        (Field::GitEngine, "gix") => IconName::Cpu,
        (Field::GitEngine, "git") => IconName::Github,
        _ => IconName::CircleCheck,
    }
}

fn popup_state_for(field: Field, window: &mut Window, cx: &mut App) -> Entity<PopupMenu> {
    popup_menu::popup_state(("settings-choice-menu", field.index()), window, cx)
}

/// Collects popup menu states for all dropdown choice rows shown on the active view.
pub(super) fn visible_menus(
    filter: &Filter,
    section: Section,
    shows_shortcuts: bool,
    window: &mut Window,
    cx: &mut App,
) -> Vec<(Field, Entity<PopupMenu>)> {
    let mut menus = Vec::new();
    for line in lines(filter, section) {
        let Line::Row(index) = line else {
            continue;
        };
        if let Some(spec) = FIELDS.get(index)
            && let Control::Choices(options, ChoiceLook::Segments) = spec.control
            && uses_dropdown(options)
        {
            menus.push((spec.field, popup_state_for(spec.field, window, cx)));
        }
    }
    if shows_shortcuts
        && let Control::Choices(options, ChoiceLook::Segments) = Field::ModifierHints.spec().control
        && uses_dropdown(options)
        && !menus
            .iter()
            .any(|(field, _)| *field == Field::ModifierHints)
    {
        menus.push((
            Field::ModifierHints,
            popup_state_for(Field::ModifierHints, window, cx),
        ));
    }
    menus
}

/// Opens a closed dropdown menu with the current option pre-selected.
pub(super) fn open_at(
    state: &Entity<PopupMenu>,
    (at, count): (usize, usize),
    window: &mut Window,
    cx: &mut App,
) {
    if state.read(cx).is_open {
        return;
    }
    popup_menu::open(state, window, cx);
    select_current(state, at, count, cx);
}

fn toggle_at(
    state: &Entity<PopupMenu>,
    (at, count): (usize, usize),
    event: &ClickEvent,
    window: &mut Window,
    cx: &mut App,
) {
    toggle(state, event, window, cx);
    if state.read(cx).is_open {
        select_current(state, at, count, cx);
    }
}

fn select_current(state: &Entity<PopupMenu>, at: usize, count: usize, cx: &mut App) {
    let picked = at.min(count.saturating_sub(1));
    state.update(cx, |menu, cx| {
        menu.selected = picked;
        cx.notify();
    });
}

/// Renders a dropdown trigger and, when open, its option menu.
pub(super) fn dropdown(
    picks: &Picks,
    state: &Entity<PopupMenu>,
    enabled: bool,
    cx: &App,
) -> AnyElement {
    let base = tokens::current(cx);
    let is_open = state.read(cx).is_open;
    let trigger = trigger_button(picks, state, (base, is_open, enabled), cx);
    let menu = (is_open && enabled).then(|| menu_popup(picks, state, base, cx));
    div()
        .relative()
        .flex_none()
        .child(trigger)
        .children(menu)
        .into_any_element()
}

fn trigger_button(
    picks: &Picks,
    state: &Entity<PopupMenu>,
    (base, is_open, enabled): (&Theme, bool, bool),
    cx: &App,
) -> AnyElement {
    let current = picks.options.get(picks.at);
    let label = current.map(Choice::text).unwrap_or_default();
    let current_icon = current.map_or(IconName::CircleCheck, |choice| {
        choice_icon(picks.field, choice.toml)
    });
    let border = if is_open {
        picks.t.colors.accent
    } else {
        base.colors.line
    };
    let hover = base.colors.hover;
    let button = div()
        .id(("settings-dropdown", picks.field.index()))
        .role(Role::Button)
        .aria_label(label.clone())
        .aria_expanded(is_open)
        .test_support();
    let button = wire_anchor(button, (state, picks.t.colors.accent), cx)
        .flex()
        .flex_none()
        .items_center()
        .gap(base.spacing.sm)
        .max_w(px(TRIGGER_MAX_WIDTH))
        .h(px(SEGMENT_HEIGHT + 4.))
        .px(base.spacing.md)
        .rounded(base.radii.md)
        .border_1()
        .border_color(border)
        .bg(base.surfaces.field)
        .text_size(base.typography.small)
        .text_color(base.colors.text)
        .when(enabled, |d| d.cursor_pointer().hover(move |d| d.bg(hover)))
        .child(icon(current_icon, base.spacing.icon, base.colors.muted))
        .child(div().min_w_0().truncate().whitespace_nowrap().child(label))
        .child(icon(
            IconName::ChevronDown,
            base.spacing.icon,
            base.colors.muted,
        ));
    if !enabled {
        return button.into_any_element();
    }
    let toggled = state.clone();
    let key_state = state.clone();
    let send = picks.send.cloned();
    let row_index = picks.field.row();
    let (at, count) = (picks.at, picks.options.len());
    button
        .on_click(move |event, window, cx| {
            cx.stop_propagation();
            if let Some(send) = &send {
                send(&SheetEvent::Select(row_index), window, cx);
            }
            toggle_at(&toggled, (at, count), event, window, cx);
        })
        .on_key_down(move |event, window, cx| {
            if matches!(event.keystroke.key.as_str(), "enter" | "space")
                && event.keystroke.modifiers.number_of_modifiers() == 0
            {
                cx.stop_propagation();
                open_at(&key_state, (at, count), window, cx);
            }
        })
        .into_any_element()
}

struct OptionProps<'a> {
    theme: &'a Theme,
    field: Field,
    index: usize,
    is_highlighted: bool,
    is_current: bool,
    state: Entity<PopupMenu>,
    on_choose: ChooseHandler,
}

fn menu_popup(picks: &Picks, state: &Entity<PopupMenu>, t: &Theme, cx: &App) -> AnyElement {
    let field = picks.field;
    let send = picks.send.cloned();
    let on_choose: ChooseHandler = Rc::new(move |index, window, cx| {
        if let Some(send) = &send {
            send(
                &SheetEvent::Set(super::values::Change {
                    field,
                    value: Value::Pick(index),
                }),
                window,
                cx,
            );
        }
    });
    let selected = state.read(cx).selected;
    let rows: Vec<AnyElement> = picks
        .options
        .iter()
        .enumerate()
        .map(|(index, option)| {
            option_row(
                option,
                OptionProps {
                    theme: t,
                    field,
                    index,
                    is_highlighted: index == selected,
                    is_current: index == picks.at,
                    state: state.clone(),
                    on_choose: on_choose.clone(),
                },
            )
        })
        .collect();
    let keys = Keys {
        state: state.clone(),
        count: picks.options.len(),
        on_choose,
    };
    let c = &t.colors;
    let panel = wire_keys(
        div().id(("settings-dropdown-menu", field.index())),
        keys,
        cx,
    )
    .role(Role::Menu)
    .aria_label(field.spec().label())
    .occlude()
    .flex()
    .flex_col()
    .min_w(px(MENU_MIN_WIDTH))
    .p(t.spacing.xs)
    .rounded(t.radii.md)
    .border_1()
    .border_color(c.line)
    .bg(c.surface)
    .shadow(t.shadows.menu.clone())
    .text_color(c.text)
    .text_size(t.typography.small)
    .children(rows);
    let placed = deferred(
        anchored()
            .anchor(Anchor::TopRight)
            .offset(point(px(0.), px(MENU_GAP)))
            .snap_to_window_with_margin(px(WINDOW_MARGIN))
            .child(panel),
    )
    .with_priority(1);
    div()
        .absolute()
        .top_full()
        .right_0()
        .child(placed)
        .into_any_element()
}

fn option_row(option: &Choice, props: OptionProps) -> AnyElement {
    let themed = props.theme.selected_content(props.is_highlighted);
    let t = &themed;
    let hover = if props.is_highlighted && t.is_contrast {
        t.colors.selection
    } else {
        t.colors.hover
    };
    let icon_color = if props.is_highlighted && t.is_contrast {
        t.colors.on_selection
    } else {
        t.colors.muted
    };
    let check_color = if props.is_highlighted && t.is_contrast {
        t.colors.on_selection
    } else {
        t.colors.accent
    };
    let (state, on_choose, index) = (props.state, props.on_choose, props.index);
    div()
        .id(("settings-dropdown-option", index))
        .role(Role::MenuItem)
        .aria_label(option.text())
        .aria_selected(props.is_highlighted)
        .test_support()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .h(px(ROW_HEIGHT))
        .px(t.spacing.md)
        .rounded(t.radii.sm)
        .cursor_pointer()
        .when(props.is_highlighted, |d| {
            d.bg(t.colors.selection).text_color(t.colors.on_selection)
        })
        .hover(move |d| d.bg(hover))
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            choose(&state, (index, &on_choose), window, cx);
        })
        .child(icon(
            choice_icon(props.field, option.toml),
            t.spacing.icon,
            icon_color,
        ))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .whitespace_nowrap()
                .child(option.text()),
        )
        .when(props.is_current, |d| {
            d.child(icon(IconName::Check, t.spacing.icon, check_color))
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_or_multi_option_choices_use_the_dropdown() {
        let expects_dropdown = |field: Field| {
            let Control::Choices(options, ChoiceLook::Segments) = field.spec().control else {
                panic!("{field:?} is not a segmented choice");
            };
            uses_dropdown(options)
        };
        assert!(expects_dropdown(Field::TerminalProvider));
        assert!(expects_dropdown(Field::Sort));
        assert!(expects_dropdown(Field::SearchIndex));
        assert!(expects_dropdown(Field::GitEngine));
    }

    #[test]
    fn short_two_or_three_option_choices_stay_segments() {
        for field in [
            Field::ModifierHints,
            Field::HiddenFiles,
            Field::SizeMode,
            Field::DropConfirm,
            Field::RightButton,
            Field::CopyMode,
            Field::CopyVerify,
            Field::TextSyntaxTheme,
            Field::AutomotivePreset,
            Field::ClipboardPosition,
            Field::CompareGitEngine,
            Field::CompareContentCheck,
        ] {
            let Control::Choices(options, ChoiceLook::Segments) = field.spec().control else {
                panic!("{field:?} is not a segmented choice");
            };
            assert!(!uses_dropdown(options), "{field:?} should stay segments");
        }
    }
}
