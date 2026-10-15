mod convert;

use std::cell::Cell;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum LayoutState {
    Pane(u64),
    Split {
        side_by_side: bool,
        ratio: u16,
        first: Box<LayoutState>,
        second: Box<LayoutState>,
    },
}

pub const MAX_LAYOUT_DEPTH: usize = 64;

#[derive(Deserialize)]
#[serde(rename = "LayoutState")]
enum LayoutRepr {
    Pane(u64),
    Split {
        side_by_side: bool,
        ratio: u16,
        first: Box<LayoutState>,
        second: Box<LayoutState>,
    },
}

thread_local! {
    static LAYOUT_DEPTH: Cell<usize> = const { Cell::new(0) };
}

struct DepthGuard;

impl DepthGuard {
    fn enter() -> Option<Self> {
        LAYOUT_DEPTH.with(|depth| {
            let now = depth.get();
            (now < MAX_LAYOUT_DEPTH).then(|| {
                depth.set(now + 1);
                Self
            })
        })
    }
}

impl Drop for DepthGuard {
    fn drop(&mut self) {
        LAYOUT_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

impl<'de> Deserialize<'de> for LayoutState {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let _guard =
            DepthGuard::enter().ok_or_else(|| D::Error::custom("layout is nested too deeply"))?;
        Ok(match LayoutRepr::deserialize(deserializer)? {
            LayoutRepr::Pane(id) => Self::Pane(id),
            LayoutRepr::Split {
                side_by_side,
                ratio,
                first,
                second,
            } => Self::Split {
                side_by_side,
                ratio,
                first,
                second,
            },
        })
    }
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
