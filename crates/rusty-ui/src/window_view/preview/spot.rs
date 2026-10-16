use rusty_core::{Location, ProviderPath};
use rusty_structured::NodeId;

use super::contents::Place;
use super::folder::FolderPreview;
use super::summary::LARGEST_AT;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::window_view) enum Spot {
    Tree(NodeId),
    Largest(usize),
}

pub(in crate::window_view) enum Restore {
    First,
    Path(Vec<String>),
}

pub(in crate::window_view) enum Carry {
    Path(Vec<String>),
    Largest(usize),
}

pub(super) enum Fold {
    Toggle(usize),
    Step,
    Parent(usize),
}

fn step(at: Option<usize>, delta: isize, total: usize) -> Option<usize> {
    let last = total.checked_sub(1)?;
    Some(match at {
        None => 0,
        Some(at) => at.min(last).saturating_add_signed(delta).min(last),
    })
}

fn largest_place(base: &Location, path: &str) -> Option<Place> {
    let mut parts: Vec<&str> = path.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
    let name = parts.pop()?.to_owned();
    let folder = parts.into_iter().try_fold(base.clone(), |at, part| {
        at.join(&ProviderPath::from_text(part)).ok()
    })?;
    Some(Place {
        folder,
        name,
        is_folder: false,
    })
}

impl Place {
    pub(super) fn location(&self) -> Option<Location> {
        self.folder.join(&ProviderPath::from_text(&self.name)).ok()
    }
}

impl FolderPreview {
    fn largest_len(&self) -> usize {
        self.scan.profile.as_ref().map_or(0, |p| p.largest.len())
    }

    pub(in crate::window_view) fn spot_row(&self) -> Option<usize> {
        match self.spot? {
            Spot::Tree(node) => self.contents.index_of(node),
            Spot::Largest(_) => None,
        }
    }

    fn flat(&self) -> Option<usize> {
        let tree = self.contents.len();
        match self.spot? {
            Spot::Tree(node) => self.contents.index_of(node),
            Spot::Largest(rank) => Some(tree + rank.min(self.largest_len().saturating_sub(1))),
        }
    }

    fn spot_at(&self, at: usize) -> Option<Spot> {
        let tree = self.contents.len();
        if at < tree {
            self.contents.node_at(at).map(Spot::Tree)
        } else {
            Some(Spot::Largest(at - tree))
        }
    }

    pub(super) fn step_by(&mut self, delta: isize) {
        let total = self.contents.len() + self.largest_len();
        let spot = step(self.flat(), delta, total).and_then(|at| self.spot_at(at));
        self.move_to(spot);
    }

    pub(super) fn move_to_row(&mut self, index: usize) {
        let spot = self.spot_at(index);
        self.move_to(spot);
    }

    fn move_to(&mut self, spot: Option<Spot>) {
        self.spot = spot;
        match (spot, self.spot_row()) {
            (Some(Spot::Tree(_)), Some(index)) => self.tree_scroll.scroll_to(index),
            (Some(Spot::Largest(_)), _) => self.summary_scroll.scroll_to_item(LARGEST_AT),
            _ => {}
        }
    }

    pub(super) fn want_spot(&mut self) {
        if self.spot.is_none() && self.restore.is_none() {
            self.restore = Some(Restore::First);
        }
        self.settle_spot();
    }

    pub(super) fn settle_spot(&mut self) {
        if self.spot.is_some() || !self.contents.is_read() {
            return;
        }
        self.spot = match self.restore.take() {
            Some(Restore::Path(path)) => self
                .contents
                .find(&path)
                .or_else(|| self.contents.node_at(0))
                .map(Spot::Tree),
            Some(Restore::First) => self.contents.node_at(0).map(Spot::Tree),
            None => None,
        };
    }

    pub(super) fn carry(&self) -> Option<Carry> {
        match self.spot? {
            Spot::Tree(node) => Some(Carry::Path(self.contents.path_of(node))),
            Spot::Largest(rank) => Some(Carry::Largest(rank)),
        }
    }

    pub(super) fn adopt(&mut self, carry: Carry) {
        match carry {
            Carry::Path(path) => self.restore = Some(Restore::Path(path)),
            Carry::Largest(rank) => self.spot = Some(Spot::Largest(rank)),
        }
        self.settle_spot();
    }

    pub(super) fn place(&self) -> Option<Place> {
        match self.spot? {
            Spot::Tree(_) => self.contents.place(self.spot_row()?),
            Spot::Largest(rank) => {
                let file = self.scan.profile.as_ref()?.largest.get(rank)?;
                largest_place(&self.target.location, &file.path)
            }
        }
    }

    pub(super) fn fold_for(&self, is_opening: bool) -> Option<Fold> {
        let index = self.spot_row()?;
        let (_, shape) = self.contents.row(index)?;
        match (is_opening, shape.can_open, shape.is_open) {
            (true, true, false) | (false, true, true) => Some(Fold::Toggle(index)),
            (true, true, true) => Some(Fold::Step),
            (false, _, _) => self.contents.parent_row(index).map(Fold::Parent),
            (true, false, _) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::step;

    #[test]
    fn a_step_moves_through_one_flat_list_and_stops_at_both_ends() {
        assert_eq!(step(None, 1, 5), Some(0));
        assert_eq!(step(Some(2), 1, 5), Some(3));
        assert_eq!(step(Some(3), -1, 5), Some(2));
        assert_eq!(step(Some(4), 1, 5), Some(4));
        assert_eq!(step(Some(0), -1, 5), Some(0));
        assert_eq!(step(Some(9), -1, 5), Some(3));
        assert_eq!(step(None, 1, 0), None);
    }
}
