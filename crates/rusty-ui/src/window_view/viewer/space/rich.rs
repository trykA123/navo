use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::{Bounds, FocusHandle, Focusable, ScrollHandle, Subscription, WeakEntity};
use rusty_automotive::space::{Grouping, Item, Rect, Region, ScaleMode, TOP_GROUPS, Tile, layout};

use super::keys::Part;
use super::model::RichModel;
use super::strip::{MemoryStrip, StripData, StripEvent};
use super::treemap::TreemapGroup;
use crate::prelude::*;
use crate::window_view::viewer::parts::LayoutParts;

const FLASH_STRIP: f32 = 40.;
const OTHER_STRIP: f32 = 28.;
const TILE_STEP: f32 = 8.;

type SizedTiles = ((u32, u32), Rc<[Tile]>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::window_view::viewer) enum Pick {
    Block { region: usize, block: usize },
    Group(u32),
}

pub(in crate::window_view::viewer) trait RichHost:
    'static + Sized
{
    fn rich(&mut self) -> &mut RichSpace;
    fn space_layout(&self) -> Option<LayoutParts>;
    fn table_focus(&self) -> FocusHandle;
    fn picked(&mut self, pick: Pick, cx: &mut Context<Self>);
    fn regroup(&mut self, grouping: Grouping, cx: &mut Context<Self>);
    fn enter_module(&mut self, _: u32, _: &mut Context<Self>) -> bool {
        false
    }
    fn leave_module(&mut self, _: &mut Context<Self>) -> bool {
        false
    }
}

pub(in crate::window_view::viewer) struct RichSpace {
    pub model: Option<Arc<RichModel>>,
    pub strips: Vec<Entity<MemoryStrip>>,
    pub mode: ScaleMode,
    pub grouping: Option<Grouping>,
    pub modules: Arc<[String]>,
    pub module: Option<String>,
    pub pick: Option<Pick>,
    pub part: Part,
    pub tile: Option<u32>,
    pub treemap_focus: Option<FocusHandle>,
    pub top_scroll: ScrollHandle,
    groups: Rc<[TreemapGroup]>,
    items: Vec<Item>,
    tiles: Option<SizedTiles>,
    _events: Vec<Subscription>,
}

impl Default for RichSpace {
    fn default() -> Self {
        Self {
            model: None,
            strips: Vec::new(),
            mode: ScaleMode::Compressed,
            grouping: None,
            modules: Arc::default(),
            module: None,
            pick: None,
            part: Part::Table,
            tile: None,
            treemap_focus: None,
            top_scroll: ScrollHandle::new(),
            groups: Rc::from(Vec::new()),
            items: Vec::new(),
            tiles: None,
            _events: Vec::new(),
        }
    }
}

fn on_strip<V: RichHost>(
    region: usize,
) -> impl FnMut(&mut V, Entity<MemoryStrip>, &StripEvent, &mut Context<V>) {
    move |view, _, event, cx| {
        if let StripEvent::Pick(block) = event {
            let block = *block;
            view.rich().part = Part::Strip(region);
            view.picked(Pick::Block { region, block }, cx);
        }
    }
}

fn bucket(value: Pixels) -> u32 {
    let value = f32::from(value);
    if value.is_finite() && value > 0. {
        (value / TILE_STEP).floor() as u32 * TILE_STEP as u32
    } else {
        0
    }
}

fn strip_height(model: &RichModel, at: usize) -> Pixels {
    px(if Some(at) == model.flash {
        FLASH_STRIP
    } else {
        OTHER_STRIP
    })
}

fn place(items: &[Item], (width, height): (u32, u32)) -> Rc<[Tile]> {
    let rect = Rect::new(0., 0., f64::from(width), f64::from(height));
    layout(items, rect).into()
}

fn same_group(old: &RichModel, new: &RichModel, id: u32) -> Option<u32> {
    if old.grouping != new.grouping {
        return None;
    }
    let name = &old.groups.get(id as usize)?.name;
    new.groups
        .iter()
        .take(TOP_GROUPS)
        .position(|group| group.name == *name)
        .map(|at| at as u32)
}

fn refocus_table<V: RichHost>(
    (view, lost): (&WeakEntity<V>, &[FocusHandle]),
    window: &mut Window,
    cx: &mut App,
) {
    if !lost.iter().any(|handle| handle.is_focused(window)) {
        return;
    }
    let _ = view.update(cx, |view, cx| {
        view.rich().part = Part::Table;
        window.focus(&view.table_focus(), cx);
        cx.notify();
    });
}

fn recover_focus<V: RichHost>(lost: Vec<FocusHandle>, cx: &mut Context<V>) {
    let view = cx.weak_entity();
    cx.defer(move |cx| {
        for window in cx.windows() {
            let _ = window.update(cx, |_, window, cx| {
                refocus_table((&view, &lost), window, cx);
            });
        }
    });
}

fn refresh(strip: &Entity<MemoryStrip>, (height, data): (Pixels, StripData), cx: &mut App) {
    strip.update(cx, |strip, cx| {
        strip.set_height(height, cx);
        strip.set_data(data, cx);
    });
}

impl RichSpace {
    pub fn install<V: RichHost>(
        &mut self,
        model: RichModel,
        data: impl Fn(&Arc<Region>) -> StripData,
        cx: &mut Context<V>,
    ) -> Option<u32> {
        let (groups, items) = model.treemap();
        let old = self.model.take();
        self.sync_strips(&model, data, cx);
        self.treemap_focus.get_or_insert_with(|| cx.focus_handle());
        let kept = old.as_deref().and_then(|old| self.kept_pick(old, &model));
        let tile = self
            .tile
            .zip(old.as_deref())
            .and_then(|(id, old)| same_group(old, &model, id));
        self.tile = tile.or_else(|| (self.part == Part::Treemap && !items.is_empty()).then_some(0));
        self.tiles = self.tiles.take().map(|(key, _)| (key, place(&items, key)));
        self.groups = groups;
        self.items = items;
        self.pick = kept;
        self.model = Some(Arc::new(model));
        match kept {
            Some(Pick::Group(id)) => Some(id),
            _ => None,
        }
    }

    pub fn wants_regroup(&self) -> Option<Grouping> {
        if self.module.is_some() {
            return None;
        }
        let model = self.model.as_ref()?;
        let wanted = self.grouping?;
        (model.grouping != wanted && model.usable[wanted as usize]).then_some(wanted)
    }

    fn kept_pick(&self, old: &RichModel, new: &RichModel) -> Option<Pick> {
        let pick = self.pick?;
        match pick {
            Pick::Block { region, block } => {
                let (was, now) = (old.regions.get(region)?, new.regions.get(region)?);
                (was.blocks == now.blocks && block < now.blocks.len()).then_some(pick)
            }
            Pick::Group(id) => same_group(old, new, id).map(Pick::Group),
        }
    }

    fn sync_strips<V: RichHost>(
        &mut self,
        model: &RichModel,
        data: impl Fn(&Arc<Region>) -> StripData,
        cx: &mut Context<V>,
    ) {
        let mode = self.mode;
        for (at, region) in model.regions.iter().enumerate() {
            let (height, data) = (strip_height(model, at), data(region));
            if let Some(strip) = self.strips.get(at) {
                refresh(strip, (height, data), cx);
                continue;
            }
            let strip = cx.new(|cx| {
                let mut strip = MemoryStrip::new(data, height, cx);
                strip.set_mode(mode, cx);
                strip
            });
            self._events.push(cx.subscribe(&strip, on_strip::<V>(at)));
            self.strips.push(strip);
        }
        let keep = model.regions.len();
        let lost: Vec<FocusHandle> = self
            .strips
            .split_off(keep)
            .iter()
            .map(|strip| strip.focus_handle(cx))
            .collect();
        self._events.truncate(keep);
        if matches!(self.part, Part::Strip(at) if at >= keep) {
            self.part = Part::Table;
        }
        if !lost.is_empty() {
            recover_focus(lost, cx);
        }
    }

    pub fn set_mode(&mut self, mode: ScaleMode, cx: &mut App) {
        self.mode = mode;
        for strip in &self.strips {
            strip.update(cx, |strip, cx| strip.set_mode(mode, cx));
        }
    }

    pub fn set_pick(&mut self, pick: Option<Pick>, lit: &[Option<Rc<[bool]>>], cx: &mut App) {
        self.pick = pick;
        for (at, strip) in self.strips.iter().enumerate() {
            let caret = match pick {
                Some(Pick::Block { region, block }) if region == at => Some(block),
                _ => None,
            };
            let lit = lit.get(at).cloned().flatten();
            strip.update(cx, |strip, cx| {
                strip.set_caret(caret, cx);
                strip.set_lit(lit, cx);
            });
        }
    }

    pub fn groups(&self) -> Rc<[TreemapGroup]> {
        self.groups.clone()
    }

    pub fn tiles(&self) -> Option<Rc<[Tile]>> {
        self.tiles.as_ref().map(|(_, tiles)| tiles.clone())
    }

    pub fn measure_treemap(&mut self, bounds: Bounds<Pixels>) -> bool {
        let key = (bucket(bounds.size.width), bucket(bounds.size.height));
        if self.tiles.as_ref().is_some_and(|(at, _)| *at == key) {
            return false;
        }
        self.tiles = Some((key, place(&self.items, key)));
        true
    }
}
