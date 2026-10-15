mod geometry;
mod tree;

use crate::session::LayoutState;
use crate::{Direction, PaneId};
use tree::{Axis, Node, Side};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    root: Node,
    focused: PaneId,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LayoutError {
    #[error("pane {} is not in the layout", .0.get())]
    UnknownPane(PaneId),
    #[error("pane {} is already in the layout", .0.get())]
    DuplicatePane(PaneId),
    #[error("the last pane cannot be closed")]
    LastPane,
    #[error("no pane to the {}", .0.keyword())]
    NoNeighbour(Direction),
    #[error("no split to resize towards {}", .0.keyword())]
    NoSplitToResize(Direction),
}

const UNIT: Rect = Rect {
    x: 0.0,
    y: 0.0,
    width: 1.0,
    height: 1.0,
};

impl Layout {
    #[must_use]
    pub fn new(pane: PaneId) -> Self {
        Self {
            root: Node::Leaf(pane),
            focused: pane,
        }
    }

    #[must_use]
    pub fn focused(&self) -> PaneId {
        self.focused
    }

    #[must_use]
    pub fn panes(&self) -> Vec<PaneId> {
        let mut out = Vec::new();
        self.root.leaves(&mut out);
        out
    }

    #[must_use]
    pub fn contains(&self, pane: PaneId) -> bool {
        self.root.path_to(pane).is_some()
    }

    #[must_use]
    pub fn rects(&self) -> Vec<(PaneId, Rect)> {
        let mut out = Vec::new();
        geometry::rects(&self.root, UNIT, &mut out);
        out
    }

    pub fn split(&mut self, direction: Direction, new_pane: PaneId) -> Result<(), LayoutError> {
        if self.contains(new_pane) {
            return Err(LayoutError::DuplicatePane(new_pane));
        }
        let path = self.path_to(self.focused)?;
        let leaf = self
            .root
            .at_mut(&path)
            .ok_or(LayoutError::UnknownPane(self.focused))?;
        let side = if grows_first(direction) {
            Side::Second
        } else {
            Side::First
        };
        leaf.split_leaf(axis_of(direction), side, new_pane);
        self.focused = new_pane;
        Ok(())
    }

    pub fn close(&mut self, pane: PaneId) -> Result<(), LayoutError> {
        let path = self.path_to(pane)?;
        let Some((side, parent_path)) = path.split_last() else {
            return Err(LayoutError::LastPane);
        };
        let parent = self
            .root
            .at_mut(parent_path)
            .ok_or(LayoutError::UnknownPane(pane))?;
        let keep = match side {
            Side::First => Side::Second,
            Side::Second => Side::First,
        };
        parent.replace_with_child(keep);
        if self.focused == pane {
            self.focused = parent.first_leaf();
        }
        Ok(())
    }

    pub fn focus(&mut self, pane: PaneId) -> Result<(), LayoutError> {
        self.path_to(pane)?;
        self.focused = pane;
        Ok(())
    }

    #[must_use]
    pub fn neighbour(&self, pane: PaneId, direction: Direction) -> Option<PaneId> {
        geometry::neighbour(&self.rects(), pane, direction)
    }

    pub fn focus_direction(&mut self, direction: Direction) -> Result<PaneId, LayoutError> {
        let next = self
            .neighbour(self.focused, direction)
            .ok_or(LayoutError::NoNeighbour(direction))?;
        self.focused = next;
        Ok(next)
    }

    pub fn swap(&mut self, direction: Direction) -> Result<(), LayoutError> {
        let other = self
            .neighbour(self.focused, direction)
            .ok_or(LayoutError::NoNeighbour(direction))?;
        self.root.exchange(self.focused, other);
        Ok(())
    }

    pub fn resize(&mut self, direction: Direction, delta: u16) -> Result<(), LayoutError> {
        let path = self.path_to(self.focused)?;
        let axis = axis_of(direction);
        let grow_side = if grows_first(direction) {
            Side::First
        } else {
            Side::Second
        };
        let matching: Vec<_> = self
            .root
            .ancestors(&path)
            .into_iter()
            .filter(|(_, a, _)| *a == axis)
            .collect();
        let depth = matching
            .iter()
            .rev()
            .find(|(_, _, side)| *side == grow_side)
            .or_else(|| matching.last())
            .map(|(depth, _, _)| *depth)
            .ok_or(LayoutError::NoSplitToResize(direction))?;
        let split = path
            .get(..depth)
            .and_then(|p| self.root.at_mut(p))
            .ok_or(LayoutError::NoSplitToResize(direction))?;
        split.shift_ratio(grows_first(direction), delta);
        Ok(())
    }

    pub(crate) fn to_state(&self) -> LayoutState {
        self.root.to_state()
    }

    pub(crate) fn from_state(state: &LayoutState, focused: PaneId) -> Result<Self, String> {
        let layout = Self {
            root: Node::from_state(state)?,
            focused,
        };
        let mut panes = layout.panes();
        if !panes.contains(&focused) {
            return Err(format!(
                "focused pane {} is not in the layout",
                focused.get()
            ));
        }
        panes.sort_unstable();
        if panes.windows(2).any(|w| w[0] == w[1]) {
            return Err("the layout names a pane twice".to_owned());
        }
        Ok(layout)
    }

    fn path_to(&self, pane: PaneId) -> Result<Vec<Side>, LayoutError> {
        self.root
            .path_to(pane)
            .ok_or(LayoutError::UnknownPane(pane))
    }
}

fn axis_of(direction: Direction) -> Axis {
    if direction.is_horizontal() {
        Axis::SideBySide
    } else {
        Axis::Stacked
    }
}

fn grows_first(direction: Direction) -> bool {
    matches!(direction, Direction::Right | Direction::Down)
}

#[cfg(test)]
mod tests;
