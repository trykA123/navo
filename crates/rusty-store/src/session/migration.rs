use rusty_core::{LayoutState, PaneState, SessionState, WindowState};
use serde::Deserialize;

use super::SessionFileError;

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
    panes: Vec<PaneState>,
}

pub(super) fn decode(version: u16, body: &[u8]) -> Result<SessionState, SessionFileError> {
    if version >= 2 {
        return postcard::from_bytes(body).map_err(|e| SessionFileError::Decode(e.to_string()));
    }
    let old: SessionV1 =
        postcard::from_bytes(body).map_err(|e| SessionFileError::Decode(e.to_string()))?;
    Ok(SessionState {
        windows: old
            .windows
            .into_iter()
            .map(|window| WindowState {
                id: window.id,
                layout: window.layout,
                focused_pane: window.focused_pane,
                panes: window.panes,
                placement: Vec::new(),
            })
            .collect(),
        focused_window: old.focused_window,
        next_id: old.next_id,
    })
}
