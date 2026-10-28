use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use rusty_core::{
    EntryState, LayoutState, PaneState, RowsState, SessionState, TabState, WindowState,
};
use tempfile::TempDir;

use super::*;
use crate::backup_path;

mod file;
mod keep;
mod load;
mod migration;
mod speed;

const HEADER_LEN: usize = 26;

fn tmp() -> TempDir {
    TempDir::new().unwrap()
}

fn session_file(dir: &TempDir) -> PathBuf {
    dir.path().join("session.bin")
}

fn tab(id: u64, location: &str) -> TabState {
    TabState {
        id,
        location: location.to_owned(),
        back: vec!["/".to_owned()],
        forward: Vec::new(),
        cursor: 3,
        scroll: 1,
        rows: None,
    }
}

fn pane(id: u64, tab: TabState) -> PaneState {
    PaneState {
        id,
        active: 0,
        page_rows: 20,
        tabs: vec![tab],
    }
}

fn one_pane(location: &str) -> SessionState {
    SessionState {
        windows: vec![WindowState {
            id: 1,
            placement: Vec::new(),
            workspaces: Vec::new(),
            active_workspace: 0,
            target: None,
            layout: LayoutState::Pane(2),
            focused_pane: 2,
            panes: vec![pane(2, tab(3, location))],
        }],
        focused_window: 1,
        next_id: 4,
    }
}

fn first() -> SessionState {
    one_pane("/home")
}

fn second() -> SessionState {
    one_pane("/work")
}

fn rejected() -> SessionState {
    SessionState {
        windows: Vec::new(),
        focused_window: 1,
        next_id: 4,
    }
}

fn split(first: LayoutState, second: LayoutState) -> LayoutState {
    LayoutState::Split {
        side_by_side: true,
        ratio: 500,
        first: Box::new(first),
        second: Box::new(second),
    }
}

fn rows(count: u64) -> RowsState {
    RowsState {
        first: 0,
        total: 5000,
        entries: (0..count)
            .map(|row| EntryState {
                name: format!("a-rather-long-file-name-{row:06}.txt"),
                is_folder: row % 4 == 0,
            })
            .collect(),
    }
}

fn three_panes_of_100_rows() -> SessionState {
    let panes = [2, 4, 6]
        .into_iter()
        .map(|id| {
            let mut tab = tab(id + 1, "/home/admin/projects");
            tab.back = (0..20).map(|i| format!("/home/admin/{i}")).collect();
            tab.rows = Some(rows(100));
            pane(id, tab)
        })
        .collect();
    SessionState {
        windows: vec![WindowState {
            id: 1,
            placement: Vec::new(),
            workspaces: Vec::new(),
            active_workspace: 0,
            target: None,
            layout: split(
                LayoutState::Pane(2),
                split(LayoutState::Pane(4), LayoutState::Pane(6)),
            ),
            focused_pane: 2,
            panes,
        }],
        focused_window: 1,
        next_id: 8,
    }
}

fn body(session: &SessionState) -> Vec<u8> {
    postcard::to_stdvec(session).unwrap()
}

fn framed(version: u16, body: &[u8]) -> Vec<u8> {
    let mut bytes = SESSION_MAGIC.to_vec();
    bytes.extend_from_slice(&version.to_le_bytes());
    bytes.extend_from_slice(&u32::try_from(body.len()).unwrap().to_le_bytes());
    bytes.extend_from_slice(&crc32fast::hash(body).to_le_bytes());
    bytes.extend_from_slice(body);
    bytes
}

fn put(path: &Path, session: &SessionState) {
    fs::write(path, framed(SESSION_VERSION, &body(session))).unwrap();
}

fn put_corrupt(path: &Path, session: &SessionState) {
    let mut bytes = framed(SESSION_VERSION, &body(session));
    bytes[HEADER_LEN] ^= 0x01;
    fs::write(path, bytes).unwrap();
}

fn not_found(error: &SessionFileError) -> bool {
    matches!(error, SessionFileError::Io(e) if e.kind() == io::ErrorKind::NotFound)
}
