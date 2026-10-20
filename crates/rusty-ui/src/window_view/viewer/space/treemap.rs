use std::collections::HashMap;
use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{RenderOnce, Role, StatefulInteractiveElement, relative};
use rusty_automotive::space::{KIND_BAR_PX, Kind, Rect, Target, Tile};
use rusty_l10n::t;

use crate::format;
use crate::prelude::*;
use crate::tokens::{self, MemKind, Theme};

const GAP: f32 = 1.;
const SELECTED_WIDTH: f32 = 2.;

#[derive(Clone, Debug, PartialEq)]
pub struct TreemapPart {
    pub id: u32,
    pub name: SharedString,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TreemapGroup {
    pub id: u32,
    pub name: SharedString,
    pub by_kind: [u64; Kind::COUNT],
    pub parts: Vec<TreemapPart>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreemapEvent {
    Pick(u32),
}

type OnEvent = Rc<dyn Fn(TreemapEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Treemap {
    id: ElementId,
    groups: Rc<[TreemapGroup]>,
    tiles: Rc<[Tile]>,
    total: u64,
    selected: Option<u32>,
    stripped: bool,
    on_event: Option<OnEvent>,
}

impl Treemap {
    pub fn new(id: impl Into<ElementId>, groups: Rc<[TreemapGroup]>, tiles: Rc<[Tile]>) -> Self {
        let total = tiles.iter().map(|tile| tile.value).sum();
        Self {
            id: id.into(),
            groups,
            tiles,
            total,
            selected: None,
            stripped: false,
            on_event: None,
        }
    }

    pub fn selected(mut self, group: Option<u32>) -> Self {
        self.selected = group;
        self
    }

    pub fn stripped(mut self, stripped: bool) -> Self {
        self.stripped = stripped;
        self
    }

    pub fn on_event(mut self, f: impl Fn(TreemapEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_event = Some(Rc::new(f));
        self
    }
}

fn dominant(by_kind: &[u64; Kind::COUNT]) -> usize {
    let mut best = 0;
    for (index, bytes) in by_kind.iter().enumerate() {
        if *bytes > by_kind[best] {
            best = index;
        }
    }
    best
}

fn colours(t: &Theme, group: Option<&TreemapGroup>, more: bool) -> (Hsla, Hsla) {
    if more {
        return (t.mem.free, t.colors.muted);
    }
    let kind: MemKind = group.map_or(t.mem.other, |g| t.mem.kinds()[dominant(&g.by_kind)].1);
    (kind.tint, kind.text)
}

fn pick_of(tile: &Tile) -> Option<u32> {
    match tile.target {
        Target::Item(id) => Some(id),
        Target::More { .. } => None,
    }
}

fn share(value: u64, total: u64) -> String {
    let percent = if total == 0 {
        0.
    } else {
        value as f64 * 100. / total as f64
    };
    t!("viewer-space-share", percent = format!("{percent:.1}"))
}

fn caption(tile: &Tile, name: &str, total: u64) -> Option<(String, String)> {
    tile.text.then(|| {
        let detail = format!(
            "{} · {}",
            format::size(tile.value),
            share(tile.value, total)
        );
        (name.to_owned(), detail)
    })
}

fn place(rect: Rect) -> Div {
    div()
        .absolute()
        .left(px(rect.x as f32))
        .top(px(rect.y as f32))
        .w(px((rect.w as f32 - GAP).max(0.)))
        .h(px((rect.h as f32 - GAP).max(0.)))
        .overflow_hidden()
}

fn label(t: &Theme, color: Hsla, (name, detail): (String, String), detail_line: bool) -> Div {
    let head = div()
        .truncate()
        .text_size(t.typography.small)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color)
        .child(name);
    let body = div().flex().flex_col().px(t.spacing.sm).py(t.spacing.xs);
    if detail_line {
        body.child(head).child(
            div()
                .truncate()
                .text_size(t.typography.tiny)
                .text_color(color)
                .child(detail),
        )
    } else {
        body.child(head)
    }
}

fn kind_bar(t: &Theme, rect: Rect, by_kind: &[u64; Kind::COUNT]) -> Div {
    let total: u64 = by_kind.iter().sum();
    let kinds = t.mem.kinds();
    let bar = div()
        .absolute()
        .left_0()
        .bottom_0()
        .w_full()
        .h(px(KIND_BAR_PX as f32))
        .flex();
    if total == 0 || rect.h < f64::from(2. * KIND_BAR_PX as f32) {
        return bar;
    }
    by_kind
        .iter()
        .zip(kinds)
        .filter(|(bytes, _)| **bytes > 0)
        .fold(bar, |bar, (bytes, (_, kind))| {
            bar.child(
                div()
                    .h_full()
                    .w(relative(*bytes as f32 / total as f32))
                    .bg(kind.strong),
            )
        })
}

fn name_of(tile: &Tile, group: Option<&TreemapGroup>, part_of: bool) -> String {
    match (tile.target, group) {
        (Target::More { count }, _) => t!("viewer-space-more", count = count as i64),
        (Target::Item(id), Some(group)) if part_of => group
            .parts
            .iter()
            .find(|part| part.id == id)
            .map(|part| part.name.to_string())
            .unwrap_or_default(),
        (_, Some(group)) if !part_of => group.name.to_string(),
        _ => String::new(),
    }
}

impl Treemap {
    fn tile(&self, t: &Theme, tile: &Tile, group: Option<&TreemapGroup>) -> AnyElement {
        let more = matches!(tile.target, Target::More { .. });
        let (bg, ink) = colours(t, group, more);
        let name = name_of(tile, group, false);
        let nested = !tile.inner.is_empty();
        let mut cell = place(tile.rect).bg(bg).rounded(t.radii.xs);
        if more {
            cell = cell.border_1().border_color(t.mem.free_line);
        }
        if let Some(text) = caption(tile, &name, self.total) {
            cell = cell.child(label(t, ink, text, !nested));
        }
        if let (Some(group), false) = (group, more) {
            cell = cell.child(kind_bar(t, tile.rect, &group.by_kind));
        }
        let origin = tile.rect;
        let cell = tile.inner.iter().fold(cell, |cell, inner| {
            let local = Rect::new(
                inner.rect.x - origin.x,
                inner.rect.y - origin.y,
                inner.rect.w,
                inner.rect.h,
            );
            let name = name_of(inner, group, true);
            cell.child(self.inner_tile(t, inner, (local, name, ink)))
        });
        let id = pick_of(tile).map_or(usize::MAX, |id| id as usize);
        let described = format!(
            "{name}, {} · {}",
            format::size(tile.value),
            share(tile.value, self.total)
        );
        let on_event = self.on_event.clone();
        cell.id(("treemap-tile", id))
            .role(Role::Button)
            .aria_label(described)
            .test_support()
            .when_some(pick_of(tile), |cell, id| {
                cell.cursor_pointer().on_click(move |_, window, cx| {
                    emit(&on_event, TreemapEvent::Pick(id), window, cx);
                })
            })
            .into_any_element()
    }

    fn inner_tile(&self, t: &Theme, tile: &Tile, (local, name, ink): (Rect, String, Hsla)) -> Div {
        let mut cell = place(local).bg(t.colors.surface).rounded(t.radii.xs);
        if let Some(text) = caption(tile, &name, self.total) {
            cell = cell.child(label(t, ink, text, false));
        }
        cell
    }
}

fn emit(on_event: &Option<OnEvent>, event: TreemapEvent, window: &mut Window, cx: &mut App) {
    if let Some(f) = on_event {
        f(event, window, cx);
    }
}

impl RenderOnce for Treemap {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let root = div()
            .id(self.id.clone())
            .relative()
            .size_full()
            .overflow_hidden();
        if self.stripped {
            return root
                .flex()
                .items_center()
                .justify_center()
                .p(t.spacing.xl)
                .text_size(t.typography.small)
                .text_color(t.colors.muted)
                .child(t!("viewer-space-stripped"));
        }
        let by_id: HashMap<u32, &TreemapGroup> =
            self.groups.iter().map(|group| (group.id, group)).collect();
        let tiles = self.tiles.iter().map(|tile| {
            let group = pick_of(tile).and_then(|id| by_id.get(&id).copied());
            self.tile(&t, tile, group)
        });
        let outline = self
            .tiles
            .iter()
            .find(|tile| pick_of(tile) == self.selected && self.selected.is_some());
        root.children(tiles).when_some(outline, |root, tile| {
            root.child(
                place(tile.rect)
                    .border(px(SELECTED_WIDTH))
                    .border_color(t.colors.accent)
                    .rounded(t.radii.xs),
            )
        })
    }
}

#[cfg(test)]
mod tests;
