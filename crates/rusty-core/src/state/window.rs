use std::collections::BTreeMap;

use super::{Pane, StateError};
use crate::{Direction, Layout, PaneId, WindowId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    id: WindowId,
    layout: Layout,
    focused: Pane,
    others: BTreeMap<PaneId, Pane>,
}

impl Window {
    #[must_use]
    pub fn id(&self) -> WindowId {
        self.id
    }

    #[must_use]
    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    #[must_use]
    pub fn pane(&self, id: PaneId) -> Option<&Pane> {
        if self.focused.id() == id {
            Some(&self.focused)
        } else {
            self.others.get(&id)
        }
    }

    #[must_use]
    pub fn panes(&self) -> Vec<&Pane> {
        self.layout
            .panes()
            .into_iter()
            .filter_map(|id| self.pane(id))
            .collect()
    }

    pub(crate) fn restore(id: WindowId, layout: Layout, panes: Vec<Pane>) -> Result<Self, String> {
        let mut others: BTreeMap<PaneId, Pane> = panes.into_iter().map(|p| (p.id(), p)).collect();
        let mut wanted = layout.panes();
        wanted.sort_unstable();
        if others.keys().copied().collect::<Vec<_>>() != wanted {
            return Err(format!(
                "window {} lists other panes than its layout",
                id.get()
            ));
        }
        let focused = others
            .remove(&layout.focused())
            .ok_or_else(|| format!("window {} has no focused pane", id.get()))?;
        Ok(Self {
            id,
            layout,
            focused,
            others,
        })
    }

    pub(super) fn new(id: WindowId, pane: Pane) -> Self {
        Self {
            id,
            layout: Layout::new(pane.id()),
            focused: pane,
            others: BTreeMap::new(),
        }
    }

    pub(super) fn contains(&self, id: PaneId) -> bool {
        self.pane(id).is_some()
    }

    pub(super) fn panes_mut(&mut self) -> impl Iterator<Item = &mut Pane> {
        std::iter::once(&mut self.focused).chain(self.others.values_mut())
    }

    pub(super) fn focused_pane(&self) -> &Pane {
        &self.focused
    }

    pub(super) fn focused_pane_mut(&mut self) -> &mut Pane {
        &mut self.focused
    }

    pub(super) fn split(&mut self, direction: Direction, pane: Pane) -> Result<(), StateError> {
        self.layout.split(direction, pane.id())?;
        let old = std::mem::replace(&mut self.focused, pane);
        self.others.insert(old.id(), old);
        Ok(())
    }

    pub(super) fn close(&mut self, id: PaneId) -> Result<(), StateError> {
        let mut layout = self.layout.clone();
        layout.close(id)?;
        if id == self.focused.id() {
            let next = layout.focused();
            self.focused = self
                .others
                .remove(&next)
                .ok_or(StateError::UnknownPane(next))?;
        } else {
            self.others.remove(&id);
        }
        self.layout = layout;
        Ok(())
    }

    pub(super) fn focus_direction(&mut self, direction: Direction) -> Result<PaneId, StateError> {
        let mut layout = self.layout.clone();
        let next = layout.focus_direction(direction)?;
        let pane = self
            .others
            .remove(&next)
            .ok_or(StateError::UnknownPane(next))?;
        let old = std::mem::replace(&mut self.focused, pane);
        self.others.insert(old.id(), old);
        self.layout = layout;
        Ok(next)
    }
}
