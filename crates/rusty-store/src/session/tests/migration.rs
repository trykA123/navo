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
    session
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
        2
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
