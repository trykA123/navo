use super::*;
use navo_core::placement::{
    Monitor, MonitorLocalLogicalRect, MonitorSetup, PhysicalRect, Placement,
};

fn placed() -> SessionState {
    let bounds = PhysicalRect {
        x: 0,
        y: 0,
        width: 2560,
        height: 1440,
    };
    let setup = MonitorSetup::new(vec![Monitor {
        bounds,
        work_area: bounds,
        scale: 1.5,
        primary: true,
    }])
    .unwrap();
    let mut session = first();
    session.windows[0].remember(
        &setup,
        Placement {
            setup: setup.fingerprint(),
            bounds: MonitorLocalLogicalRect {
                x: 40.0,
                y: 30.0,
                width: 1200.0,
                height: 800.0,
            },
            monitor: bounds,
            maximized: false,
        },
    );
    session
}

const V5_SESSION: &[u8] = include_bytes!("../../../tests/fixtures/session/v5.session");

#[test]
fn frozen_v5_file_loads_with_its_placement_and_stays_unchanged() {
    let dir = tmp();
    let path = session_file(&dir);
    fs::write(&path, V5_SESSION).unwrap();
    let (loaded, source, errors) = load_session(&path);
    assert_eq!((loaded, source), (Some(placed()), SessionSource::Main));
    assert!(errors.is_empty());
    assert_eq!(fs::read(&path).unwrap(), V5_SESSION);
}

#[test]
fn frozen_v5_file_loads_with_an_empty_find_window() {
    let dir = tmp();
    let path = session_file(&dir);
    fs::write(&path, V5_SESSION).unwrap();
    let loaded = load_session(&path).0.unwrap();
    assert!(loaded.windows[0].find_window.is_empty());
    let mut expected = placed();
    expected.windows[0].find_window = FindMemoryState::default();
    assert_eq!(loaded, expected);
}

#[test]
fn an_empty_find_window_round_trips_through_the_current_file() {
    let dir = tmp();
    let path = session_file(&dir);
    write_session(&path, &placed()).unwrap();
    assert_eq!(load_session(&path).0, Some(placed()));
}

#[test]
fn a_find_window_with_two_queries_and_a_placement_round_trips() {
    let mut session = placed();
    let placement = session.windows[0].placement.clone();
    session.windows[0].find_window = FindMemoryState {
        placement,
        pinned: true,
        queries: vec![
            SavedFindState {
                text: "*.rs".into(),
                base: "/home".into(),
            },
            SavedFindState {
                text: "todo".into(),
                base: "/work".into(),
            },
        ],
    };
    let dir = tmp();
    let path = session_file(&dir);
    write_session(&path, &session).unwrap();
    assert_eq!(load_session(&path).0, Some(session));
}

#[test]
fn writing_over_a_v5_main_keeps_its_bytes_as_backup_and_writes_v6() {
    let dir = tmp();
    let path = session_file(&dir);
    fs::write(&path, V5_SESSION).unwrap();
    let loaded = load_session(&path).0.unwrap();
    write_session(&path, &loaded).unwrap();
    assert_eq!(fs::read(backup_path(&path)).unwrap(), V5_SESSION);
    let header = u16::from_le_bytes(fs::read(&path).unwrap()[16..18].try_into().unwrap());
    assert_eq!((header, SESSION_VERSION), (6, 6));
    assert_eq!(read_session_file(&path).unwrap(), loaded);
}

#[test]
fn a_v5_body_is_not_read_as_the_current_version() {
    let dir = tmp();
    let path = session_file(&dir);
    let mut bytes = V5_SESSION.to_vec();
    bytes[16..18].copy_from_slice(&SESSION_VERSION.to_le_bytes());
    fs::write(&path, bytes).unwrap();
    assert!(read_session_file(&path).is_err());
}
