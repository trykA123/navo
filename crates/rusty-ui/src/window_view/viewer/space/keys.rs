use std::sync::LazyLock;

use gpui_kit::{Focusable, KeyContext, Stateful};
use rusty_automotive::space::{ScaleMode, Target, Tile};
use rusty_l10n::tr;

use super::layout::GROUPINGS;
use super::rich::{Pick, RichHost, RichSpace};
use super::strip::Step;
use crate::prelude::*;
use crate::window_view::viewer::body::Key;
use crate::window_view::viewer::parts::LayoutParts;

pub(in crate::window_view::viewer) const CONTEXT: &str = "Space";
pub(super) const STRIP_CONTEXT: &str = "SpaceStrip";
pub(super) const TREEMAP_CONTEXT: &str = "SpaceTreemap";
pub(in crate::window_view::viewer) const NEXT_PART: Key = ("Tab", "Parts");
static OPEN_MODULE: LazyLock<String> = LazyLock::new(|| tr("space-open-module"));
static ALL_MODULES: LazyLock<String> = LazyLock::new(|| tr("space-all-modules"));

gpui_kit::actions!(
    space_view,
    [
        NextPart,
        PreviousPart,
        PreviousItem,
        NextItem,
        RegionAbove,
        RegionBelow,
        FirstBlock,
        LastBlock,
        ZoomIn,
        ZoomOut,
        PickGroup,
        NextGrouping,
        ToggleScale
    ]
);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(in crate::window_view::viewer) enum Part {
    #[default]
    Table,
    Strip(usize),
    Treemap,
}

pub(in crate::window_view::viewer) fn bind_keys(cx: &mut App) {
    crate::commands::bind_namespace(cx, "space_view");
}

pub(in crate::window_view::viewer) fn host_context(name: &'static str) -> KeyContext {
    let mut context = KeyContext::default();
    context.add(name);
    context.add(CONTEXT);
    context
}

pub(in crate::window_view::viewer) fn part_keys(rich: &RichSpace, cx: &App) -> Option<Vec<Key>> {
    match rich.part {
        Part::Table => None,
        Part::Strip(at) => {
            let is_zoomed = rich
                .strips
                .get(at)
                .is_some_and(|strip| strip.read(cx).zoom().is_some());
            let zoom = if is_zoomed {
                ("Bksp", "Unzoom")
            } else {
                ("Enter", "Zoom")
            };
            Some(vec![
                ("←/→", "Block"),
                ("↑/↓", "Region"),
                zoom,
                ("Alt+Z", "Scale"),
                NEXT_PART,
            ])
        }
        Part::Treemap => Some(
            vec![
                ("Arrows", "Tile"),
                (
                    "Enter",
                    if rich.module.is_none()
                        && rich.model.as_ref().is_some_and(|model| {
                            model.from_map
                                && model.grouping == rusty_automotive::space::Grouping::Module
                        })
                    {
                        OPEN_MODULE.as_str()
                    } else {
                        "Select"
                    },
                ),
                ("Alt+G", "Grouping"),
                NEXT_PART,
            ]
            .into_iter()
            .chain(rich.module.as_ref().map(|_| ("Bksp", ALL_MODULES.as_str())))
            .collect(),
        ),
    }
}

fn reading_order(rich: &RichSpace) -> Vec<u32> {
    let shows = rich.model.as_ref().is_some_and(|model| !model.stripped);
    let Some(tiles) = rich.tiles().filter(|_| shows) else {
        return Vec::new();
    };
    let mut placed: Vec<(&Tile, u32)> = tiles
        .iter()
        .filter_map(|tile| match tile.target {
            Target::Item(id) => Some((tile, id)),
            Target::More { .. } => None,
        })
        .collect();
    placed.sort_by(|(a, _), (b, _)| {
        a.rect
            .y
            .total_cmp(&b.rect.y)
            .then(a.rect.x.total_cmp(&b.rect.x))
    });
    placed.into_iter().map(|(_, id)| id).collect()
}

fn strip_stops(rich: &RichSpace) -> Vec<usize> {
    let Some(model) = rich.model.as_ref() else {
        return Vec::new();
    };
    (0..rich.strips.len())
        .filter(|at| model.regions.get(*at).is_some_and(|r| !r.blocks.is_empty()))
        .collect()
}

fn parts(rich: &RichSpace, layout: LayoutParts) -> Vec<Part> {
    let mut parts = vec![Part::Table];
    if rich.model.is_none() {
        return parts;
    }
    if layout.strip {
        parts.extend(strip_stops(rich).into_iter().map(Part::Strip));
    }
    if layout.who && rich.treemap_focus.is_some() && !reading_order(rich).is_empty() {
        parts.push(Part::Treemap);
    }
    parts
}

fn focused(rich: &RichSpace, window: &Window, cx: &App) -> Part {
    let strip = rich
        .strips
        .iter()
        .position(|strip| strip.focus_handle(cx).is_focused(window));
    let treemap = rich
        .treemap_focus
        .as_ref()
        .is_some_and(|focus| focus.is_focused(window));
    match strip {
        Some(at) => Part::Strip(at),
        None if treemap => Part::Treemap,
        None => Part::Table,
    }
}

fn current<V: RichHost>(view: &mut V, window: &Window, cx: &App) -> Option<(LayoutParts, Part)> {
    let layout = view.space_layout()?;
    let rich = view.rich();
    rich.model.as_ref()?;
    let part = focused(rich, window, cx);
    rich.part = part;
    Some((layout, part))
}

fn focus_part<V: RichHost>(view: &mut V, part: Part, window: &mut Window, cx: &mut Context<V>) {
    let handle = match part {
        Part::Table => Some(view.table_focus()),
        Part::Strip(at) => view
            .rich()
            .strips
            .get(at)
            .map(|strip| strip.focus_handle(cx)),
        Part::Treemap => view.rich().treemap_focus.clone(),
    };
    let Some(handle) = handle else {
        return;
    };
    let rich = view.rich();
    rich.part = part;
    if let Part::Strip(at) = part {
        rich.top_scroll.scroll_to_item(at + 1);
    }
    if part == Part::Treemap && rich.tile.is_none() {
        rich.tile = match rich.pick {
            Some(Pick::Group(id)) => Some(id),
            _ => reading_order(rich).first().copied(),
        };
    }
    window.focus(&handle, cx);
    cx.notify();
}

fn move_part<V: RichHost>(view: &mut V, back: bool, window: &mut Window, cx: &mut Context<V>) {
    let Some((layout, now)) = current(view, window, cx) else {
        return cx.propagate();
    };
    let parts = parts(view.rich(), layout);
    let len = parts.len();
    if len < 2 {
        return cx.propagate();
    }
    let at = parts.iter().position(|part| *part == now).unwrap_or(0);
    let next = if back {
        (at + len - 1) % len
    } else {
        (at + 1) % len
    };
    focus_part(view, parts[next], window, cx);
}

fn step_strip<V: RichHost>(view: &mut V, step: Step, window: &Window, cx: &mut Context<V>) {
    let Some((_, Part::Strip(at))) = current(view, window, cx) else {
        return cx.propagate();
    };
    if let Some(strip) = view.rich().strips.get(at).cloned() {
        strip.update(cx, |strip, cx| strip.step(step, cx));
    }
}

fn move_tile<V: RichHost>(view: &mut V, forward: bool, cx: &mut Context<V>) {
    let rich = view.rich();
    let order = reading_order(rich);
    let Some(last) = order.len().checked_sub(1) else {
        return;
    };
    let at = rich
        .tile
        .and_then(|id| order.iter().position(|tile| *tile == id));
    let next = match (at, forward) {
        (None, _) => 0,
        (Some(at), true) => (at + 1).min(last),
        (Some(at), false) => at.saturating_sub(1),
    };
    rich.tile = Some(order[next]);
    cx.notify();
}

fn step_item<V: RichHost>(view: &mut V, forward: bool, window: &Window, cx: &mut Context<V>) {
    match current(view, window, cx) {
        Some((_, Part::Treemap)) => move_tile(view, forward, cx),
        Some((_, Part::Strip(_))) => {
            let step = if forward { Step::Next } else { Step::Prev };
            step_strip(view, step, window, cx);
        }
        _ => cx.propagate(),
    }
}

fn change_region<V: RichHost>(view: &mut V, below: bool, window: &mut Window, cx: &mut Context<V>) {
    let Some((_, Part::Strip(at))) = current(view, window, cx) else {
        return cx.propagate();
    };
    let stops = strip_stops(view.rich());
    let next = if below {
        stops.iter().copied().find(|stop| *stop > at)
    } else {
        stops.iter().rev().copied().find(|stop| *stop < at)
    };
    if let Some(next) = next {
        focus_part(view, Part::Strip(next), window, cx);
    }
}

fn zoom<V: RichHost>(view: &mut V, out: bool, window: &Window, cx: &mut Context<V>) {
    let part = current(view, window, cx);
    if out && matches!(part, Some((_, Part::Treemap))) && view.leave_module(cx) {
        return;
    }
    let strip = match part {
        Some((_, Part::Strip(at))) => view.rich().strips.get(at).cloned(),
        _ => None,
    };
    let Some(strip) = strip else {
        return cx.propagate();
    };
    let done = strip.update(cx, |strip, cx| {
        if out {
            strip.zoom_out(cx)
        } else {
            strip.zoom_in(cx)
        }
    });
    if out && !done {
        return cx.propagate();
    }
    cx.notify();
}

fn pick_group<V: RichHost>(view: &mut V, window: &Window, cx: &mut Context<V>) {
    let Some((_, Part::Treemap)) = current(view, window, cx) else {
        return cx.propagate();
    };
    if let Some(id) = view.rich().tile
        && !view.enter_module(id, cx)
    {
        view.picked(Pick::Group(id), cx);
    }
}

fn next_grouping<V: RichHost>(view: &mut V, window: &Window, cx: &mut Context<V>) {
    let model = current(view, window, cx)
        .filter(|(layout, _)| layout.who)
        .and_then(|_| view.rich().model.clone());
    let Some(model) = model else {
        return cx.propagate();
    };
    let len = GROUPINGS.len();
    let now = GROUPINGS
        .iter()
        .position(|(grouping, _)| *grouping == model.grouping)
        .unwrap_or(0);
    let next = (1..len)
        .map(|by| (now + by) % len)
        .find(|at| model.usable[*at]);
    if let Some(at) = next {
        view.regroup(GROUPINGS[at].0, cx);
    }
}

fn toggle_scale<V: RichHost>(view: &mut V, window: &Window, cx: &mut Context<V>) {
    if current(view, window, cx).is_none_or(|(layout, _)| !layout.strip) {
        return cx.propagate();
    }
    let rich = view.rich();
    let mode = match rich.mode {
        ScaleMode::Compressed => ScaleMode::True,
        ScaleMode::True => ScaleMode::Compressed,
    };
    rich.set_mode(mode, cx);
    cx.notify();
}

pub(in crate::window_view::viewer) fn space_keys<V: RichHost>(
    root: Stateful<Div>,
    cx: &mut Context<V>,
) -> Stateful<Div> {
    root.on_action(cx.listener(|view, _: &NextPart, window, cx| move_part(view, false, window, cx)))
        .on_action(
            cx.listener(|view, _: &PreviousPart, window, cx| move_part(view, true, window, cx)),
        )
        .on_action(
            cx.listener(|view, _: &PreviousItem, window, cx| step_item(view, false, window, cx)),
        )
        .on_action(cx.listener(|view, _: &NextItem, window, cx| step_item(view, true, window, cx)))
        .on_action(
            cx.listener(|view, _: &RegionAbove, window, cx| change_region(view, false, window, cx)),
        )
        .on_action(
            cx.listener(|view, _: &RegionBelow, window, cx| change_region(view, true, window, cx)),
        )
        .on_action(cx.listener(|view, _: &FirstBlock, window, cx| {
            step_strip(view, Step::First, window, cx);
        }))
        .on_action(
            cx.listener(|view, _: &LastBlock, window, cx| step_strip(view, Step::Last, window, cx)),
        )
        .on_action(cx.listener(|view, _: &ZoomIn, window, cx| zoom(view, false, window, cx)))
        .on_action(cx.listener(|view, _: &ZoomOut, window, cx| zoom(view, true, window, cx)))
        .on_action(cx.listener(|view, _: &PickGroup, window, cx| pick_group(view, window, cx)))
        .on_action(
            cx.listener(|view, _: &NextGrouping, window, cx| next_grouping(view, window, cx)),
        )
        .on_action(cx.listener(|view, _: &ToggleScale, window, cx| toggle_scale(view, window, cx)))
}

#[cfg(test)]
mod rebuild;
#[cfg(test)]
mod scroll;
#[cfg(test)]
mod tests;
