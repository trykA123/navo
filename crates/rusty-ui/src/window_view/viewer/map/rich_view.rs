use super::rich::block_name;
use super::select::Table;
use super::text::{Check, span};
use super::{Index, MapView};
use crate::prelude::*;
use crate::tokens::Theme;
use crate::window_view::viewer::space::{
    Pick, Picked, pick_facts, share_cell, share_head, share_of,
};

impl MapView {
    fn shows_share(&self) -> bool {
        self.is_rich() && self.layout.row_bars && self.table() == Table::Sections
    }

    pub(super) fn share_head(&self, t: &Theme) -> Option<Div> {
        self.shows_share().then(|| share_head(t))
    }

    pub(super) fn share_cell(&self, t: &Theme, at: usize) -> Option<Div> {
        if !self.shows_share() {
            return None;
        }
        let section = self.loaded()?.model.sections.get(at)?;
        let share = self
            .rich
            .model
            .as_ref()
            .and_then(|model| share_of(&model.regions, (section.address, section.size)));
        Some(share_cell(t, share))
    }

    pub(super) fn pick_name(&self) -> Option<String> {
        let (Index::Ready(loaded), Some(model)) = (&self.index, &self.rich.model) else {
            return None;
        };
        match self.rich.pick? {
            Pick::Block { region, block } => {
                Some(block_name(loaded, model.regions.get(region)?, block))
            }
            Pick::Group(id) => Some(model.groups.get(id as usize)?.name.clone()),
        }
    }

    pub(super) fn rich_checks(&self) -> (Option<Check>, Option<Check>) {
        let picked = self
            .pick_name()
            .filter(|_| self.is_rich())
            .map(|text| Check {
                text,
                is_bad: false,
            });
        let outside = self
            .rich
            .model
            .as_ref()
            .and_then(|model| model.outside_note())
            .map(|text| Check { text, is_bad: true });
        (picked, outside)
    }

    pub(super) fn pick_body(&self, t: &Theme) -> Option<Vec<AnyElement>> {
        let picked = Picked {
            model: self.rich.model.as_ref()?,
            pick: self.rich.pick?,
            name: self.pick_name()?,
            name_id: "map-inspector-name",
        };
        pick_facts(t, picked, &span)
    }
}
