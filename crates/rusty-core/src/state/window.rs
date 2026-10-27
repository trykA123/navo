use std::collections::BTreeMap;

use super::{Pane, StateError, Workspace};
use crate::placement::{MonitorSetup, Placement};
use crate::{Applied, Direction, Layout, MAX_PANES, PaneId, SplitPath, Tiles, WindowId};

pub(super) const MAX_LAST_MARKS: usize = 100_000;

#[derive(Debug, Clone, PartialEq)]
pub struct Window {
    id: WindowId,
    pub(super) layout: Layout,
    pub(super) focused: Pane,
    pub(super) others: BTreeMap<PaneId, Pane>,
    pub(super) target: Option<PaneId>,
    placement: Vec<Placement>,
    pub(super) last_marks: Option<Vec<String>>,
    pub(super) workspaces: Vec<Workspace>,
    pub(super) active: usize,
    pub(super) deleted_workspace: Option<(usize, Workspace)>,
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
    pub fn target(&self) -> Option<PaneId> {
        self.target
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

    #[must_use]
    pub fn placement(&self) -> &[Placement] {
        &self.placement
    }

    pub fn remember(&mut self, setup: &MonitorSetup, placement: Placement) {
        crate::placement::remember(&mut self.placement, setup, placement);
    }

    pub(crate) fn restore(
        id: WindowId,
        layout: Layout,
        panes: Vec<Pane>,
        mut placement: Vec<Placement>,
    ) -> Result<Self, String> {
        placement.truncate(crate::placement::MAX_PLACEMENTS);
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
        let mut window = Self {
            id,
            layout,
            focused,
            others,
            target: None,
            placement,
            last_marks: None,
            workspaces: Vec::new(),
            active: 0,
            deleted_workspace: None,
        };
        window.initialize_workspace();
        Ok(window)
    }

    pub(super) fn new(id: WindowId, pane: Pane) -> Self {
        let mut window = Self {
            id,
            layout: Layout::new(pane.id()),
            focused: pane,
            others: BTreeMap::new(),
            target: None,
            placement: Vec::new(),
            last_marks: None,
            workspaces: Vec::new(),
            active: 0,
            deleted_workspace: None,
        };
        window.initialize_workspace();
        window
    }

    fn initialize_workspace(&mut self) {
        self.repair_target();
        self.workspaces.push(Workspace {
            name: "Workspace 1".into(),
            saved: self.snapshot(),
            live: None,
        });
    }

    pub(super) fn last_marks(&self) -> Option<&[String]> {
        self.last_marks.as_deref()
    }

    pub(super) fn remember_marks(&mut self, mut names: Vec<String>) {
        if !names.is_empty() {
            names.truncate(MAX_LAST_MARKS);
            self.last_marks = Some(names);
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
        if self.others.len() + 1 >= MAX_PANES {
            return Err(StateError::TooManyPanes);
        }
        self.layout.split(direction, pane.id())?;
        let old = std::mem::replace(&mut self.focused, pane);
        self.target = Some(old.id());
        self.others.insert(old.id(), old);
        Ok(())
    }

    pub(super) fn close(&mut self, id: PaneId) -> Result<(), StateError> {
        let mut layout = self.layout.clone();
        layout.close(id)?;
        self.layout = layout;
        self.sync_focus();
        self.merge_tabs(std::iter::once(id));
        self.repair_target();
        Ok(())
    }

    pub(super) fn focus_direction(&mut self, direction: Direction) -> Result<PaneId, StateError> {
        let next = self.layout.focus_direction(direction)?;
        self.sync_focus();
        Ok(next)
    }

    pub fn apply_tiles(&mut self, tiles: &Tiles, make_pane: &mut dyn FnMut() -> Pane) -> Applied {
        let applied = self.layout.apply(tiles, &mut || {
            let pane = make_pane();
            let id = pane.id();
            self.others.insert(id, pane);
            id
        });
        self.sync_focus();
        self.merge_tabs(applied.removed.iter().rev().copied());
        self.repair_target();
        applied
    }

    pub fn swap(&mut self, a: PaneId, b: PaneId) -> Result<(), StateError> {
        self.layout.swap_panes(a, b)?;
        Ok(())
    }

    pub fn set_ratio(&mut self, split: &SplitPath, ratio: u16) -> Result<(), StateError> {
        self.layout.set_ratio(split, ratio)?;
        Ok(())
    }

    pub fn even(&mut self) {
        self.layout.even();
    }

    pub fn focus_number(&mut self, n: usize) -> Result<(), StateError> {
        let id = n
            .checked_sub(1)
            .and_then(|index| self.layout.numbered().get(index).copied())
            .ok_or(StateError::NoSuchPane(n))?;
        self.layout.focus(id)?;
        self.sync_focus();
        Ok(())
    }

    #[expect(clippy::expect_used, reason = "layout panes are owned by the window")]
    fn sync_focus(&mut self) {
        let next = self.layout.focused();
        if next == self.focused.id() {
            return;
        }
        let pane = self
            .others
            .remove(&next)
            .expect("the layout's focused pane exists in the window");
        let old = std::mem::replace(&mut self.focused, pane);
        self.target = Some(old.id());
        self.others.insert(old.id(), old);
    }

    #[expect(clippy::expect_used, reason = "layout panes are owned by the window")]
    fn merge_tabs(&mut self, removed: impl Iterator<Item = PaneId>) {
        let tabs: Vec<_> = removed
            .flat_map(|id| {
                self.others
                    .remove(&id)
                    .expect("a removed layout pane exists outside the focused pane")
                    .take_tabs()
            })
            .collect();
        if tabs.is_empty() {
            return;
        }
        let last = *self.layout.numbered().last().expect("a layout has a pane");
        let pane = if last == self.focused.id() {
            &mut self.focused
        } else {
            self.others
                .get_mut(&last)
                .expect("the last layout pane exists in the window")
        };
        pane.append_tabs(tabs);
    }

    fn repair_target(&mut self) {
        if self
            .target
            .is_some_and(|id| id != self.focused.id() && self.layout.contains(id))
        {
            return;
        }
        self.target = self
            .layout
            .numbered()
            .into_iter()
            .find(|&id| id != self.focused.id());
    }
}
