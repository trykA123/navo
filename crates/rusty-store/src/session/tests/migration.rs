use super::*;
use rusty_core::placement::{
    Monitor, MonitorLocalLogicalRect, MonitorSetup, PhysicalRect, Placement,
};

const V1_SESSION: &[u8] = include_bytes!("../../../tests/fixtures/session/v1.session");

fn old_session() -> SessionState {
    let mut session = first();
    let mut other = second().windows.remove(0);
    other.id = 4;
    other.layout = LayoutState::Pane(5);
    other.focused_pane = 5;
    other.panes[0].id = 5;
    other.panes[0].tabs[0].id = 6;
    session.windows.push(other);
    session.focused_window = 4;
    session.next_id = 7;
    AppState::from_session(&session).unwrap().to_session(0)
}

#[test]
fn frozen_v1_file_loads_with_no_placements_and_remains_unchanged() {
    let dir = tmp();
    let path = session_file(&dir);
    fs::write(&path, V1_SESSION).unwrap();
    let (session, source, errors) = load_session(&path);
    assert_eq!(
        (session, source),
        (Some(old_session()), SessionSource::Main)
    );
    assert!(errors.is_empty());
    assert_eq!(fs::read(&path).unwrap(), V1_SESSION);
}

#[test]
fn version_zero_loads_with_no_placements_and_does_not_modify_the_file() {
    let dir = tmp();
    let path = session_file(&dir);
    let mut bytes = V1_SESSION.to_vec();
    bytes[16..18].copy_from_slice(&0_u16.to_le_bytes());
    fs::write(&path, &bytes).unwrap();
    let (session, source, errors) = load_session(&path);
    assert_eq!(
        (session, source),
        (Some(old_session()), SessionSource::Main)
    );
    assert!(errors.is_empty());
    assert_eq!(fs::read(&path).unwrap(), bytes);
}

#[test]
fn corrupt_main_loads_a_v1_backup_then_preserves_it_when_upgrading() {
    let dir = tmp();
    let path = session_file(&dir);
    let old = V1_SESSION;
    fs::write(backup_path(&path), old).unwrap();
    fs::write(&path, b"corrupt").unwrap();
    let (loaded, source, _) = load_session(&path);
    assert_eq!(source, SessionSource::Backup);
    write_session(&path, &loaded.unwrap()).unwrap();
    assert_eq!(fs::read(backup_path(&path)).unwrap(), old);
    assert_eq!(read_session_file(&path).unwrap(), old_session());
}

#[test]
fn upgrading_a_v1_main_keeps_the_original_bytes_as_backup() {
    let dir = tmp();
    let path = session_file(&dir);
    let old = V1_SESSION;
    fs::write(&path, old).unwrap();
    write_session(&path, &old_session()).unwrap();
    assert_eq!(fs::read(backup_path(&path)).unwrap(), old);
    assert_eq!(read_session_file(&path).unwrap(), old_session());
    assert_eq!(
        u16::from_le_bytes(fs::read(&path).unwrap()[16..18].try_into().unwrap()),
        SESSION_VERSION
    );
}

#[test]
fn placements_round_trip_through_the_current_session_file() {
    let dir = tmp();
    let path = session_file(&dir);
    let bounds = PhysicalRect {
        x: -1920,
        y: 0,
        width: 1920,
        height: 1080,
    };
    let setup = MonitorSetup::new(vec![Monitor {
        bounds,
        work_area: bounds,
        scale: 1.25,
        primary: true,
    }])
    .unwrap();
    let mut session = first();
    session.windows[0].remember(
        &setup,
        Placement {
            setup: setup.fingerprint(),
            bounds: MonitorLocalLogicalRect {
                x: 10.0,
                y: 20.0,
                width: 800.0,
                height: 600.0,
            },
            monitor: bounds,
            maximized: true,
        },
    );
    write_session(&path, &session).unwrap();
    assert_eq!(load_session(&path).0, Some(session));
}

#[derive(serde::Serialize)]
struct LegacySession {
    windows: Vec<LegacyWindow>,
    focused_window: u64,
    next_id: u64,
}

#[derive(serde::Serialize)]
struct LegacyWindow {
    id: u64,
    layout: LayoutState,
    focused_pane: u64,
    panes: Vec<crate::session::legacy::PaneV4>,
    placement: Vec<Placement>,
}

#[test]
fn v2_golden_bytes_preserve_panes_placements_and_focus() {
    let bounds = PhysicalRect {
        x: -1920,
        y: 0,
        width: 1920,
        height: 1080,
    };
    let setup = MonitorSetup::new(vec![Monitor {
        bounds,
        work_area: bounds,
        scale: 1.25,
        primary: true,
    }])
    .unwrap();
    let placement = Placement {
        setup: setup.fingerprint(),
        bounds: MonitorLocalLogicalRect {
            x: 10.0,
            y: 20.0,
            width: 800.0,
            height: 600.0,
        },
        monitor: bounds,
        maximized: true,
    };
    let mut expected = old_session();
    expected.windows[1].placement = vec![placement];
    let legacy = LegacySession {
        windows: expected
            .windows
            .iter()
            .map(|window| LegacyWindow {
                id: window.id,
                layout: window.layout.clone(),
                focused_pane: window.focused_pane,
                panes: super::tabs::v4_panes(&window.panes),
                placement: window.placement.clone(),
            })
            .collect(),
        focused_window: expected.focused_window,
        next_id: expected.next_id,
    };
    let bytes = framed(2, &postcard::to_stdvec(&legacy).unwrap());
    assert_eq!(bytes, V2_SESSION);
    let dir = tmp();
    let path = session_file(&dir);
    fs::write(&path, bytes).unwrap();
    assert_eq!(load_session(&path).0, Some(expected));
}

const V2_SESSION: &[u8] = include_bytes!("../../../tests/fixtures/session/v2.session");

#[test]
fn v3_round_trip_keeps_workspaces_and_writes_no_drafts() {
    use rusty_core::{Location, WorkspaceAction, apply_workspace};
    let mut state = AppState::new(Location::new("C:/start").unwrap());
    apply_workspace(
        &mut state,
        &WorkspaceAction::New {
            name: Some("Project".into()),
        },
    )
    .unwrap();
    state.remember_marks(vec!["draft.txt".into()]);
    apply_workspace(&mut state, &WorkspaceAction::Switch { n: 1 }).unwrap();
    let expected = state.to_session(0);
    assert!(
        expected.windows[0]
            .workspaces
            .iter()
            .all(|w| w.live.is_none())
    );
    let dir = tmp();
    let path = session_file(&dir);
    write_session(&path, &expected).unwrap();
    let loaded = load_session(&path).0.unwrap();
    assert_eq!(loaded, expected);
    assert_eq!(
        AppState::from_session(&loaded).unwrap().to_session(0),
        expected
    );
}

#[test]
fn v1_golden_bytes_match_the_original_shape() {
    let expected = old_session();
    let windows: Vec<_> = expected
        .windows
        .iter()
        .map(|window| {
            (
                window.id,
                &window.layout,
                window.focused_pane,
                super::tabs::v4_panes(&window.panes),
            )
        })
        .collect();
    let body = postcard::to_stdvec(&(windows, expected.focused_window, expected.next_id)).unwrap();
    assert_eq!(framed(1, &body), V1_SESSION);
}

#[test]
fn v3_keeps_the_first_nine_workspaces() {
    use rusty_core::{Location, WorkspaceAction, apply_workspace};
    let mut state = AppState::new(Location::new("C:/start").unwrap());
    for _ in 1..9 {
        apply_workspace(&mut state, &WorkspaceAction::New { name: None }).unwrap();
    }
    let mut session = state.to_session(0);
    let expected = session.clone();
    let extra = session.windows[0].workspaces[0].clone();
    session.windows[0]
        .workspaces
        .extend([extra.clone(), extra.clone(), extra]);
    let dir = tmp();
    let path = session_file(&dir);
    put(&path, &session);
    assert_eq!(load_session(&path).0, Some(expected));
}

#[test]
fn v3_drops_broken_workspaces_and_remaps_the_active_index() {
    use rusty_core::{Location, WorkspaceAction, apply_workspace};
    let mut state = AppState::new(Location::new("C:/start").unwrap());
    apply_workspace(&mut state, &WorkspaceAction::New { name: None }).unwrap();
    let mut session = state.to_session(0);
    let retained = session.windows[0].workspaces[1].clone();
    session.windows[0].workspaces[0].name = "x".repeat(41);
    let dir = tmp();
    let path = session_file(&dir);
    put(&path, &session);
    let loaded = load_session(&path).0.unwrap();
    assert_eq!(loaded.windows[0].workspaces, vec![retained]);
    assert_eq!(loaded.windows[0].active_workspace, 0);
}

fn three_panes_two_workspaces() -> AppState {
    use rusty_core::{Command, Direction, Location, WorkspaceAction, apply, apply_workspace};
    let mut state = AppState::new(Location::new("C:/start").unwrap());
    let split = Command::LayoutSplit {
        direction: Direction::Right,
    };
    apply(&mut state, &split).unwrap();
    apply(&mut state, &split).unwrap();
    apply_workspace(&mut state, &WorkspaceAction::New { name: None }).unwrap();
    apply(&mut state, &split).unwrap();
    apply_workspace(&mut state, &WorkspaceAction::Switch { n: 1 }).unwrap();
    state
}

const V3_SESSION: &[u8] = include_bytes!("../../../tests/fixtures/session/v3.session");

#[test]
fn frozen_v3_file_loads_its_workspaces_with_fallback_targets() {
    let dir = tmp();
    let path = session_file(&dir);
    fs::write(&path, V3_SESSION).unwrap();
    let (session, source, errors) = load_session(&path);
    assert_eq!(source, SessionSource::Main);
    assert!(errors.is_empty());
    let session = session.unwrap();
    let mut expected = three_panes_two_workspaces().to_session(0);
    let window = &mut expected.windows[0];
    window.target = None;
    for workspace in &mut window.workspaces {
        for snapshot in std::iter::once(&mut workspace.saved).chain(workspace.live.as_mut()) {
            snapshot.target = None;
        }
    }
    let mut promoted = session.clone();
    for workspace in &mut promoted.windows[0].workspaces {
        if let Some(live) = workspace.live.take() {
            workspace.saved = live;
        }
    }
    assert_eq!(promoted, expected);
    let restored = AppState::from_session(&session).unwrap();
    let window = restored.window(restored.focused_window()).unwrap();
    assert_eq!(window.workspaces().len(), 2);
    assert_eq!(window.layout().numbered().len(), 3);
    assert!(window.target().is_some());
    assert_eq!(fs::read(&path).unwrap(), V3_SESSION);
}
