use std::collections::{HashMap, HashSet};

use super::{PaneState, SessionState, SnapshotState, WindowState, WorkspaceState};
use crate::{Layout, PaneId, Snapshot, Window, Workspace};

pub(super) fn save(window: &Window) -> Vec<WorkspaceState> {
    let current = window.snapshot();
    window
        .workspaces()
        .iter()
        .enumerate()
        .map(|(index, workspace)| {
            let live = if index == window.active_workspace() {
                (current != workspace.saved).then_some(&current)
            } else {
                workspace.live.as_ref()
            };
            let saved = save_snapshot(&workspace.saved);
            let live = live
                .map(save_snapshot)
                .filter(|snapshot| snapshot != &saved);
            WorkspaceState {
                name: workspace.name.clone(),
                saved,
                live,
            }
        })
        .collect()
}

fn save_snapshot(snapshot: &Snapshot) -> SnapshotState {
    SnapshotState {
        layout: snapshot.layout().to_state(),
        focused_pane: snapshot.layout().focused().get(),
        panes: snapshot
            .panes()
            .into_iter()
            .map(|pane| super::convert::save_pane(pane, 0))
            .collect(),
        last_marks: snapshot.last_marks().map(<[String]>::to_vec),
    }
}

pub(super) fn restore(window: &mut Window, state: &WindowState) -> Result<(), String> {
    if state.workspaces.is_empty() {
        return Ok(());
    }
    let workspaces = state
        .workspaces
        .iter()
        .map(|workspace| {
            let name = crate::state::workspace_name(&workspace.name).map_err(|e| e.to_string())?;
            Ok(Workspace {
                name,
                saved: restore_snapshot(&workspace.saved)?,
                live: workspace.live.as_ref().map(restore_snapshot).transpose()?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    window.restore_workspaces(workspaces, usize::from(state.active_workspace))
}

fn restore_snapshot(snapshot: &SnapshotState) -> Result<Snapshot, String> {
    let layout = Layout::from_state(&snapshot.layout, PaneId::new(snapshot.focused_pane))?;
    let panes = snapshot
        .panes
        .iter()
        .map(super::convert::restore_pane)
        .collect::<Result<Vec<_>, _>>()?;
    Snapshot::from_parts(layout, panes, snapshot.last_marks.clone())
}

type Owner = (usize, usize, bool);

pub(super) fn check_ids(session: &SessionState) -> Result<(), String> {
    if session.next_id > 1 << 62 {
        return Err(format!("next_id {} is implausibly large", session.next_id));
    }
    let mut owners = HashMap::new();
    for (index, window) in session.windows.iter().enumerate() {
        check_id(
            window.id,
            (index, usize::MAX, false),
            session.next_id,
            &mut owners,
        )?;
        let active = usize::from(window.active_workspace);
        check_panes(&window.panes, (index, active), session.next_id, &mut owners)?;
        for (workspace_index, workspace) in window.workspaces.iter().enumerate() {
            for snapshot in std::iter::once(&workspace.saved).chain(workspace.live.as_ref()) {
                check_panes(
                    &snapshot.panes,
                    (index, workspace_index),
                    session.next_id,
                    &mut owners,
                )?;
            }
        }
    }
    Ok(())
}

fn check_panes(
    panes: &[PaneState],
    owner: (usize, usize),
    next_id: u64,
    owners: &mut HashMap<u64, Owner>,
) -> Result<(), String> {
    let mut seen = HashSet::new();
    for pane in panes {
        for (id, is_tab) in
            std::iter::once((pane.id, false)).chain(pane.tabs.iter().map(|t| (t.id, true)))
        {
            if !seen.insert(id) {
                return Err(format!("id {id} is used twice"));
            }
            check_id(id, (owner.0, owner.1, is_tab), next_id, owners)?;
        }
    }
    Ok(())
}

fn check_id(
    id: u64,
    owner: Owner,
    next_id: u64,
    owners: &mut HashMap<u64, Owner>,
) -> Result<(), String> {
    if id >= next_id {
        return Err(format!("id {id} is not below next_id {next_id}"));
    }
    if let Some(previous) = owners.insert(id, owner)
        && previous != owner
    {
        return Err(format!("id {id} is used twice"));
    }
    Ok(())
}
