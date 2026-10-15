use crate::PaneId;

pub(super) const MIN_RATIO: u16 = 100;
pub(super) const MAX_RATIO: u16 = 900;
const HALF: u16 = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Axis {
    SideBySide,
    Stacked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Side {
    First,
    Second,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Node {
    Leaf(PaneId),
    Split {
        axis: Axis,
        ratio: u16,
        first: Box<Node>,
        second: Box<Node>,
    },
}

impl Node {
    pub(super) fn leaves(&self, out: &mut Vec<PaneId>) {
        match self {
            Self::Leaf(pane) => out.push(*pane),
            Self::Split { first, second, .. } => {
                first.leaves(out);
                second.leaves(out);
            }
        }
    }

    pub(super) fn first_leaf(&self) -> PaneId {
        match self {
            Self::Leaf(pane) => *pane,
            Self::Split { first, .. } => first.first_leaf(),
        }
    }

    pub(super) fn path_to(&self, pane: PaneId) -> Option<Vec<Side>> {
        match self {
            Self::Leaf(leaf) => (*leaf == pane).then(Vec::new),
            Self::Split { first, second, .. } => {
                let (side, mut path) = first
                    .path_to(pane)
                    .map(|p| (Side::First, p))
                    .or_else(|| second.path_to(pane).map(|p| (Side::Second, p)))?;
                path.insert(0, side);
                Some(path)
            }
        }
    }

    pub(super) fn at_mut(&mut self, path: &[Side]) -> Option<&mut Self> {
        let Some((side, rest)) = path.split_first() else {
            return Some(self);
        };
        match self {
            Self::Leaf(_) => None,
            Self::Split { first, second, .. } => match side {
                Side::First => first.at_mut(rest),
                Side::Second => second.at_mut(rest),
            },
        }
    }

    pub(super) fn ancestors(&self, path: &[Side]) -> Vec<(usize, Axis, Side)> {
        let mut out = Vec::with_capacity(path.len());
        let mut node = self;
        for (depth, side) in path.iter().enumerate() {
            let Self::Split {
                axis,
                first,
                second,
                ..
            } = node
            else {
                break;
            };
            out.push((depth, *axis, *side));
            node = match side {
                Side::First => first,
                Side::Second => second,
            };
        }
        out
    }

    pub(super) fn split_leaf(&mut self, axis: Axis, new_side: Side, new_pane: PaneId) {
        let old = std::mem::replace(self, Self::Leaf(new_pane));
        let new = Box::new(Self::Leaf(new_pane));
        let (first, second) = match new_side {
            Side::First => (new, Box::new(old)),
            Side::Second => (Box::new(old), new),
        };
        *self = Self::Split {
            axis,
            ratio: HALF,
            first,
            second,
        };
    }

    pub(super) fn replace_with_child(&mut self, keep: Side) {
        let placeholder = Self::Leaf(PaneId::new(0));
        if let Self::Split { first, second, .. } = std::mem::replace(self, placeholder) {
            *self = match keep {
                Side::First => *first,
                Side::Second => *second,
            };
        }
    }

    pub(super) fn exchange(&mut self, a: PaneId, b: PaneId) {
        match self {
            Self::Leaf(pane) if *pane == a => *pane = b,
            Self::Leaf(pane) if *pane == b => *pane = a,
            Self::Leaf(_) => {}
            Self::Split { first, second, .. } => {
                first.exchange(a, b);
                second.exchange(a, b);
            }
        }
    }

    pub(super) fn shift_ratio(&mut self, grow_first: bool, delta: u16) {
        if let Self::Split { ratio, .. } = self {
            *ratio = if grow_first {
                ratio.saturating_add(delta).min(MAX_RATIO)
            } else {
                ratio.saturating_sub(delta).max(MIN_RATIO)
            };
        }
    }
}
