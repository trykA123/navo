use navo_index::pipe::ServiceVersion;
use navo_update::InstallKind;

use super::*;

const OWN: u32 = 4;

fn running(version: Option<ServiceVersion>) -> ServiceQuery {
    ServiceQuery {
        running: Some(true),
        version,
        legacy_present: false,
    }
}

fn known(revision: u32) -> Option<ServiceVersion> {
    Some(ServiceVersion::Known {
        revision,
        build: "test".into(),
    })
}

fn step(kind: InstallKind, query: &ServiceQuery, declined: Option<u32>) -> Step {
    service_step(kind, query, OWN, declined)
}

#[test]
fn nothing_happens_for_a_portable_copy_a_missing_or_stopped_service_or_a_current_one() {
    let older = running(Some(ServiceVersion::Older));
    let cases = [
        (InstallKind::Portable, older.clone()),
        (InstallKind::Unpackaged, older.clone()),
        (
            InstallKind::Installed,
            ServiceQuery {
                running: None,
                ..older.clone()
            },
        ),
        (
            InstallKind::Installed,
            ServiceQuery {
                running: Some(false),
                ..older
            },
        ),
        (InstallKind::Installed, running(known(OWN))),
        (InstallKind::Installed, running(known(OWN + 1))),
        (InstallKind::Installed, running(None)),
    ];
    for (kind, query) in cases {
        assert_eq!(
            step(kind, &query, None),
            Step::Nothing,
            "{kind:?} {query:?}"
        );
    }
}

#[test]
fn an_installed_navo_asks_to_update_a_lower_or_pre_version_service() {
    for version in [known(OWN - 1), Some(ServiceVersion::Older)] {
        let query = running(version);
        assert_eq!(step(InstallKind::Installed, &query, None), Step::Elevate);
    }
}

#[test]
fn a_declined_revision_is_not_asked_again_but_a_newer_one_is() {
    let query = running(Some(ServiceVersion::Older));
    assert_eq!(
        step(InstallKind::Installed, &query, Some(OWN)),
        Step::Outdated
    );
    assert_eq!(
        step(InstallKind::Installed, &query, Some(OWN - 1)),
        Step::Elevate
    );
}

#[test]
fn only_a_successful_prompt_without_an_older_answer_counts_as_updated() {
    assert!(is_updated(true, known(OWN).as_ref(), OWN));
    assert!(is_updated(true, None, OWN));
    assert!(!is_updated(false, known(OWN).as_ref(), OWN));
    assert!(!is_updated(true, Some(&ServiceVersion::Older), OWN));
    assert!(!is_updated(true, known(OWN - 1).as_ref(), OWN));
}

#[test]
fn the_declined_marker_round_trips_and_garbage_reads_as_none() {
    let root = tempfile::tempdir().unwrap();
    let marker = root.path().join(DECLINED);
    assert_eq!(read_declined(&marker), None);
    write_declined(&marker, OWN).unwrap();
    assert_eq!(read_declined(&marker), Some(OWN));
    std::fs::write(&marker, b"not a number").unwrap();
    assert_eq!(read_declined(&marker), None);
}
