use rusty_core::placement::Placement;
use rusty_core::{
    LayoutState, PaneState, RowsState, SessionState, SnapshotState, TabState, WindowState,
    WorkspaceState,
};
use serde::Deserialize;

use super::migration::deserialize_placements;

#[derive(Deserialize)]
#[cfg_attr(test, derive(serde::Serialize))]
pub(super) struct SessionV4 {
    pub(super) windows: Vec<WindowV4>,
    pub(super) focused_window: u64,
    pub(super) next_id: u64,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(serde::Serialize))]
pub(super) struct WindowV4 {
    pub(super) id: u64,
    pub(super) layout: LayoutState,
    pub(super) focused_pane: u64,
    pub(super) panes: Vec<PaneV4>,
    pub(super) workspaces: Vec<WorkspaceV4>,
    pub(super) active_workspace: u8,
    #[serde(deserialize_with = "deserialize_placements")]
    pub(super) placement: Vec<Placement>,
    pub(super) target: Option<u64>,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(serde::Serialize))]
pub(super) struct WorkspaceV4 {
    pub(super) name: String,
    pub(super) saved: SnapshotV4,
    pub(super) live: Option<SnapshotV4>,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(serde::Serialize))]
pub(super) struct SnapshotV4 {
    pub(super) layout: LayoutState,
    pub(super) focused_pane: u64,
    pub(super) panes: Vec<PaneV4>,
    pub(super) last_marks: Option<Vec<String>>,
    pub(super) target: Option<u64>,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(serde::Serialize))]
pub(super) struct PaneV4 {
    pub(super) id: u64,
    pub(super) active: usize,
    pub(super) page_rows: u64,
    pub(super) tabs: Vec<TabV4>,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(serde::Serialize))]
pub(super) struct TabV4 {
    pub(super) id: u64,
    pub(super) location: String,
    pub(super) back: Vec<String>,
    pub(super) forward: Vec<String>,
    pub(super) cursor: u64,
    pub(super) scroll: u64,
    pub(super) rows: Option<RowsState>,
}

pub(super) fn panes(panes: Vec<PaneV4>) -> Vec<PaneState> {
    panes.into_iter().map(PaneState::from).collect()
}

impl From<PaneV4> for PaneState {
    fn from(pane: PaneV4) -> Self {
        Self {
            id: pane.id,
            active: pane.active,
            page_rows: pane.page_rows,
            tabs: pane.tabs.into_iter().map(TabState::from).collect(),
        }
    }
}

impl From<TabV4> for TabState {
    fn from(tab: TabV4) -> Self {
        Self {
            id: tab.id,
            location: tab.location,
            back: tab.back,
            forward: tab.forward,
            cursor: tab.cursor,
            scroll: tab.scroll,
            rows: tab.rows,
            pinned: false,
            label: None,
        }
    }
}

impl From<SnapshotV4> for SnapshotState {
    fn from(snapshot: SnapshotV4) -> Self {
        Self {
            layout: snapshot.layout,
            focused_pane: snapshot.focused_pane,
            panes: panes(snapshot.panes),
            last_marks: snapshot.last_marks,
            target: snapshot.target,
        }
    }
}

impl From<SessionV4> for SessionState {
    fn from(old: SessionV4) -> Self {
        Self {
            windows: old.windows.into_iter().map(WindowState::from).collect(),
            focused_window: old.focused_window,
            next_id: old.next_id,
        }
    }
}

impl From<WindowV4> for WindowState {
    fn from(window: WindowV4) -> Self {
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
            target: window.target,
        }
    }
}
