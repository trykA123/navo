use rusty_core::placement::Placement;
use rusty_core::{LayoutState, SessionState, SnapshotState, WindowState, WorkspaceState};
use serde::Deserialize;

use super::SessionFileError;
use super::legacy::{PaneV4, SessionV4, panes};

#[derive(Deserialize)]
struct SessionV1 {
    windows: Vec<WindowV1>,
    focused_window: u64,
    next_id: u64,
}

#[derive(Deserialize)]
struct WindowV1 {
    id: u64,
    layout: LayoutState,
    focused_pane: u64,
    panes: Vec<PaneV4>,
}

#[derive(Deserialize)]
struct SessionV2 {
    windows: Vec<WindowV2>,
    focused_window: u64,
    next_id: u64,
}

#[derive(Deserialize)]
struct WindowV2 {
    id: u64,
    layout: LayoutState,
    focused_pane: u64,
    panes: Vec<PaneV4>,
    #[serde(default, deserialize_with = "deserialize_placements")]
    placement: Vec<Placement>,
}

#[derive(Deserialize)]
struct SessionV3 {
    windows: Vec<WindowV3>,
    focused_window: u64,
    next_id: u64,
}

#[derive(Deserialize)]
struct WindowV3 {
    id: u64,
    layout: LayoutState,
    focused_pane: u64,
    panes: Vec<PaneV4>,
    workspaces: Vec<WorkspaceV3>,
    active_workspace: u8,
    #[serde(deserialize_with = "deserialize_placements")]
    placement: Vec<Placement>,
}

#[derive(Deserialize)]
struct WorkspaceV3 {
    name: String,
    saved: SnapshotV3,
    live: Option<SnapshotV3>,
}

#[derive(Deserialize)]
struct SnapshotV3 {
    layout: LayoutState,
    focused_pane: u64,
    panes: Vec<PaneV4>,
    last_marks: Option<Vec<String>>,
}

impl From<SnapshotV3> for SnapshotState {
    fn from(snapshot: SnapshotV3) -> Self {
        Self {
            layout: snapshot.layout,
            focused_pane: snapshot.focused_pane,
            panes: panes(snapshot.panes),
            last_marks: snapshot.last_marks,
            target: None,
        }
    }
}

impl From<SessionV3> for SessionState {
    fn from(old: SessionV3) -> Self {
        Self {
            windows: old.windows.into_iter().map(WindowState::from).collect(),
            focused_window: old.focused_window,
            next_id: old.next_id,
        }
    }
}

impl From<WindowV3> for WindowState {
    fn from(window: WindowV3) -> Self {
        Self {
            id: window.id,
            layout: window.layout,
            focused_pane: window.focused_pane,
            panes: panes(window.panes),
            workspaces: window
                .workspaces
                .into_iter()
                .map(|workspace| WorkspaceState {
                    name: workspace.name,
                    saved: workspace.saved.into(),
                    live: workspace.live.map(Into::into),
                })
                .collect(),
            active_workspace: window.active_workspace,
            placement: window.placement,
            target: None,
        }
    }
}

pub(super) fn deserialize_placements<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<Placement>, D::Error> {
    let mut placements = Vec::<Placement>::deserialize(deserializer)?;
    placements.truncate(8);
    Ok(placements)
}

impl From<WindowV2> for WindowState {
    fn from(window: WindowV2) -> Self {
        Self {
            id: window.id,
            layout: window.layout,
            focused_pane: window.focused_pane,
            panes: panes(window.panes),
            workspaces: Vec::new(),
            active_workspace: 0,
            target: None,
            placement: window.placement,
        }
    }
}

pub(super) fn decode(version: u16, body: &[u8]) -> Result<SessionState, SessionFileError> {
    let decode_error = |error: postcard::Error| SessionFileError::Decode(error.to_string());
    if version >= 3 {
        let mut session = decode_with_workspaces(version, body).map_err(decode_error)?;
        sanitize_workspaces(&mut session);
        return Ok(session);
    }
    let old = if version == 2 {
        postcard::from_bytes::<SessionV2>(body).map_err(decode_error)?
    } else {
        let old = postcard::from_bytes::<SessionV1>(body).map_err(decode_error)?;
        SessionV2 {
            windows: old
                .windows
                .into_iter()
                .map(|window| WindowV2 {
                    id: window.id,
                    layout: window.layout,
                    focused_pane: window.focused_pane,
                    panes: window.panes,
                    placement: Vec::new(),
                })
                .collect(),
            focused_window: old.focused_window,
            next_id: old.next_id,
        }
    };
    let mut session = SessionState {
        windows: old.windows.into_iter().map(WindowState::from).collect(),
        focused_window: old.focused_window,
        next_id: old.next_id,
    };
    let converted = rusty_core::AppState::from_session(&session)
        .map_err(|error| SessionFileError::Decode(error.to_string()))?
        .to_session(0);
    for converted in converted.windows {
        if let Some(window) = session
            .windows
            .iter_mut()
            .find(|window| window.id == converted.id)
        {
            window.workspaces = converted.workspaces;
        }
    }
    Ok(session)
}

fn decode_with_workspaces(version: u16, body: &[u8]) -> Result<SessionState, postcard::Error> {
    Ok(match version {
        3 => postcard::from_bytes::<SessionV3>(body)?.into(),
        4 => postcard::from_bytes::<SessionV4>(body)?.into(),
        _ => postcard::from_bytes(body)?,
    })
}

fn sanitize_workspaces(session: &mut SessionState) {
    let mut checked = session.clone();
    for window in &mut checked.windows {
        window.workspaces.clear();
        window.active_workspace = 0;
    }
    for index in 0..session.windows.len() {
        let window = &mut session.windows[index];
        let original_active = usize::from(window.active_workspace);
        let mut active = None;
        for (position, workspace) in std::mem::take(&mut window.workspaces)
            .into_iter()
            .take(9)
            .enumerate()
        {
            let mut trial = checked.clone();
            let candidate = &mut trial.windows[index];
            candidate.workspaces.push(workspace);
            candidate.active_workspace = if position == original_active {
                (candidate.workspaces.len() - 1) as u8
            } else {
                active.unwrap_or(0)
            };
            load_active_snapshot(candidate);
            if let Err(error) = rusty_core::AppState::from_session(&trial) {
                tracing::warn!(
                    window = window.id,
                    workspace = position + 1,
                    %error,
                    "session workspace dropped"
                );
                continue;
            }
            if position == original_active {
                active = Some(trial.windows[index].active_workspace);
            }
            checked = trial;
        }
        window
            .workspaces
            .clone_from(&checked.windows[index].workspaces);
        window.active_workspace = active.unwrap_or(0);
        if active.is_none() && !window.workspaces.is_empty() {
            load_active_snapshot(window);
        }
    }
}

fn load_active_snapshot(window: &mut WindowState) {
    let selected = &window.workspaces[usize::from(window.active_workspace)];
    let snapshot = selected.live.as_ref().unwrap_or(&selected.saved);
    window.layout = snapshot.layout.clone();
    window.focused_pane = snapshot.focused_pane;
    window.panes = snapshot.panes.clone();
    window.target = snapshot.target;
}
