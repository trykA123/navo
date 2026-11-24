//! Builds the file list's scrolling area and its rows, and routes a left press on a row.

use std::ops::Range;
use std::rc::Rc;

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, ClickEvent, Div, InteractiveElement, IntoElement, MouseButton, MouseDownEvent,
    ParentElement, Pixels, StatefulInteractiveElement, Styled, Window, div,
};
use navo_core::{Command, CursorMove, SpanMode};

use super::row::{self, RowLook};
use super::{
    FileListAction, FileListProps, KnownWidth, ListContext, PageRowsHandler, drop, git_cell, kind,
    lasso, lift, press, row_number, watch,
};
use crate::name_match::Needle;
use crate::primitives::{ListScroll, RowSemantics, VirtualList};
use crate::tokens::{self, Theme};

fn render_row(
    ctx: &Rc<ListContext>,
    (theme, needle): (&Theme, Option<&Needle>),
    index: usize,
) -> Option<AnyElement> {
    let entry = ctx.props.rows.get(index)?;
    let row = row_number(index);
    let note = ctx.note_of.and_then(|note| note(&entry.name.display()));
    let look = RowLook {
        theme,
        entry,
        is_cursor: ctx.props.view.cursor == row,
        is_marked: ctx.props.marked.is_marked(row),
        is_active_pane: ctx.props.is_active,
        parts: ctx.parts,
        widths: ctx.widths,
        is_find: ctx.find_results.is_some(),
        needle,
        git: git_cell::slot(ctx, (entry, row)),
        note: note.as_ref(),
    };
    let (left, right, click) = (ctx.clone(), ctx.clone(), ctx.clone());
    let zones = (drop::grip(ctx, row), lasso::start_zone(ctx, row));
    let element = row::cells(&look, zones)
        .id("cells")
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            left.left_down((row, event), window, cx);
        })
        .on_mouse_down(MouseButton::Right, move |event, window, cx| {
            cx.stop_propagation();
            right.right_down((row, event.position, event.modifiers.shift), window, cx);
        })
        .on_mouse_move(press::drag_over(ctx, row))
        .on_click(move |event, window, cx| click.modified_click((row, event), window, cx));
    Some(drop::target(element, ctx, (row, theme)).into_any_element())
}

pub(super) fn render_rows(
    ctx: &Rc<ListContext>,
) -> impl Fn(Range<usize>, &mut Window, &mut App) -> Vec<AnyElement> + 'static {
    let ctx = ctx.clone();
    move |range, _, cx| {
        let theme = tokens::current(cx);
        let needle = ctx.props.filter.as_deref().map(Needle::parse);
        range
            .filter_map(|index| render_row(&ctx, (theme, needle.as_ref()), index))
            .collect()
    }
}

impl ListContext {
    fn left_down(&self, (row, event): (u64, &MouseDownEvent), window: &mut Window, cx: &mut App) {
        self.state.update(cx, |state, _| state.lasso.press());
        let modifiers = event.modifiers;
        if modifiers.control || modifiers.shift {
            self.gate_down(event.position, cx);
            self.take_focus(window, cx);
            return;
        }
        self.click(row, window, cx);
        if event.click_count >= 2 {
            self.act(FileListAction::Open(row), window, cx);
        }
    }

    /// Extends or toggles the marks for a Shift or Ctrl left click on `row`. Uses the modifiers
    /// held at the press, and ignores a click that GPUI completed with another button's release.
    fn modified_click(&self, (row, event): (u64, &ClickEvent), window: &mut Window, cx: &mut App) {
        let ClickEvent::Mouse(click) = event else {
            return;
        };
        if !event.standard_click() || self.state.read(cx).lasso.swallows_click() {
            return;
        }
        let modifiers = click.down.modifiers;
        let to = CursorMove::Row(row);
        if modifiers.shift {
            let mode = SpanMode::Mark;
            self.send(Command::SelectionExtend { to, mode }, window, cx);
        } else if modifiers.control {
            self.send(Command::PaneCursorMove { to }, window, cx);
            self.send(Command::SelectionToggle, window, cx);
        }
    }
}

pub(super) fn row_semantics(props: &FileListProps, index: usize) -> RowSemantics {
    let row = row_number(index);
    RowSemantics {
        label: props
            .rows
            .get(index)
            .map(|entry| kind::spoken(entry).into()),
        is_selected: Some(props.marked.is_marked(row)),
        is_active_descendant: props.is_active && props.view.cursor == row,
    }
}

fn virtual_list(ctx: &Rc<ListContext>, scroll: &ListScroll, row_height: Pixels) -> VirtualList {
    let semantics = ctx.clone();
    let label = format!("{} files", ctx.props.folder);
    VirtualList::new("list", ctx.props.rows.len(), scroll, render_rows(ctx))
        .row_height(row_height)
        .label(label)
        .describe_rows(move |index| row_semantics(&semantics.props, index))
        .when_some(ctx.focus.clone(), |list, focus| list.track_focus(&focus))
}

/// Builds the list area below the header: the lifted rows, their size and scroll reports,
/// the lasso, the drag gate, the lift hold and the folder drop target.
pub(super) fn list_area(
    ctx: &Rc<ListContext>,
    (on_page_rows, known_width): (Option<PageRowsHandler>, Option<KnownWidth>),
    (scroll, row_height, lift): (&ListScroll, Pixels, Pixels),
    (t, cx): (&Theme, &App),
) -> Div {
    let list = virtual_list(ctx, scroll, row_height);
    let watch = |state| watch::Watch { state, row_height };
    let area = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .on_children_prepainted(watch::report_page_rows(
            (ctx.clone(), on_page_rows, known_width),
            watch(ctx.state.clone()),
        ))
        .on_scroll(watch::report_scroll(ctx, watch(ctx.state.clone())))
        .on_mouse_down(MouseButton::Right, press::background_down(ctx))
        .overflow_hidden()
        .child(lift::lifted(list, lift))
        .child(lift::hold(ctx));
    drop::background(lasso::layer(area, ctx, t, cx), ctx)
}
