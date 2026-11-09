use navo_index::pipe::ServiceVersion;
use navo_update::InstallKind;

use super::*;

const OWN: u32 = 3;

fn running(version: Option<ServiceVersion>) -> ServiceQuery {
    ServiceQuery {
        running: Some(true),
        version,
        legacy_present: false,
        legacy_image: false,
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
fn only_a_successful_prompt_with_a_current_answer_counts_as_updated() {
    assert!(is_updated(true, known(OWN).as_ref(), OWN));
    assert!(is_updated(true, known(OWN + 1).as_ref(), OWN));
    assert!(!is_updated(true, None, OWN));
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

#[test]
fn a_held_change_lock_skips_the_recheck_and_prompt() {
    prompt_after_lock(
        Ok(None::<()>),
        || unreachable!("another Navo owns the change"),
        || unreachable!("another Navo owns the prompt"),
    )
    .unwrap();
}

#[test]
fn a_service_updated_or_declined_before_the_recheck_never_prompts() {
    let older = running(known(OWN - 1));
    assert_eq!(step(InstallKind::Installed, &older, None), Step::Elevate);
    for (query, declined) in [(running(known(OWN)), None), (older, Some(OWN))] {
        prompt_after_lock(
            Ok(Some(())),
            || Ok(step(InstallKind::Installed, &query, declined)),
            || unreachable!("the fresh query no longer needs a prompt"),
        )
        .unwrap();
    }
}

#[test]
fn failed_lock_or_recheck_never_prompts() {
    let denied = || io::Error::from(io::ErrorKind::PermissionDenied);
    let result = prompt_after_lock::<()>(
        Err(denied()),
        || unreachable!("the guard was not acquired"),
        || unreachable!("the guard was not acquired"),
    );
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    let result = prompt_after_lock(
        Ok(Some(())),
        || Err(denied()),
        || unreachable!("a failed query cannot prompt"),
    );
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
}

#[test]
fn the_change_guard_stays_held_through_recheck_and_prompt() {
    use std::cell::Cell;

    struct Guard<'a>(&'a Cell<bool>);
    impl Drop for Guard<'_> {
        fn drop(&mut self) {
            self.0.set(false);
        }
    }
    let held = Cell::new(true);
    let prompted = Cell::new(false);
    prompt_after_lock(
        Ok(Some(Guard(&held))),
        || {
            assert!(held.get());
            Ok(step(InstallKind::Installed, &running(known(OWN - 1)), None))
        },
        || {
            assert!(held.get());
            prompted.set(true);
        },
    )
    .unwrap();
    assert!(prompted.get());
    assert!(!held.get());
}
