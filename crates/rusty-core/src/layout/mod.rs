use crate::{Direction, PaneId};

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

#[allow(dead_code, reason = "stubs until the layout is implemented")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    SideBySide,
    Stacked,
}

#[allow(dead_code, reason = "stubs until the layout is implemented")]
#[derive(Debug, Clone, PartialEq, Eq)]
enum Node {
    Leaf(PaneId),
    Split {
        axis: Axis,
        ratio: u16,
        first: Box<Node>,
        second: Box<Node>,
    },
}

#[allow(unused_variables, reason = "stubs until the layout is implemented")]
impl Layout {
    #[must_use]
    pub fn new(pane: PaneId) -> Self {
        todo!()
    }

    #[must_use]
    pub fn focused(&self) -> PaneId {
        todo!()
    }

    #[must_use]
    pub fn panes(&self) -> Vec<PaneId> {
        todo!()
    }

    #[must_use]
    pub fn contains(&self, pane: PaneId) -> bool {
        todo!()
    }

    #[must_use]
    pub fn rects(&self) -> Vec<(PaneId, Rect)> {
        todo!()
    }

    pub fn split(&mut self, direction: Direction, new_pane: PaneId) -> Result<(), LayoutError> {
        todo!()
    }

    pub fn close(&mut self, pane: PaneId) -> Result<(), LayoutError> {
        todo!()
    }

    pub fn focus(&mut self, pane: PaneId) -> Result<(), LayoutError> {
        todo!()
    }

    #[must_use]
    pub fn neighbour(&self, pane: PaneId, direction: Direction) -> Option<PaneId> {
        todo!()
    }

    pub fn focus_direction(&mut self, direction: Direction) -> Result<PaneId, LayoutError> {
        todo!()
    }

    pub fn swap(&mut self, direction: Direction) -> Result<(), LayoutError> {
        todo!()
    }

    pub fn resize(&mut self, direction: Direction, delta: u16) -> Result<(), LayoutError> {
        todo!()
    }
}

#[cfg(test)]
mod tests;
