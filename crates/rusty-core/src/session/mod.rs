mod convert;

use serde::{Deserialize, Serialize};

use crate::AppState;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionState {
    pub windows: Vec<WindowState>,
    pub focused_window: u64,
    pub next_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowState {
    pub id: u64,
    pub layout: LayoutState,
    pub focused_pane: u64,
    pub panes: Vec<PaneState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayoutState {
    Pane(u64),
    Split {
        side_by_side: bool,
        ratio: u16,
        first: Box<LayoutState>,
        second: Box<LayoutState>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneState {
    pub id: u64,
    pub active: usize,
    pub page_rows: u64,
    pub tabs: Vec<TabState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabState {
    pub id: u64,
    pub location: String,
    pub back: Vec<String>,
    pub forward: Vec<String>,
    pub cursor: u64,
    pub scroll: u64,
    pub rows: Option<RowsState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowsState {
    pub first: u64,
    pub total: u64,
    pub entries: Vec<EntryState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryState {
    pub name: String,
    pub is_folder: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SessionError {
    #[error("invalid session: {0}")]
    Invalid(String),
}

impl AppState {
    #[must_use]
    pub fn to_session(&self, rows_per_pane: usize) -> SessionState {
        convert::save(self, rows_per_pane)
    }

    pub fn from_session(session: SessionState) -> Result<AppState, SessionError> {
        convert::restore(&session).map_err(SessionError::Invalid)
    }
}

#[cfg(test)]
mod tests;
