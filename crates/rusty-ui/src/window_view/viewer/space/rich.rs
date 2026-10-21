use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::{Bounds, FocusHandle, Subscription};
use rusty_automotive::space::{Grouping, Item, Rect, Region, ScaleMode, Tile, layout};

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
}

pub(in crate::window_view::viewer) struct RichSpace {
    pub model: Option<Arc<RichModel>>,
    pub strips: Vec<Entity<MemoryStrip>>,
    pub mode: ScaleMode,
    pub grouping: Option<Grouping>,
    pub pick: Option<Pick>,
    pub part: Part,
    pub tile: Option<u32>,
    pub treemap_focus: Option<FocusHandle>,
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
            pick: None,
            part: Part::Table,
            tile: None,
            treemap_focus: None,
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

impl RichSpace {
    pub fn install<V: RichHost>(
        &mut self,
        model: RichModel,
        data: impl Fn(&Arc<Region>) -> StripData,
        cx: &mut Context<V>,
    ) {
        let (groups, items) = model.treemap();
        let same = self.strips.len() == model.regions.len()
            && self
                .model
                .as_ref()
                .is_some_and(|old| old.flash == model.flash);
        if same {
            for (strip, region) in self.strips.iter().zip(&model.regions) {
                let data = data(region);
                strip.update(cx, |strip, cx| strip.set_data(data, cx));
            }
        } else {
            self.make_strips(&model, data, cx);
        }
        self.treemap_focus.get_or_insert_with(|| cx.focus_handle());
        self.groups = groups;
        self.items = items;
        self.tiles = None;
        self.pick = None;
        self.tile = None;
        self.model = Some(Arc::new(model));
    }

    fn make_strips<V: RichHost>(
        &mut self,
        model: &RichModel,
        data: impl Fn(&Arc<Region>) -> StripData,
        cx: &mut Context<V>,
    ) {
        let flash = model.flash;
        let mode = self.mode;
        self.strips = model
            .regions
            .iter()
            .enumerate()
            .map(|(at, region)| {
                let height = if Some(at) == flash {
                    FLASH_STRIP
                } else {
                    OTHER_STRIP
                };
                let data = data(region);
                cx.new(|cx| {
                    let mut strip = MemoryStrip::new(data, px(height), cx);
                    strip.set_mode(mode, cx);
                    strip
                })
            })
            .collect();
        self._events = self
            .strips
            .iter()
            .enumerate()
            .map(|(region, strip)| cx.subscribe(strip, on_strip::<V>(region)))
            .collect();
        self.part = Part::Table;
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
        let rect = Rect::new(0., 0., f64::from(key.0), f64::from(key.1));
        self.tiles = Some((key, layout(&self.items, rect).into()));
        true
    }
}
