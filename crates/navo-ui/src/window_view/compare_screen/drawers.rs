//! The drawers of the file diff (D58): Ctrl+B lists the changes (Up, Down, Enter jump),
//! Ctrl+I shows the facts of the pair and every fallback. Both start closed; Esc closes one.

use std::sync::Arc;

use gpui_kit::{KeyDownEvent, Stateful, rems};
use navo_l10n::{t, tr};

use crate::commands::compare::{Changes, Details};
use crate::compare_diff::{DiffRow, PairFacts, PairText};
use crate::prelude::*;
use crate::primitives::{ListScroll, VirtualList};
use crate::tokens::Theme;
use crate::window_view::NavoWindow;

/// Width of a drawer, in rem.
const WIDTH_REM: f32 = 16.;

/// Which drawers are open, and the cursor in the list of changes.
pub(super) struct Drawers {
    pub(super) changes: bool,
    pub(super) details: bool,
    pub(super) cursor: usize,
    scroll: ListScroll,
}

impl Drawers {
    /// Both drawers closed.
    pub(super) fn new() -> Self {
        Self {
            changes: false,
            details: false,
            cursor: 0,
            scroll: ListScroll::new(),
        }
    }

    /// Closes the details drawer, else the list of changes; false when none was open.
    pub(super) fn close_one(&mut self) -> bool {
        let was_open = self.details || self.changes;
        match self.details {
            true => self.details = false,
            false => self.changes = false,
        }
        was_open
    }
}

/// The line (1-based) where change `hunk` starts.
fn first_line(text: &PairText, hunk: usize) -> u64 {
    let row = text
        .hunks
        .get(hunk)
        .and_then(|h| text.rows.get(h.rows.start as usize));
    let line = match row {
        Some(DiffRow::Equal(l, _) | DiffRow::Changed(l, _) | DiffRow::Deleted(l)) => *l,
        Some(DiffRow::Inserted(r)) => *r,
        None => 0,
    };
    u64::from(line) + 1
}

/// A drawer frame of fixed width with a title.
pub(super) fn frame(t: &Theme, id: &'static str, title: String) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_col()
        .flex_none()
        .w(rems(WIDTH_REM))
        .min_h_0()
        .bg(t.colors.rail)
        .border_color(t.colors.line)
        .child(
            div()
                .flex_none()
                .px(t.spacing.md)
                .h(t.spacing.row_height)
                .flex()
                .items_center()
                .font_weight(FontWeight::SEMIBOLD)
                .text_size(t.typography.small)
                .child(title),
        )
}

/// The list of changes: one row per change with its first line and `+n −m`.
pub(super) fn changes_view(t: &Theme, text: &Arc<PairText>, drawers: &Drawers) -> AnyElement {
    let (rows_text, cursor, theme) = (Arc::clone(text), drawers.cursor, t.clone());
    let rows = move |range: std::ops::Range<usize>, _: &mut Window, _: &mut App| {
        range
            .filter_map(|at| Some((at, rows_text.hunks.get(at)?)))
            .map(|(at, hunk)| {
                let label = t!(
                    "compare-drawer-change",
                    line = first_line(&rows_text, at),
                    added = hunk.added,
                    removed = hunk.removed
                );
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .px(theme.spacing.md)
                    .text_size(theme.typography.small)
                    .when(at == cursor, |row| row.bg(theme.surfaces.current))
                    .child(label)
                    .into_any_element()
            })
            .collect()
    };
    let list = VirtualList::new(
        "compare-changes-rows",
        text.hunks.len(),
        &drawers.scroll,
        rows,
    )
    .row_height(t.spacing.row_height)
    .label(tr("compare-drawer-changes"));
    frame(t, "compare-changes", tr("compare-drawer-changes"))
        .border_r_1()
        .child(div().flex_1().min_h_0().child(list))
        .into_any_element()
}

/// One line of the details drawer.
pub(super) fn fact(t: &Theme, text: String) -> Div {
    div()
        .px(t.spacing.md)
        .py(t.spacing.xs)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(text)
}

/// The two names of a fact, as one name when they are the same.
fn pair_of(left: &'static str, right: &'static str) -> String {
    match left == right {
        true => tr(left),
        false => t!("compare-drawer-sides", left = tr(left), right = tr(right)),
    }
}

/// The details drawer: encodings, line endings, whether raw bytes differ, every fallback.
pub(super) fn details_view(t: &Theme, facts: &PairFacts) -> AnyElement {
    let raw = match (facts.raw_equal, facts.display_equal) {
        (true, _) => Some(tr("compare-drawer-same")),
        (false, true) => Some(tr("compare-drawer-raw-differs")),
        (false, false) => None,
    };
    let encoding = format!(
        "{}: {}",
        tr("compare-drawer-encoding"),
        pair_of(facts.left_encoding, facts.right_encoding)
    );
    let endings = format!(
        "{}: {}",
        tr("compare-drawer-endings"),
        pair_of(facts.left_endings, facts.right_endings)
    );
    let fallbacks = (!facts.fallbacks.is_empty()).then(|| fact(t, tr("compare-drawer-fallbacks")));
    frame(t, "compare-details", tr("compare-drawer-details"))
        .border_l_1()
        .child(fact(t, encoding))
        .child(fact(t, endings))
        .children(raw.map(|raw| fact(t, raw)))
        .children(fallbacks)
        .children(facts.fallbacks.iter().map(|id| fact(t, tr(id))))
        .into_any_element()
}

impl NavoWindow {
    /// Registers Ctrl+B, Ctrl+I and the list keys on the file diff root.
    pub(super) fn drawer_actions(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &Changes, _, cx| view.toggle_drawer(true, cx)))
            .on_action(cx.listener(|view, _: &Details, _, cx| view.toggle_drawer(false, cx)))
            .on_key_down(cx.listener(|view, event: &KeyDownEvent, window, cx| {
                view.drawer_key(event, window, cx);
            }))
    }

    /// Opens or closes the list of changes (`changes`) or the details drawer.
    fn toggle_drawer(&mut self, changes: bool, cx: &mut Context<Self>) {
        let Some(file) = self.compare.screen.as_mut().and_then(|s| s.file.as_mut()) else {
            return;
        };
        let current = file.view.as_ref().and_then(|view| view.current());
        let drawers = &mut file.drawers;
        match changes {
            true => {
                drawers.changes = !drawers.changes;
                drawers.cursor = current.unwrap_or(0);
                drawers.scroll.scroll_to(drawers.cursor);
            }
            false => drawers.details = !drawers.details,
        }
        cx.notify();
    }

    /// Up, Down and Enter in the open list of changes; Enter jumps to the change.
    fn drawer_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(file) = self.compare.screen.as_mut().and_then(|s| s.file.as_mut()) else {
            return;
        };
        let Some(view) = file.view.as_mut().filter(|_| file.drawers.changes) else {
            return;
        };
        let last = view.change_count().saturating_sub(1);
        let drawers = &mut file.drawers;
        match event.keystroke.key.as_str() {
            "down" => drawers.cursor = (drawers.cursor + 1).min(last),
            "up" => drawers.cursor = drawers.cursor.saturating_sub(1),
            "enter" => view.go_to(drawers.cursor, file.look.layout_in(window)),
            _ => return,
        }
        drawers.scroll.scroll_to(drawers.cursor);
        cx.stop_propagation();
        cx.notify();
    }
}
