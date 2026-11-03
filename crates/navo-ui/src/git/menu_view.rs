use gpui_kit::base::animation::ease_out_cubic;
use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Animation, AnimationExt, AnyElement, BoxShadow, Div, Entity, FontWeight, InteractiveElement,
    IntoElement, ParentElement, Role, SharedString, Stateful, StatefulInteractiveElement, Styled,
    div, point, px,
};
use navo_l10n::{t, tr};

use super::menu::{Head, Model, Row, Via};
use crate::file_list::git_cell::{Mark, mark};
use crate::primitives::popup_menu::{ChooseHandler, PopupMenu, choose};
use crate::primitives::{Kbd, icon};
use crate::tokens::Theme;

pub const WIDTH: f32 = 340.;
const PAD: f32 = 4.;
const ROW: f32 = 28.;
const TITLE_LINE: f32 = 22.;
const ICON_CELL: f32 = 20.;
const FOOTER: f32 = 30.;

/// How one frame of the menu looks: the theme, the selected row and the shown key.
pub struct Look<'a> {
    pub theme: &'a Theme,
    pub selected: Option<usize>,
    pub hint: Option<&'a str>,
}

pub(crate) struct Live {
    pub state: Entity<PopupMenu>,
    pub on_choose: ChooseHandler,
}

fn shadow(t: &Theme) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: t.shadows.ink.opacity(0.16),
        offset: point(px(0.), px(4.)),
        blur_radius: px(14.),
        spread_radius: px(0.),
        inset: false,
    }]
}

fn glyph(t: &Theme, model: &Model) -> AnyElement {
    let cell = div()
        .w(px(ICON_CELL))
        .h(px(TITLE_LINE))
        .flex_none()
        .flex()
        .items_center()
        .justify_center();
    match mark(t, &model.badge) {
        Mark::Glyph(name, color) => cell.child(icon(name, t.spacing.icon - px(1.), color)),
        Mark::Dot(color) => cell.child(div().size(px(4.)).rounded(t.radii.full).bg(color)),
    }
    .into_any_element()
}

fn title_line(t: &Theme, head: &Head) -> Div {
    div()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .h(px(TITLE_LINE))
        .min_w_0()
        .child(
            div()
                .truncate()
                .font_weight(FontWeight::BOLD)
                .child(head.title.clone()),
        )
        .children(head.place.clone().map(|place| {
            div()
                .truncate()
                .min_w_0()
                .font_family(t.typography.mono_font.clone())
                .text_size(t.typography.small)
                .text_color(t.colors.muted)
                .child(place)
        }))
}

fn facts_line(t: &Theme, head: &Head) -> Div {
    div()
        .flex()
        .flex_wrap()
        .gap_x(t.spacing.md)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(t.colors.text)
                .child(head.state.clone()),
        )
        .children(head.facts.iter().cloned().map(|fact| div().child(fact)))
}

fn head_block(t: &Theme, model: &Model) -> Div {
    div()
        .flex()
        .items_start()
        .gap(t.spacing.md)
        .px(t.spacing.sm)
        .py(t.spacing.md)
        .mb(px(PAD))
        .border_b_1()
        .border_color(t.colors.line)
        .child(glyph(t, model))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(title_line(t, &model.head))
                .child(facts_line(t, &model.head)),
        )
}

fn row_label(row: &Row) -> SharedString {
    match row.note.is_empty() {
        true => row.label.clone().into(),
        false => format!("{}, {}", row.label, row.note).into(),
    }
}

const SETTLE_FROM: f32 = 0.4;

/// A row whose confirmation just arrived fades from dim to full; nothing moves.
fn settling(cell: Div, id: &'static str, (t, index, row): (&Theme, usize, &Row)) -> AnyElement {
    if !row.settled {
        return cell.into_any_element();
    }
    let fade = Animation::new(t.motion.fade).with_easing(ease_out_cubic);
    cell.with_animation((id, index), fade, |cell, delta| {
        cell.opacity(SETTLE_FROM + (1. - SETTLE_FROM) * delta)
    })
    .into_any_element()
}

fn icon_cell(t: &Theme, row: &Row) -> Div {
    let color = if row.enabled {
        t.colors.accent
    } else {
        t.colors.faint
    };
    div()
        .w(px(ICON_CELL))
        .flex_none()
        .flex()
        .justify_center()
        .child(icon(row.icon, t.spacing.icon, color))
}

fn name_cell(row: &Row) -> Div {
    div()
        .flex_none()
        .when(row.lead, |d| d.font_weight(FontWeight::BOLD))
        .child(row.label.clone())
}

fn note_cell(t: &Theme, row: &Row) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .truncate()
        .text_right()
        .pl(t.spacing.lg)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(row.note.clone())
}

fn row_element(
    (index, row): (usize, &Row),
    look: &Look,
    live: Option<&Live>,
) -> ObservedElement<Stateful<Div>> {
    let is_selected = look.selected == Some(index);
    let themed = look.theme.selected_content(is_selected);
    let t: &Theme = &themed;
    let ink = if row.enabled {
        t.colors.text
    } else {
        t.colors.muted
    };
    let hover = t.colors.hover;
    div()
        .id(("git-menu-row", index))
        .role(Role::MenuItem)
        .aria_label(row_label(row))
        .aria_selected(is_selected)
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.md)
        .h(px(ROW))
        .px(t.spacing.sm)
        .rounded(t.radii.md)
        .text_color(ink)
        .when(is_selected, |d| d.bg(t.colors.selection))
        .when(row.enabled && !is_selected, |d| d.hover(|s| s.bg(hover)))
        .when(row.enabled, |d| d.cursor_pointer())
        .child(settling(
            icon_cell(t, row),
            "git-settle-icon",
            (t, index, row),
        ))
        .child(settling(name_cell(row), "git-settle-name", (t, index, row)))
        .child(note_cell(t, row))
        .when_some(
            live.map(|l| (l.state.clone(), l.on_choose.clone())),
            |d, (state, on)| {
                d.on_click(move |_, window, cx| choose(&state, (index, &on), window, cx))
            },
        )
}

fn via_text(t: &Theme, via: &Via) -> Option<AnyElement> {
    match via {
        Via::Tool(tool) => Some(
            div()
                .flex()
                .gap(t.spacing.xs)
                .child(tr("git-menu-via"))
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(t.colors.text)
                        .child(tool.clone()),
                )
                .into_any_element(),
        ),
        Via::NoTool => Some(div().child(tr("git-menu-no-tool")).into_any_element()),
        Via::Unnamed => None,
    }
}

fn footer(t: &Theme, model: &Model, hint: Option<&str>) -> Div {
    let key = hint.and_then(|keys| Kbd::new("git-menu-key", keys));
    div()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .min_h(px(FOOTER))
        .mt(px(PAD))
        .px(t.spacing.sm)
        .pt(px(PAD))
        .border_t_1()
        .border_color(t.colors.line)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .whitespace_nowrap()
        .children(via_text(t, &model.via))
        .child(
            div()
                .ml_auto()
                .flex()
                .items_center()
                .gap(t.spacing.xs)
                .children(key)
                .child(tr("git-menu-rebindable")),
        )
}

fn panel(model: &Model, look: &Look, live: Option<&Live>) -> Stateful<Div> {
    let t = look.theme;
    let rows = model
        .rows
        .iter()
        .enumerate()
        .map(|item| row_element(item, look, live));
    div()
        .id("git-menu")
        .role(Role::Menu)
        .aria_label(t!("git-menu-aria", name = model.head.title.as_str()))
        .occlude()
        .flex()
        .flex_col()
        .w(px(WIDTH))
        .max_w_full()
        .p(px(PAD))
        .rounded(t.radii.lg)
        .border_1()
        .border_color(t.colors.line)
        .bg(t.colors.surface)
        .when(!t.is_contrast, |d| d.shadow(shadow(t)))
        .text_color(t.colors.text)
        .text_size(t.typography.body)
        .child(head_block(t, model))
        .children(rows)
        .child(footer(t, model, look.hint))
}

/// The menu as it looks, with no behaviour: for the gallery.
pub fn preview(model: &Model, look: &Look) -> Stateful<Div> {
    panel(model, look, None)
}

pub(crate) fn live(model: &Model, look: &Look, live: &Live) -> Stateful<Div> {
    panel(model, look, Some(live))
}
