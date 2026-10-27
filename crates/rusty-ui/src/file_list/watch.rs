use std::rc::Rc;

use gpui_kit::{App, Bounds, Entity, Pixels, ScrollWheelEvent, Window};
use rusty_core::Command;

use super::{KnownWidth, ListContext, ListState, PageRowsHandler, lift, row_index, row_number};
use crate::primitives::ListScroll;

pub(super) fn follow_scroll(
    state: &Entity<ListState>,
    (wanted, len): (u64, usize),
    row_height: Pixels,
    cx: &mut App,
) -> ListScroll {
    state.update(cx, |state, _| {
        let key = (wanted, len, row_height);
        if len > 0 && state.followed_scroll != Some(key) {
            state.followed_scroll = Some(key);
            if row_number(state.scroll.top_row(row_height)) != wanted {
                state.scroll.scroll_to_top(row_index(wanted));
            }
        }
        state.scroll.clone()
    })
}

pub(super) struct Watch {
    pub state: Entity<ListState>,
    pub row_height: Pixels,
}

fn shown_top(state: &ListState, len: usize, row_height: Pixels) -> u64 {
    let page = state.page_rows.map_or(1, row_index).max(1);
    let last_top = len.saturating_sub(page);
    row_number(state.scroll.top_row(row_height).min(last_top))
}

pub(super) fn report_scroll(
    ctx: &Rc<ListContext>,
    watch: Watch,
) -> impl Fn(&ScrollWheelEvent, &mut Window, &mut App) + 'static {
    let ctx = ctx.clone();
    move |_, window, cx| {
        let known = ctx.props.view.scroll;
        let top = watch.state.update(cx, |state, _| {
            let top = shown_top(state, ctx.props.rows.len(), watch.row_height);
            state.synced_top = Some((known, top));
            top
        });
        if top != known {
            ctx.send(Command::PaneScroll { to: top }, window, cx);
        }
    }
}

fn sync_top(ctx: &Rc<ListContext>, watch: &Watch, window: &mut Window, cx: &mut App) {
    let props = &ctx.props;
    if !props.is_active || props.rows.is_empty() {
        return;
    }
    let known = props.view.scroll;
    let top = watch.state.update(cx, |state, _| {
        state.page_rows?;
        let top = shown_top(state, props.rows.len(), watch.row_height);
        let pair = Some((known, top));
        let is_past_end = known > top && state.scroll.is_at_bottom();
        let is_new = top != known && !is_past_end && state.synced_top != pair;
        state.synced_top = pair;
        is_new.then_some(top)
    });
    if let Some(top) = top {
        let ctx = ctx.clone();
        window.defer(cx, move |window, cx| {
            ctx.send(Command::PaneScroll { to: top }, window, cx);
        });
    }
}

pub(super) fn report_page_rows(
    (ctx, handler, known_width): (Rc<ListContext>, Option<PageRowsHandler>, Option<KnownWidth>),
    watch: Watch,
) -> impl Fn(Vec<Bounds<Pixels>>, &mut Window, &mut App) + 'static {
    move |bounds, window, cx| {
        let Some(list) = bounds.first() else {
            return;
        };
        let rows = (list.size.height / watch.row_height).floor();
        let rows = if rows.is_finite() && rows > 0. {
            rows as u64
        } else {
            0
        };
        let is_new = watch.state.update(cx, |state, cx| {
            let is_new = state.page_rows != Some(rows);
            state.page_rows = Some(rows);
            let was = state.list_bounds.map(|bounds| bounds.size.width);
            state.list_bounds = Some(lift::unshifted(*list, state.lift));
            if let Some(known) = &known_width {
                known.set(Some(list.size.width));
            }
            if was != Some(list.size.width) {
                cx.notify();
            }
            is_new
        });
        if let (true, Some(handler)) = (is_new, handler.clone()) {
            window.defer(cx, move |window, cx| handler(&rows, window, cx));
        }
        sync_top(&ctx, &watch, window, cx);
    }
}
