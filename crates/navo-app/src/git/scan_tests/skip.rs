use std::path::Path;
use std::time::SystemTime;

use navo_git::status::{fingerprint, parse_porcelain_v2};

use super::*;

fn clean() -> RepoStatus {
    parse_porcelain_v2(b"# branch.oid (initial)\0# branch.head main\0", false).unwrap()
}

fn status_with_dirs(count: usize) -> RepoStatus {
    let mut path = String::new();
    for index in 0..count {
        path.push_str(&format!("d{index}/"));
    }
    path.push_str("file");
    let mut output = b"# branch.oid (initial)\0# branch.head main\0".to_vec();
    output.extend_from_slice(format!("? {path}\0").as_bytes());
    parse_porcelain_v2(&output, false).unwrap()
}

fn write_stamp(root: &Path, relative: &str, text: &str, at: SystemTime) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(at)
        .unwrap();
}

/// Two fingerprints of one settled `.git` layout, before and after an index write.
fn prints() -> (Fingerprint, Fingerprint) {
    let dir = tempfile::tempdir().unwrap();
    let old = SystemTime::now() - Duration::from_secs(60);
    let now = SystemTime::now();
    write_stamp(dir.path(), ".git/HEAD", "ref: refs/heads/main\n", old);
    write_stamp(dir.path(), ".git/refs/heads/main", "a\n", old);
    write_stamp(dir.path(), ".git/index", "one", old);
    write_stamp(dir.path(), ".git/config", "[core]\n", old);
    let first = fingerprint(dir.path(), None, now).unwrap();
    write_stamp(dir.path(), ".git/index", "two-longer", old);
    let second = fingerprint(dir.path(), None, now).unwrap();
    assert_ne!(
        first, second,
        "changing the index must change the fingerprint"
    );
    (first, second)
}

fn kept(now: Instant, fingerprint: Fingerprint) -> Kept {
    Kept {
        proof: Proof {
            mark: Mark {
                epoch: 7,
                changes: 11,
            },
            fingerprint,
            upstream: Some("origin/main".into()),
            checked_at: now,
        },
        status: Arc::new(clean()),
    }
}

fn facts(kept: Option<&Kept>, now: Instant) -> Facts {
    Facts {
        forced: false,
        truncated: false,
        class: DriveClass::Ssd,
        kept: kept.cloned(),
        mark: kept.map(|kept| kept.proof.mark),
        now,
    }
}

#[test]
fn precheck_reports_the_first_failure_in_decision_order() {
    let now = Instant::now();
    let mut case = facts(None, now);
    case.forced = true;
    case.truncated = true;
    case.class = DriveClass::Network;
    assert_eq!(precheck(&case), Err(Why::Forced));
    case.forced = false;
    assert_eq!(precheck(&case), Err(Why::Truncated));
    case.truncated = false;
    assert_eq!(precheck(&case), Err(Why::NotFixed));
    case.class = DriveClass::Ssd;
    assert_eq!(precheck(&case), Err(Why::NoProof));
}

#[test]
fn only_ssd_and_hdd_drives_pass_the_class_check() {
    let now = Instant::now();
    let (print, _) = prints();
    let fresh = kept(now, print);
    for class in [
        DriveClass::Network,
        DriveClass::Removable,
        DriveClass::Unknown,
    ] {
        let mut case = facts(Some(&fresh), now);
        case.class = class;
        assert_eq!(
            precheck(&case),
            Err(Why::NotFixed),
            "{class:?} is not fixed"
        );
    }
    for class in [DriveClass::Ssd, DriveClass::Hdd] {
        let mut case = facts(Some(&fresh), now);
        case.class = class;
        assert_eq!(precheck(&case), Ok(()), "{class:?} is fixed");
    }
}

#[test]
fn a_proof_is_fresh_until_five_minutes_and_never_in_the_future() {
    let now = Instant::now();
    let (print, _) = prints();
    let fresh = facts(Some(&kept(now - Duration::from_secs(299), print)), now);
    assert_eq!(precheck(&fresh), Ok(()));
    let expired = facts(Some(&kept(now - MAX_PROOF_AGE, print)), now);
    assert_eq!(precheck(&expired), Err(Why::Expired));
    let ahead = facts(Some(&kept(now + Duration::from_secs(1), print)), now);
    assert_eq!(precheck(&ahead), Err(Why::Expired));
}

#[test]
fn a_root_without_a_live_watch_has_no_coverage() {
    let now = Instant::now();
    let (print, _) = prints();
    let mut case = facts(Some(&kept(now, print)), now);
    case.mark = None;
    assert_eq!(precheck(&case), Err(Why::NoCoverage));
}

#[test]
fn a_new_epoch_or_one_more_change_is_watched() {
    let now = Instant::now();
    let (print, _) = prints();
    let fresh = kept(now, print);
    let mark = fresh.proof.mark;
    let mut epoch = facts(Some(&fresh), now);
    epoch.mark = Some(Mark {
        epoch: mark.epoch + 1,
        changes: mark.changes,
    });
    assert_eq!(precheck(&epoch), Err(Why::Watched));
    let mut changes = facts(Some(&fresh), now);
    changes.mark = Some(Mark {
        epoch: mark.epoch,
        changes: mark.changes + 1,
    });
    assert_eq!(precheck(&changes), Err(Why::Watched));
}

#[test]
fn equal_marks_and_fingerprints_reuse_the_kept_status() {
    let now = Instant::now();
    let (print, _) = prints();
    let fresh = kept(now, print);
    let case = facts(Some(&fresh), now);
    assert_eq!(precheck(&case), Ok(()));
    assert_eq!(confirm(&case, || Some(print)), Verdict::Reuse);
}

#[test]
fn confirm_reuses_only_when_the_fingerprint_is_present_and_equal() {
    let now = Instant::now();
    let (print, changed) = prints();
    let case = facts(Some(&kept(now, print)), now);
    assert_eq!(confirm(&case, || None), Verdict::Unproved);
    assert_eq!(confirm(&case, || Some(changed)), Verdict::Fingerprint);
}

#[test]
fn a_failed_precheck_never_reads_the_fingerprint() {
    let now = Instant::now();
    let (print, _) = prints();
    let fresh = kept(now, print);
    let mut forced = facts(Some(&fresh), now);
    forced.forced = true;
    let mut truncated = facts(Some(&fresh), now);
    truncated.truncated = true;
    let mut network = facts(Some(&fresh), now);
    network.class = DriveClass::Network;
    let expired = facts(Some(&kept(now - MAX_PROOF_AGE, print)), now);
    let mut uncovered = facts(Some(&fresh), now);
    uncovered.mark = None;
    let mut watched = facts(Some(&fresh), now);
    watched.mark = Some(Mark {
        epoch: 0,
        changes: 0,
    });
    let cases = [
        forced,
        truncated,
        network,
        facts(None, now),
        expired,
        uncovered,
        watched,
    ];
    for case in cases {
        let mut called = false;
        let verdict = confirm(&case, || {
            called = true;
            Some(print)
        });
        assert_eq!(verdict, Verdict::Unproved);
        assert!(
            !called,
            "the precheck must fail before the fingerprint is read"
        );
    }
}

#[test]
fn a_proof_needs_a_mark_and_two_equal_fingerprints() {
    let now = Instant::now();
    let (print, changed) = prints();
    let mark = Mark {
        epoch: 1,
        changes: 0,
    };
    assert_eq!(proof(None, Some(print), Some(print), None, None, now), None);
    assert_eq!(proof(Some(mark), None, Some(print), None, None, now), None);
    assert_eq!(proof(Some(mark), Some(print), None, None, None, now), None);
    assert_eq!(
        proof(Some(mark), Some(print), Some(changed), None, None, now),
        None
    );
    assert_eq!(
        proof(
            Some(mark),
            Some(print),
            Some(print),
            None,
            Some("origin/main"),
            now
        ),
        None
    );
    assert_eq!(
        proof(
            Some(mark),
            Some(print),
            Some(print),
            Some("origin/main"),
            Some("origin/dev"),
            now
        ),
        None
    );
    assert_eq!(
        proof(
            Some(mark),
            Some(print),
            Some(print),
            Some("origin/main"),
            None,
            now
        ),
        None
    );
}

#[test]
fn a_proved_scan_keeps_its_mark_fingerprint_and_upstream() {
    let now = Instant::now();
    let (print, _) = prints();
    let mark = Mark {
        epoch: 4,
        changes: 2,
    };
    assert_eq!(
        proof(
            Some(mark),
            Some(print),
            Some(print),
            Some("origin/main"),
            Some("origin/main"),
            now
        ),
        Some(Proof {
            mark,
            fingerprint: print,
            upstream: Some("origin/main".into()),
            checked_at: now,
        })
    );
    let no_upstream = proof(Some(mark), Some(print), Some(print), None, None, now).unwrap();
    assert_eq!(no_upstream.upstream, None);
    assert_eq!(no_upstream.checked_at, now);
}

#[test]
fn sixty_five_proofs_evict_the_least_recently_used() {
    let now = Instant::now();
    let (print, _) = prints();
    let mut proofs = Proofs::default();
    for index in 0..WATCH_CAP {
        proofs.insert(format!("root-{index}"), kept(now, print));
    }
    assert_eq!(proofs.entries.len(), WATCH_CAP);
    assert!(proofs.get("root-0").is_some());
    proofs.insert("root-64".into(), kept(now, print));
    assert_eq!(proofs.entries.len(), WATCH_CAP);
    assert!(
        proofs.get("root-0").is_some(),
        "a read entry is the last to go"
    );
    assert!(proofs.get("root-1").is_none());
    assert!(proofs.get("root-64").is_some());
}

#[test]
fn proof_bytes_evict_until_the_new_status_fits() {
    let now = Instant::now();
    let (print, _) = prints();
    let big = Kept {
        proof: Proof {
            mark: Mark {
                epoch: 1,
                changes: 0,
            },
            fingerprint: print,
            upstream: None,
            checked_at: now,
        },
        status: Arc::new(status_with_dirs(4096)),
    };
    assert!(
        big.bytes() * 9 > PROOF_BYTES,
        "the fixture must exceed the byte cap"
    );
    let mut proofs = Proofs::default();
    for index in 0..9 {
        proofs.insert(format!("root-{index}"), big.clone());
    }
    assert!(proofs.entries.len() < 9);
    assert!(proofs.bytes <= PROOF_BYTES);
    assert!(proofs.get("root-0").is_none());
    assert!(proofs.get("root-8").is_some());
}

#[test]
fn removing_a_proof_returns_it_once_and_frees_its_bytes() {
    let now = Instant::now();
    let (print, _) = prints();
    let mut proofs = Proofs::default();
    proofs.insert("root".into(), kept(now, print));
    assert!(proofs.bytes > 0);
    assert!(proofs.remove("root").is_some());
    assert!(proofs.remove("root").is_none());
    assert!(proofs.get("root").is_none());
    assert_eq!(proofs.bytes, 0);
}

#[test]
fn deadlines_return_each_due_value_once_in_time_order() {
    let now = Instant::now();
    let mut deadlines = Deadlines::default();
    deadlines.insert(now + CONFIRM_WAIT, "later");
    deadlines.insert(now, "first");
    deadlines.insert(now + Duration::from_millis(500), "second");
    assert_eq!(deadlines.next(), Some(now));
    assert_eq!(deadlines.pop_due(now), vec!["first"]);
    assert_eq!(deadlines.next(), Some(now + Duration::from_millis(500)));
    assert_eq!(
        deadlines.pop_due(now + CONFIRM_WAIT),
        vec!["second", "later"]
    );
    assert_eq!(deadlines.next(), None);
    assert!(deadlines.pop_due(now).is_empty());
}

#[test]
fn deadlines_retain_keeps_only_the_matching_values() {
    let now = Instant::now();
    let mut deadlines = Deadlines::default();
    deadlines.insert(now, 1);
    deadlines.insert(now + CONFIRM_WAIT, 2);
    deadlines.retain(|value| *value == 2);
    assert_eq!(deadlines.next(), Some(now + CONFIRM_WAIT));
    assert_eq!(deadlines.pop_due(now + CONFIRM_WAIT), vec![2]);
}
