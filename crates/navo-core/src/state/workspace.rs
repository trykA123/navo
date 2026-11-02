mod actions;
pub(crate) use actions::workspace_name;

use std::collections::BTreeMap;

use super::{Pane, Window};
use crate::{Layout, PaneId};

pub const SAVE_ON_SWITCH: bool = true;
pub const MAX_WORKSPACES: usize = 9;

#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub(super) layout: Layout,
    pub(super) focused: Pane,
    pub(super) others: BTreeMap<PaneId, Pane>,
    pub(super) target: Option<PaneId>,
    pub(super) last_marks: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Workspace {
    pub name: String,
    pub saved: Snapshot,
    pub live: Option<Snapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceAction {
    Switch { n: usize },
    New { name: Option<String> },
    Rename { n: usize, name: String },
    Move { n: usize, by: i8 },
    Delete { n: usize },
    Restore,
    Save,
    Discard { n: usize },
}

impl Snapshot {
    #[must_use]
    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    #[must_use]
    pub fn target(&self) -> Option<PaneId> {
        self.target
    }

    #[must_use]
    pub fn panes(&self) -> Vec<&Pane> {
        self.layout
            .panes()
            .into_iter()
            .filter_map(|id| {
                if id == self.focused.id() {
                    Some(&self.focused)
                } else {
                    self.others.get(&id)
                }
            })
            .collect()
    }

    #[must_use]
    pub fn last_marks(&self) -> Option<&[String]> {
        self.last_marks.as_deref()
    }

    pub(crate) fn from_parts(
        layout: Layout,
        panes: Vec<Pane>,
        target: Option<PaneId>,
        last_marks: Option<Vec<String>>,
    ) -> Result<Self, String> {
        let mut window = Window::restore(crate::WindowId::new(0), layout, panes, Vec::new())?;
        window.restore_target(target);
        let mut snapshot = window.snapshot();
        snapshot.last_marks = last_marks.map(|mut names| {
            names.truncate(super::window::MAX_LAST_MARKS);
            names
        });
        Ok(snapshot)
    }
}

impl Window {
    #[must_use]
    pub fn workspaces(&self) -> &[Workspace] {
        &self.workspaces
    }

    #[must_use]
    pub fn active_workspace(&self) -> usize {
        self.active
    }

    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            layout: self.layout.clone(),
            target: self.target,
            focused: self.focused.workspace_snapshot(),
            others: self
                .others
                .iter()
                .map(|(&id, pane)| (id, pane.workspace_snapshot()))
                .collect(),
            last_marks: self.last_marks.clone(),
        }
    }

    pub(crate) fn restore_workspaces(
        &mut self,
        workspaces: Vec<Workspace>,
        active: usize,
    ) -> Result<(), String> {
        if workspaces.is_empty() || workspaces.len() > MAX_WORKSPACES || active >= workspaces.len()
        {
            return Err("invalid workspace list or active index".into());
        }
        self.workspaces = workspaces;
        self.active = active;
        let snapshot = self.workspaces[active]
            .live
            .take()
            .unwrap_or_else(|| self.workspaces[active].saved.clone());
        self.load_snapshot(snapshot);
        Ok(())
    }

    pub(super) fn load_snapshot(&mut self, snapshot: Snapshot) {
        self.layout = snapshot.layout;
        self.focused = snapshot.focused;
        self.others = snapshot.others;
        self.target = snapshot.target;
        self.last_marks = snapshot.last_marks;
    }

    pub(super) fn keep_workspace(&mut self) {
        let snapshot = self.snapshot();
        let workspace = &mut self.workspaces[self.active];
        if SAVE_ON_SWITCH {
            workspace.saved = snapshot;
            workspace.live = None;
        } else {
            workspace.live = (snapshot != workspace.saved).then_some(snapshot);
        }
    }
}

#[must_use]
pub fn changes(saved: &Snapshot, live: &Snapshot) -> u16 {
    let left = saved.panes();
    let right = live.panes();
    let panes = (0..left.len().max(right.len()))
        .filter(|&index| match (left.get(index), right.get(index)) {
            (Some(a), Some(b)) => {
                a.active_index() != b.active_index()
                    || !a
                        .tabs()
                        .iter()
                        .map(|t| t.location())
                        .eq(b.tabs().iter().map(|t| t.location()))
            }
            _ => true,
        })
        .count();
    let layout = saved.layout.to_state() != live.layout.to_state();
    u16::try_from(panes)
        .unwrap_or(u16::MAX)
        .saturating_add(u16::from(layout))
}
