use super::find_window::placed;
use super::*;
use navo_core::placement::{
    Monitor, MonitorLocalLogicalRect, MonitorSetup, PhysicalRect, Placement,
};

const V6_SESSION: &[u8] = include_bytes!("../../../tests/fixtures/session/v6.session");

fn setup(x: i32) -> MonitorSetup {
    let bounds = PhysicalRect {
        x,
        y: 0,
        width: 1920,
        height: 1080,
    };
    MonitorSetup::new(vec![Monitor {
        bounds,
        work_area: bounds,
        scale: 1.0,
        primary: true,
    }])
    .unwrap()
}

fn placement(setup: &MonitorSetup, x: f64) -> Placement {
    Placement {
        setup: setup.fingerprint(),
        bounds: MonitorLocalLogicalRect {
            x,
            y: 20.0,
            width: 900.0,
            height: 700.0,
        },
        monitor: setup.monitors()[0].bounds,
        maximized: false,
    }
}

#[test]
fn frozen_v6_file_loads_with_empty_settings_window() {
    assert_eq!(
        u16::from_le_bytes(V6_SESSION[16..18].try_into().unwrap()),
        6
    );
    let dir = tmp();
    let path = session_file(&dir);
    fs::write(&path, V6_SESSION).unwrap();
    let (loaded, source, errors) = load_session(&path);
    assert_eq!(source, SessionSource::Main);
    assert!(errors.is_empty());
    let loaded = loaded.unwrap();
    assert!(loaded.settings_window.is_empty());
    assert_eq!(loaded, placed());
}

#[test]
fn v7_with_empty_settings_window_round_trips() {
    let dir = tmp();
    let path = session_file(&dir);
    let session = placed();
    assert!(session.settings_window.is_empty());
    write_session(&path, &session).unwrap();
    assert_eq!(load_session(&path).0, Some(session));
}

#[test]
fn v7_with_two_settings_placements_round_trips() {
    let first = setup(0);
    let second = setup(2000);
    let mut session = placed();
    session.settings_window = vec![placement(&first, 10.0), placement(&second, 20.0)];
    let dir = tmp();
    let path = session_file(&dir);
    write_session(&path, &session).unwrap();
    assert_eq!(load_session(&path).0, Some(session));
}

#[test]
fn nine_settings_placements_decode_as_eight() {
    let mut session = placed();
    session.settings_window = (0..9)
        .map(|i| {
            let setup = setup(i * 2000);
            placement(&setup, f64::from(i))
        })
        .collect();
    let bytes = postcard::to_stdvec(&session).unwrap();
    let decoded: SessionState = postcard::from_bytes(&bytes).unwrap();
    assert_eq!(decoded.settings_window.len(), 8);
    assert_eq!(decoded.settings_window, session.settings_window[..8]);
}

#[test]
fn writing_over_a_v6_main_keeps_its_bytes_as_backup_and_writes_v7() {
    let dir = tmp();
    let path = session_file(&dir);
    fs::write(&path, V6_SESSION).unwrap();
    let loaded = load_session(&path).0.unwrap();
    write_session(&path, &loaded).unwrap();
    assert_eq!(fs::read(backup_path(&path)).unwrap(), V6_SESSION);
    let header = u16::from_le_bytes(fs::read(&path).unwrap()[16..18].try_into().unwrap());
    assert_eq!((header, SESSION_VERSION), (7, 7));
    assert_eq!(read_session_file(&path).unwrap(), loaded);
}

#[test]
fn a_v6_body_is_not_read_as_the_current_version() {
    let dir = tmp();
    let path = session_file(&dir);
    let mut bytes = V6_SESSION.to_vec();
    bytes[16..18].copy_from_slice(&SESSION_VERSION.to_le_bytes());
    fs::write(&path, bytes).unwrap();
    assert!(read_session_file(&path).is_err());
}
