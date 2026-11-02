use super::tab_move::copy_of;
use super::{
    AppState, MAX_WORKSPACES, Pane, Snapshot, StateError, TabMove, Workspace, workspace_name,
};
use crate::{Direction, Layout, Location, MAX_PANES, PaneId, TabId};

const NAME_CHARS: usize = 40;

impl AppState {
    pub(crate) fn move_tab_to_workspace(&mut self, id: TabId, n: usize) -> Result<(), StateError> {
        let index = n
            .checked_sub(1)
            .filter(|&i| i < self.focused.workspaces.len())
            .ok_or(StateError::NoWorkspace(n))?;
        self.find_tab(id)?;
        if index == self.focused.active {
            return Ok(());
        }
        let mut tab = copy_of(&self.detach_tab(id)?);
        tab.id = TabId::new(self.take_ids(1));
        let workspace = &mut self.focused.workspaces[index];
        let snapshot = match &mut workspace.live {
            Some(live) => live,
            None => &mut workspace.saved,
        };
        snapshot.focused.append_tabs(vec![tab]);
        Ok(())
    }

    pub(crate) fn move_tab_to_new_workspace(&mut self, id: TabId) -> Result<(), StateError> {
        self.find_tab(id)?;
        let count = self.focused.workspaces.len();
        if count == MAX_WORKSPACES {
            return Err(StateError::WorkspaceLimit);
        }
        let mut tab = copy_of(&self.detach_tab(id)?);
        let name = workspace_name_for(tab.base_or_location())
            .unwrap_or_else(|| format!("Workspace {}", count + 1));
        tab.id = TabId::new(self.take_ids(1));
        let pane = PaneId::new(self.take_ids(1));
        let saved = Snapshot::from_parts(Layout::new(pane), vec![Pane::new(pane, tab)], None, None)
            .map_err(StateError::InvalidWorkspace)?;
        self.focused.workspaces.push(Workspace {
            name,
            saved,
            live: None,
        });
        Ok(())
    }

    pub(crate) fn move_tab_to_pane(&mut self, id: TabId, n: usize) -> Result<(), StateError> {
        let (from, index) = self.find_tab(id)?;
        let to = n
            .checked_sub(1)
            .and_then(|i| self.focused.layout().numbered().get(i).copied())
            .ok_or(StateError::NoSuchPane(n))?;
        if from == to {
            return self.focus_pane_id(to);
        }
        let at = self
            .pane(to)
            .ok_or(StateError::UnknownPane(to))?
            .tabs()
            .len();
        let change = TabMove {
            from,
            index,
            to,
            at,
            copy: false,
        };
        self.move_tab(change).map(drop)
    }

    pub(crate) fn move_tab_to_new_pane(&mut self, id: TabId) -> Result<(), StateError> {
        let (from, _) = self.find_tab(id)?;
        if self.focused.layout().panes().len() >= MAX_PANES {
            return Err(StateError::TooManyPanes);
        }
        self.focus_pane_id(from)?;
        let to = self.split(Direction::Right)?;
        let placeholder = self
            .pane(to)
            .ok_or(StateError::UnknownPane(to))?
            .active()
            .id();
        let (_, index) = self.find_tab(id)?;
        let change = TabMove {
            from,
            index,
            to,
            at: 0,
            copy: false,
        };
        self.move_tab(change)?;
        let pane = self.pane_mut(to).ok_or(StateError::UnknownPane(to))?;
        let at = pane
            .position(placeholder)
            .ok_or(StateError::UnknownTab(placeholder))?;
        pane.take_tab(at)?;
        let shown = pane.position(id).ok_or(StateError::UnknownTab(id))?;
        pane.activate(shown)
    }
}

fn workspace_name_for(folder: &Location) -> Option<String> {
    let text = folder.name().map_or_else(
        || folder.as_str().to_owned(),
        |name| name.display().into_owned(),
    );
    let short: String = text.chars().take(NAME_CHARS).collect();
    workspace_name(&short).ok()
}
