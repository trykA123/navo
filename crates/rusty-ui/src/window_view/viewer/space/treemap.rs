use std::collections::HashMap;
use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{FocusHandle, RenderOnce, Role, StatefulInteractiveElement, relative};
use rusty_automotive::space::{KIND_BAR_PX, Kind, Rect, Target, Tile};
use rusty_l10n::t;

use self::palette::{GroupColours, Shade, palette};
use crate::format;
use crate::prelude::*;
use crate::tokens::{self, Theme};

mod palette;

const GAP: f32 = 1.;
const SELECTED_WIDTH: f32 = 2.;
const CURSOR_INSET: f64 = 3.;

#[derive(Clone, Debug, PartialEq)]
pub struct TreemapPart {
    pub id: u32,
    pub name: SharedString,
    pub by_kind: [u64; Kind::COUNT],
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
    focus: Option<FocusHandle>,
    cursor: Option<u32>,
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
            focus: None,
            cursor: None,
            on_event: None,
        }
    }

    pub fn selected(mut self, group: Option<u32>) -> Self {
        self.selected = group;
        self
    }

    pub fn focus(mut self, focus: Option<FocusHandle>, cursor: Option<u32>) -> Self {
        self.focus = focus;
        self.cursor = cursor;
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

fn shade_of(
    t: &Theme,
    tile: &Tile,
    (colours, rank): (Option<&GroupColours>, Option<usize>),
) -> Shade {
    if matches!(tile.target, Target::More { .. }) {
        return Shade {
            bg: t.mem.free,
            ink: t.colors.muted,
        };
    }
    match (colours, rank) {
        (Some(colours), Some(rank)) => colours.part(rank),
        (Some(colours), None) => colours.base(),
        (None, _) => Shade {
            bg: t.mem.other.tint,
            ink: t.mem.other.text,
        },
    }
}

fn group_index(groups: &[TreemapGroup], id: u32) -> Option<usize> {
    groups.iter().position(|group| group.id == id)
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

fn inner_name(name: String, parent: &str) -> Option<String> {
    (name != parent).then_some(name)
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

fn part_of(group: Option<&TreemapGroup>, id: u32) -> Option<&TreemapPart> {
    group?.parts.iter().find(|part| part.id == id)
}

fn name_of(tile: &Tile, group: Option<&TreemapGroup>, inner: bool) -> String {
    match (tile.target, group) {
        (Target::More { count }, _) => t!("viewer-space-more", count = count as i64),
        (Target::Item(id), group) if inner => part_of(group, id)
            .map(|part| part.name.to_string())
            .unwrap_or_default(),
        (_, Some(group)) if !inner => group.name.to_string(),
        _ => String::new(),
    }
}

struct Parent<'a> {
    group: Option<&'a TreemapGroup>,
    colours: Option<&'a GroupColours>,
    origin: Rect,
    name: &'a str,
}

impl Treemap {
    fn tile(
        &self,
        t: &Theme,
        tile: &Tile,
        (group, colours): (Option<&TreemapGroup>, Option<&GroupColours>),
    ) -> AnyElement {
        let more = matches!(tile.target, Target::More { .. });
        let shade = shade_of(t, tile, (colours, None));
        let name = name_of(tile, group, false);
        let nested = !tile.inner.is_empty();
        let mut cell = place(tile.rect).bg(shade.bg).rounded(t.radii.xs);
        if more {
            cell = cell.border_1().border_color(t.mem.free_line);
        }
        if let Some(text) = caption(tile, &name, self.total) {
            cell = cell.child(label(t, shade.ink, text, !nested));
        }
        if let (Some(group), false) = (group, more) {
            cell = cell.child(kind_bar(t, tile.rect, &group.by_kind));
        }
        let parent = Parent {
            group,
            colours,
            origin: tile.rect,
            name: &name,
        };
        let cell = tile
            .inner
            .iter()
            .enumerate()
            .fold(cell, |cell, (rank, inner)| {
                cell.child(self.inner_tile(t, inner, (&parent, rank)))
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

    fn inner_tile(&self, t: &Theme, tile: &Tile, (parent, rank): (&Parent<'_>, usize)) -> Div {
        let shade = shade_of(t, tile, (parent.colours, Some(rank)));
        let local = Rect::new(
            tile.rect.x - parent.origin.x,
            tile.rect.y - parent.origin.y,
            tile.rect.w,
            tile.rect.h,
        );
        let mut cell = place(local).bg(shade.bg).rounded(t.radii.xs);
        let name = inner_name(name_of(tile, parent.group, true), parent.name);
        if let Some(text) = name.and_then(|name| caption(tile, &name, self.total)) {
            cell = cell.child(label(t, shade.ink, text, true));
        }
        match tile.target {
            Target::More { .. } => cell.border_1().border_color(t.mem.free_line),
            Target::Item(id) => match part_of(parent.group, id) {
                Some(part) => cell.child(kind_bar(t, local, &part.by_kind)),
                None => cell,
            },
        }
    }
}

fn emit(on_event: &Option<OnEvent>, event: TreemapEvent, window: &mut Window, cx: &mut App) {
    if let Some(f) = on_event {
        f(event, window, cx);
    }
}

impl Treemap {
    fn cursor_ring(&self, t: &Theme, window: &Window) -> Option<Div> {
        self.focus
            .as_ref()
            .filter(|focus| focus.is_focused(window))?;
        let tile = self
            .tiles
            .iter()
            .find(|tile| self.cursor.is_some() && pick_of(tile) == self.cursor)?;
        let inset = CURSOR_INSET.min(tile.rect.w / 2.).min(tile.rect.h / 2.);
        let rect = Rect::new(
            tile.rect.x + inset,
            tile.rect.y + inset,
            tile.rect.w - 2. * inset,
            tile.rect.h - 2. * inset,
        );
        Some(
            place(rect)
                .rounded(t.radii.xs)
                .shadow(t.shadows.focus.clone()),
        )
    }
}

impl RenderOnce for Treemap {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let root = div()
            .id(self.id.clone())
            .relative()
            .size_full()
            .overflow_hidden()
            .when_some(self.focus.clone(), |root, focus| {
                root.track_focus(&focus)
                    .key_context(super::keys::TREEMAP_CONTEXT)
            });
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
        let colours = palette(self.groups.len(), t.mode);
        let tiles = self.tiles.iter().map(|tile| {
            let id = pick_of(tile);
            let group = id.and_then(|id| by_id.get(&id).copied());
            let hue = id.and_then(|id| colours.get(group_index(&self.groups, id)?));
            self.tile(&t, tile, (group, hue))
        });
        let outline = self
            .tiles
            .iter()
            .find(|tile| pick_of(tile) == self.selected && self.selected.is_some());
        let ring = self.cursor_ring(&t, window);
        root.children(tiles)
            .when_some(outline, |root, tile| {
                root.child(
                    place(tile.rect)
                        .border(px(SELECTED_WIDTH))
                        .border_color(t.colors.accent)
                        .rounded(t.radii.xs),
                )
            })
            .children(ring)
    }
}

#[cfg(test)]
mod tests;
