//! Tick boxes in the gutter of the file diff (V32): one box per changed line, shown while the
//! pointer is over its change or while any line is picked. A click picks the line, Shift+click
//! picks a range. Colours come from theme tokens; the box appears with no motion.

use std::collections::BTreeSet;
use std::rc::Rc;

use gpui_kit::Role;
use gpui_kit::base::TestSupportExt;

use super::hooks::{DiffRow, PairText};
use super::rows::{ShownKind, ShownRow};
use crate::prelude::*;
use crate::primitives::{IconName, icon};
use crate::tokens::Theme;

/// Width and height of a tick box in pixels, with the check inside it.
const BOX: f32 = 14.;
const CHECK: f32 = 11.;

/// The side of the pair a line is picked on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickSide {
    Left,
    Right,
}

/// The line index `row` holds on `side`: changed and one-sided rows only, never an equal row.
#[must_use]
pub fn line_on(row: DiffRow, side: PickSide) -> Option<u32> {
    match (row, side) {
        (DiffRow::Changed(line, _) | DiffRow::Deleted(line), PickSide::Left)
        | (DiffRow::Changed(_, line) | DiffRow::Inserted(line), PickSide::Right) => Some(line),
        _ => None,
    }
}

/// Called with the side and line index of a clicked box, and whether Shift was held.
pub type OnTick = Rc<dyn Fn(PickSide, u32, bool, &mut Window, &mut App)>;

/// The picked lines of the open pair as one frame draws them.
pub struct Ticks<'a> {
    pub text: &'a PairText,
    /// The side the picks are on; `None` while nothing is picked.
    pub side: Option<PickSide>,
    pub picked: &'a BTreeSet<u32>,
    /// The line the keys move.
    pub cursor: Option<u32>,
    /// The change the pointer is over.
    pub hovered: Option<u32>,
    pub on_tick: OnTick,
}

impl Ticks<'_> {
    /// Whether the line `kind` of change `hunk` can be picked: changed, in a copyable change.
    fn pickable(&self, kind: ShownKind, hunk: Option<u32>) -> bool {
        let copyable = hunk
            .and_then(|hunk| self.text.hunks.get(hunk as usize))
            .is_some_and(|hunk| hunk.digest.is_some());
        copyable && !matches!(kind, ShownKind::Equal | ShownKind::Filler)
    }

    /// Whether the boxes of change `hunk` show: any pick, or the pointer over the change.
    fn visible(&self, hunk: Option<u32>) -> bool {
        !self.picked.is_empty() || self.cursor.is_some() || (hunk.is_some() && hunk == self.hovered)
    }
}

/// The gutter cell of one side of `row`: a fixed-width slot, holding a box when the line can be
/// picked. The slot is always there so line numbers do not move when boxes appear.
pub fn tick_slot(t: &Theme, ticks: &Ticks, row: &ShownRow, side: PickSide) -> Div {
    let line = match side {
        PickSide::Left => row.left.map(|(line, _)| line),
        PickSide::Right => row.right.map(|(line, _)| line),
    };
    let slot = div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .w(px(BOX))
        .h(px(BOX))
        .mr(t.spacing.sm);
    let Some(line) = line else {
        return slot;
    };
    if !ticks.pickable(row.kind, row.hunk) || !ticks.visible(row.hunk) {
        return slot;
    }
    slot.child(tick_box(t, ticks, (side, line), row.row))
}

/// The box itself: ticked when picked, ringed when the keys sit on it.
fn tick_box(t: &Theme, ticks: &Ticks, (side, line): (PickSide, u32), row: u32) -> impl IntoElement {
    let on = ticks.side == Some(side) && ticks.picked.contains(&line);
    let cursor = ticks.side == Some(side) && ticks.cursor == Some(line);
    let name = match side {
        PickSide::Left => "compare-tick-left",
        PickSide::Right => "compare-tick-right",
    };
    let label = navo_l10n::t!("compare-tick-label", line = u64::from(line) + 1);
    let on_tick = Rc::clone(&ticks.on_tick);
    let ring = if on || cursor {
        t.colors.accent
    } else {
        t.colors.faint
    };
    div()
        .id((name, row as usize))
        .role(Role::CheckBox)
        .aria_label(label)
        .test_support()
        .flex()
        .items_center()
        .justify_center()
        .size(px(BOX))
        .rounded(t.radii.sm)
        .border_1()
        .border_color(ring)
        .cursor_pointer()
        .when(on, |tick| {
            tick.bg(t.colors.accent)
                .child(icon(IconName::Check, px(CHECK), t.colors.on_button))
        })
        .on_click(move |event, window, cx| {
            cx.stop_propagation();
            on_tick(side, line, event.modifiers().shift, window, cx);
        })
}
