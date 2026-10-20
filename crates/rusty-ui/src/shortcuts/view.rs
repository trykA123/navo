use std::ops::Range;
use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{Focusable, Role, WeakEntity};
use rusty_l10n::tr;

use super::model::{Line, Row, area_label};
use super::notice::{banner, footer};
use super::page::{ShortcutsPage, Stage};
use super::{CONTEXT, Cancel, NextRow, PreviousRow, Record, Reset};
use crate::commands::shown;
use crate::prelude::*;
use crate::primitives::{IconButton, IconName, Kbd, RowSemantics, TextField, VirtualList};
use crate::tokens::{self, Theme};

const ROW: f32 = 44.;
const MARK: f32 = 6.;
const CHIP: f32 = 20.;

struct Shown {
    rows: Rc<[Row]>,
    lines: Rc<[Line]>,
    selected: Option<&'static str>,
    stage: Stage,
    page: WeakEntity<ShortcutsPage>,
}

fn named(prefix: &str, name: &str) -> ElementId {
    ElementId::Name(format!("{prefix}-{name}").into())
}

fn chip(t: &Theme, text: String, is_new: bool) -> Div {
    let (edge, color) = if is_new {
        (t.colors.accent, t.colors.accent_text)
    } else {
        (t.colors.line, t.colors.muted)
    };
    div()
        .flex()
        .flex_none()
        .items_center()
        .h(px(CHIP))
        .px(t.spacing.sm)
        .rounded(t.radii.sm)
        .border_1()
        .border_color(edge)
        .font_family(t.typography.mono_font.clone())
        .text_size(t.typography.key)
        .text_color(color)
        .child(text)
}

fn quiet(t: &Theme, text: &'static str) -> Div {
    div()
        .flex_none()
        .text_size(t.typography.small)
        .text_color(t.colors.faint)
        .child(text)
}

fn keys_of(t: &Theme, shown_page: &Shown, row: &Row) -> Vec<AnyElement> {
    let name = row.spec.name;
    if shown_page.stage == Stage::Recording(row.spec) {
        let waiting =
            chip(t, "Press a key…".to_owned(), true).font_family(t.typography.ui_font.clone());
        return vec![waiting.into_any_element()];
    }
    let proposed = shown_page
        .stage
        .pending()
        .filter(|pending| pending.spec.name == name)
        .and_then(|pending| pending.keys.clone());
    let (keys, is_new) = match proposed {
        Some(keys) => (keys, true),
        None => (row.keys.clone(), false),
    };
    if keys.is_empty() {
        return vec![quiet(t, "No key").into_any_element()];
    }
    keys.iter()
        .enumerate()
        .map(
            |(index, key)| match Kbd::new(("key", index), key).filter(|_| !is_new) {
                Some(kbd) => kbd.into_any_element(),
                None => {
                    chip(t, shown(key).unwrap_or_else(|| key.clone()), is_new).into_any_element()
                }
            },
        )
        .collect()
}

fn title(t: &Theme, row: &Row) -> Div {
    let name = row.spec.name;
    let mark = row.is_changed.then(|| {
        div()
            .id(named("shortcut-changed", name))
            .role(Role::Note)
            .aria_label("Changed")
            .test_support()
            .flex_none()
            .size(px(MARK))
            .rounded(t.radii.full)
            .bg(t.colors.accent)
    });
    let label = div()
        .flex()
        .items_center()
        .gap(t.spacing.sm)
        .child(
            div()
                .truncate()
                .font_weight(FontWeight::MEDIUM)
                .child(tr(row.spec.label)),
        )
        .children(mark);
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .child(label)
        .child(
            div()
                .truncate()
                .font_family(t.typography.mono_font.clone())
                .text_size(t.typography.small)
                .text_color(t.colors.muted)
                .child(name),
        )
}

fn reset_button(page: &WeakEntity<ShortcutsPage>, name: &'static str) -> IconButton {
    let page = page.clone();
    IconButton::new(
        named("shortcut-reset", name),
        IconName::RotateCcw,
        "Reset to the default key",
    )
    .on_click(move |_, _, cx| {
        cx.stop_propagation();
        let _ = page.update(cx, |page, cx| {
            page.select(name, cx);
            page.reset(cx);
        });
    })
}

fn command_line(t: &Theme, shown_page: &Shown, row: &Row) -> AnyElement {
    let name = row.spec.name;
    let is_selected = shown_page.selected == Some(name);
    let page = shown_page.page.clone();
    div()
        .id(named("shortcut", name))
        .role(Role::Button)
        .aria_label(tr(row.spec.label))
        .aria_selected(is_selected)
        .test_support()
        .flex()
        .items_center()
        .gap(t.spacing.lg)
        .size_full()
        .mr(t.spacing.xxl)
        .px(t.spacing.lg)
        .rounded(t.radii.md)
        .border_1()
        .border_color(if is_selected {
            t.colors.accent
        } else {
            gpui_kit::transparent_black()
        })
        .cursor_pointer()
        .when(is_selected, |line| line.bg(t.colors.selection))
        .when(!is_selected, |line| {
            line.hover(|line| line.bg(t.colors.hover))
        })
        .child(title(t, row))
        .child(
            div()
                .flex()
                .flex_none()
                .gap(t.spacing.xs)
                .children(keys_of(t, shown_page, row)),
        )
        .children(row.is_changed.then(|| reset_button(&shown_page.page, name)))
        .on_click(move |_, window, cx| {
            let _ = page.update(cx, |page, cx| {
                page.select(name, cx);
                page.start_recording(cx);
                window.focus(&page.focus_handle(cx), cx);
            });
        })
        .into_any_element()
}

fn area_line(t: &Theme, area: &str) -> AnyElement {
    div()
        .flex()
        .items_end()
        .size_full()
        .mr(t.spacing.xxl)
        .px(t.spacing.lg)
        .pb(t.spacing.xs)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .font_weight(FontWeight::SEMIBOLD)
        .child(area_label(area))
        .into_any_element()
}

fn lines_in(shown_page: &Shown, range: Range<usize>, cx: &App) -> Vec<AnyElement> {
    let t = tokens::current(cx);
    range
        .filter_map(|index| shown_page.lines.get(index))
        .map(|line| match line {
            Line::Area(area) => area_line(t, area),
            Line::Row(row) => match shown_page.rows.get(*row) {
                Some(row) => command_line(t, shown_page, row),
                None => div().into_any_element(),
            },
        })
        .collect()
}

fn describe(shown_page: &Shown, index: usize) -> RowSemantics {
    let row = match shown_page.lines.get(index) {
        Some(Line::Row(row)) => shown_page.rows.get(*row),
        _ => None,
    };
    RowSemantics {
        label: row.map(|row| tr(row.spec.label).into()),
        is_selected: row.map(|row| shown_page.selected == Some(row.spec.name)),
        is_active_descendant: row.is_some_and(|row| shown_page.selected == Some(row.spec.name)),
    }
}

fn list(page: &ShortcutsPage, cx: &Context<ShortcutsPage>) -> VirtualList {
    let shown_page = Rc::new(Shown {
        rows: page.rows.clone(),
        lines: page.lines.clone(),
        selected: page.selected,
        stage: page.stage.clone(),
        page: cx.weak_entity(),
    });
    let described = shown_page.clone();
    VirtualList::new(
        "shortcuts-list",
        page.lines.len(),
        &page.scroll,
        move |range, _, cx| lines_in(&shown_page, range, cx),
    )
    .row_height(px(ROW))
    .label("Shortcuts")
    .describe_rows(move |index| describe(&described, index))
}

pub(super) fn render(
    page: &ShortcutsPage,
    _: &mut Window,
    cx: &mut Context<ShortcutsPage>,
) -> impl IntoElement + use<> {
    let t = tokens::current(cx).clone();
    let can_cancel = matches!(page.stage, Stage::Recording(_) | Stage::Conflict(..));
    let search = div()
        .flex_none()
        .px(t.spacing.xl)
        .pt(t.spacing.lg)
        .pb(t.spacing.md)
        .child(TextField::new(
            "shortcuts-search",
            &page.search,
            "Search shortcuts",
        ));
    div()
        .id("shortcuts-page")
        .key_context(CONTEXT)
        .role(Role::Group)
        .aria_label("Shortcuts")
        .test_support()
        .on_action(cx.listener(|page, _: &PreviousRow, _, cx| page.step(false, cx)))
        .on_action(cx.listener(|page, _: &NextRow, _, cx| page.step(true, cx)))
        .on_action(cx.listener(|page, _: &Record, _, cx| page.confirm(cx)))
        .on_action(cx.listener(|page, _: &Reset, _, cx| page.reset(cx)))
        .when(can_cancel, |root| {
            root.on_action(cx.listener(|page, _: &Cancel, _, cx| page.cancel(cx)))
        })
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .children(page.problem.as_ref().map(|problem| banner(&t, problem, cx)))
        .child(search)
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .px(t.spacing.md)
                .child(list(page, cx)),
        )
        .children(footer(&t, &page.stage, cx))
}
