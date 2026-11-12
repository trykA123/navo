//! Regression tests for IXM1: the walker treats "sizes unknown" as a note, not a failure.

use super::*;
use navo_index::pipe::VolumeStatus;

fn replied(status: &Status) -> Service<'_> {
    Service {
        reply: Some(status),
        volumes: None,
    }
}

fn fixed_c() -> Vec<Drive> {
    navo_shell::assemble(&navo_shell::RawDrives {
        mask: 0b100,
        kinds: vec![('C', DriveKind::Fixed)],
        ..Default::default()
    })
}

#[test]
fn a_volume_whose_only_error_is_unknown_sizes_is_covered() {
    let root = Location::new(r"C:\").unwrap();
    let status = Status {
        volumes: vec![VolumeStatus {
            root: "C:".into(),
            entries: 10,
            errors: vec!["sizes unknown: 7".into()],
            ..Default::default()
        }],
    };
    assert!(covered(&root, Some(&status)));
}

#[test]
fn configured_does_not_add_a_volume_whose_only_error_is_unknown_sizes_as_a_root() {
    let status = Status {
        volumes: vec![
            VolumeStatus {
                root: "C:".into(),
                entries: 10,
                ..Default::default()
            },
            VolumeStatus {
                root: "F:".into(),
                entries: 4,
                errors: vec!["sizes unknown: 7".into()],
                ..Default::default()
            },
        ],
    };
    let found = configured(
        &IndexOptions::default(),
        &fixed_c(),
        Vec::new(),
        replied(&status),
    );
    assert_eq!(found, [Location::new(r"C:\").unwrap()]);
}

#[test]
fn a_volume_with_another_error_is_still_not_covered() {
    let root = Location::new(r"C:\").unwrap();
    let status = Status {
        volumes: vec![VolumeStatus {
            root: "C:".into(),
            entries: 10,
            errors: vec!["unrelated error".into()],
            ..Default::default()
        }],
    };
    assert!(!covered(&root, Some(&status)));
}
