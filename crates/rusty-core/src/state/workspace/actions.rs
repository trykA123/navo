use super::{MAX_WORKSPACES, Snapshot, Workspace, WorkspaceAction};
use crate::{AppState, Layout, LayoutState, PaneId, StateError, TabId};

impl AppState {
    pub(crate) fn change_workspace(
        &mut self,
        action: &WorkspaceAction,
    ) -> Result<bool, StateError> {
        match action {
            WorkspaceAction::Switch { n } => self.switch_workspace(*n),
            WorkspaceAction::New { name } => self.new_workspace(name.as_deref()),
            WorkspaceAction::Rename { n, name } => {
                let index = self.workspace_index(*n)?;
                self.focused.workspaces[index].name = workspace_name(name)?;
                Ok(false)
            }
            WorkspaceAction::Move { n, by } => self.move_workspace(*n, *by),
            WorkspaceAction::Delete { n } => self.delete_workspace(*n),
            WorkspaceAction::Restore => self.restore_workspace(),
            WorkspaceAction::Save => {
                let snapshot = self.focused.snapshot();
                let workspace = &mut self.focused.workspaces[self.focused.active];
                workspace.saved = snapshot;
                workspace.live = None;
                Ok(false)
            }
            WorkspaceAction::Discard { n } => self.discard_workspace(*n),
        }
    }

    fn workspace_index(&self, n: usize) -> Result<usize, StateError> {
        n.checked_sub(1)
            .filter(|&i| i < self.focused.workspaces.len())
            .ok_or(StateError::NoWorkspace(n))
    }

    fn switch_workspace(&mut self, n: usize) -> Result<bool, StateError> {
        let index = self.workspace_index(n)?;
        if index == self.focused.active {
            return Ok(false);
        }
        self.focused.keep_workspace();
        self.enter_workspace(index);
        Ok(true)
    }

    fn enter_workspace(&mut self, index: usize) {
        self.focused.active = index;
        let workspace = &mut self.focused.workspaces[index];
        let snapshot = workspace
            .live
            .take()
            .unwrap_or_else(|| workspace.saved.clone());
        self.focused.load_snapshot(snapshot);
    }

    fn new_workspace(&mut self, name: Option<&str>) -> Result<bool, StateError> {
        if self.focused.workspaces.len() == MAX_WORKSPACES {
            return Err(StateError::WorkspaceLimit);
        }
        let number = self.focused.workspaces.len() + 1;
        let name = name.map_or_else(|| Ok(format!("Workspace {number}")), workspace_name)?;
        let saved = self.copy_workspace()?;
        self.focused.keep_workspace();
        self.focused.workspaces.push(Workspace {
            name,
            saved,
            live: None,
        });
        self.enter_workspace(number - 1);
        Ok(true)
    }

    fn copy_workspace(&mut self) -> Result<Snapshot, StateError> {
        let mut snapshot = self.focused.snapshot();
        let mut mapping = std::collections::BTreeMap::new();
        let mut panes = Vec::new();
        for mut pane in snapshot.panes().into_iter().cloned() {
            let id = PaneId::new(self.take_ids(1));
            mapping.insert(pane.id(), id);
            pane.id = id;
            for tab in &mut pane.tabs {
                tab.id = TabId::new(self.take_ids(1));
            }
            panes.push(pane);
        }
        let focused = mapping[&snapshot.layout.focused()];
        let mut layout = snapshot.layout.to_state();
        remap_layout(&mut layout, &mapping);
        snapshot.layout =
            Layout::from_state(&layout, focused).map_err(StateError::InvalidWorkspace)?;
        let mut panes: std::collections::BTreeMap<_, _> =
            panes.into_iter().map(|p| (p.id(), p)).collect();
        snapshot.focused = panes
            .remove(&focused)
            .ok_or(StateError::UnknownPane(focused))?;
        snapshot.others = panes;
        Ok(snapshot)
    }

    fn move_workspace(&mut self, n: usize, by: i8) -> Result<bool, StateError> {
        let index = self.workspace_index(n)?;
        if !matches!(by, -1 | 1) {
            return Err(StateError::InvalidWorkspaceMove);
        }
        let Some(next) = index
            .checked_add_signed(isize::from(by))
            .filter(|&i| i < self.focused.workspaces.len())
        else {
            return Ok(false);
        };
        self.focused.workspaces.swap(index, next);
        if self.focused.active == index {
            self.focused.active = next;
        } else if self.focused.active == next {
            self.focused.active = index;
        }
        Ok(false)
    }

    fn delete_workspace(&mut self, n: usize) -> Result<bool, StateError> {
        let index = self.workspace_index(n)?;
        if self.focused.workspaces.len() == 1 {
            return Err(StateError::LastWorkspace);
        }
        let active = self.focused.active;
        if active == index {
            self.focused.keep_workspace();
        }
        let workspace = self.focused.workspaces.remove(index);
        self.focused.deleted_workspace = Some((index, workspace));
        if active == index {
            self.enter_workspace(index.min(self.focused.workspaces.len() - 1));
        } else if active > index {
            self.focused.active -= 1;
        }
        Ok(active == index)
    }

    fn restore_workspace(&mut self) -> Result<bool, StateError> {
        if self.focused.deleted_workspace.is_none() {
            return Err(StateError::NothingToRestore);
        }
        if self.focused.workspaces.len() == MAX_WORKSPACES {
            return Err(StateError::WorkspaceLimit);
        }
        if let Some((index, workspace)) = self.focused.deleted_workspace.take() {
            let index = index.min(self.focused.workspaces.len());
            self.focused.workspaces.insert(index, workspace);
            if self.focused.active >= index {
                self.focused.active += 1;
            }
        }
        Ok(false)
    }

    fn discard_workspace(&mut self, n: usize) -> Result<bool, StateError> {
        let index = self.workspace_index(n)?;
        let workspace = &mut self.focused.workspaces[index];
        workspace.live = None;
        if self.focused.active != index {
            return Ok(false);
        }
        let saved = workspace.saved.clone();
        self.focused.load_snapshot(saved);
        Ok(true)
    }
}

pub(crate) fn workspace_name(name: &str) -> Result<String, StateError> {
    let name = name.trim();
    if !(1..=40).contains(&name.chars().count()) {
        return Err(StateError::InvalidWorkspaceName);
    }
    Ok(name.to_owned())
}

fn remap_layout(layout: &mut LayoutState, mapping: &std::collections::BTreeMap<PaneId, PaneId>) {
    match layout {
        LayoutState::Pane(id) => *id = mapping[&PaneId::new(*id)].get(),
        LayoutState::Split { first, second, .. } => {
            remap_layout(first, mapping);
            remap_layout(second, mapping);
        }
    }
}
