//! Checks for the running-Git registry: path keys, pause refusal, stops and deadlines.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use crate::GitError;

use super::{Running, Work, covers, key, overlaps};

/// A fresh registry per test, leaked because tickets borrow it for their lifetime.
fn registry() -> &'static Running {
    Box::leak(Box::new(Running::new()))
}

/// A key covers itself and its descendants, but never an ancestor.
#[test]
fn a_key_covers_itself_and_its_descendants() {
    let parent = key(Path::new("repo"));
    let child = key(Path::new("repo/sub"));
    assert!(covers(&parent, &parent));
    assert!(covers(&parent, &child));
    assert!(!covers(&child, &parent));
}

/// Overlap is symmetric for a parent and a descendant.
#[test]
fn overlap_is_symmetric_for_a_parent_and_a_descendant() {
    let parent = key(Path::new("repo"));
    let child = key(Path::new("repo/sub"));
    assert!(overlaps(&parent, &parent));
    assert!(overlaps(&parent, &child));
    assert!(overlaps(&child, &parent));
}

/// A common prefix is not an ancestor: `repo2` stays outside `repo`.
#[test]
fn a_shared_prefix_does_not_cover_a_sibling_name() {
    let repo = key(Path::new("repo"));
    let sibling = key(Path::new("repo2"));
    assert!(!covers(&repo, &sibling));
    assert!(!covers(&sibling, &repo));
    assert!(!overlaps(&repo, &sibling));
}

/// A trailing separator names the same folder.
#[test]
fn a_trailing_separator_does_not_change_a_key() {
    assert_eq!(key(Path::new("repo/")), key(Path::new("repo")));
    assert!(covers(
        &key(Path::new("repo/")),
        &key(Path::new("repo/sub"))
    ));
}

/// Windows keys fold case and drop the verbatim prefix.
#[cfg(windows)]
#[test]
fn windows_keys_fold_case_and_the_verbatim_prefix() {
    let plain = key(Path::new(r"C:\Repo"));
    let verbatim = key(Path::new(r"\\?\c:\repo"));
    assert_eq!(plain, verbatim);
    assert!(covers(&plain, &key(Path::new(r"\\?\c:\REPO\sub"))));
    assert_eq!(
        key(Path::new(r"\\?\UNC\srv\share\x")),
        key(Path::new(r"\\SRV\Share\x"))
    );
    assert_eq!(key(Path::new("C:/repo")), key(Path::new(r"c:\REPO")));
}

/// A pause refuses new children, and dropping it lets them start again.
#[test]
fn a_pause_refuses_registrations_until_it_drops() {
    let running = registry();
    let repo = Path::new("repo");
    assert!(running.register(repo, Work::Read).is_ok());
    let pause = running.stop_under(repo, Duration::from_millis(50)).unwrap();
    assert!(matches!(
        running.register(repo, Work::Read),
        Err(GitError::Paused)
    ));
    assert!(matches!(
        running.register(repo, Work::Write),
        Err(GitError::Paused)
    ));
    drop(pause);
    assert!(running.register(repo, Work::Read).is_ok());
}

/// A covered read is stopped, and the pause returns only after its ticket drops.
#[test]
fn a_covered_read_is_stopped_and_waited_for() {
    let running = registry();
    let repo = Path::new("repo");
    let ticket = running.register(repo, Work::Read).unwrap();
    let observed = ticket.stop_flag();
    let waiting = observed.clone();
    let dropped = Arc::new(AtomicBool::new(false));
    let signal = Arc::clone(&dropped);
    let dropper = thread::spawn(move || {
        while !waiting.is_cancelled() {
            thread::yield_now();
        }
        thread::sleep(Duration::from_millis(20));
        signal.store(true, Ordering::Release);
        drop(ticket);
    });

    let pause = running
        .stop_under(repo, Duration::from_millis(200))
        .unwrap();
    assert!(observed.is_cancelled());
    assert!(
        dropped.load(Ordering::Acquire),
        "stop_under must wait for the read ticket to drop"
    );
    dropper.join().unwrap();
    drop(pause);
}

/// A pause inside a pause on the same folder returns at once without waiting.
#[test]
fn a_nested_pause_returns_without_waiting() {
    let running = registry();
    let outer = running
        .stop_under(Path::new("repo"), Duration::from_millis(200))
        .unwrap();
    let began = Instant::now();
    let inner = running
        .stop_under(Path::new("repo/sub"), Duration::from_millis(200))
        .unwrap();
    assert!(
        began.elapsed() < Duration::from_millis(100),
        "a nested pause must not wait"
    );
    drop(inner);
    drop(outer);
}

/// A read in a parent repository is stopped for a path inside its submodule.
#[test]
fn a_parent_read_is_stopped_for_a_submodule_path() {
    let running = registry();
    let ticket = running.register(Path::new("repo"), Work::Read).unwrap();
    let observed = ticket.stop_flag();
    let waiting = observed.clone();
    let dropped = Arc::new(AtomicBool::new(false));
    let signal = Arc::clone(&dropped);
    let dropper = thread::spawn(move || {
        while !waiting.is_cancelled() {
            thread::yield_now();
        }
        thread::sleep(Duration::from_millis(20));
        signal.store(true, Ordering::Release);
        drop(ticket);
    });

    let pause = running
        .stop_under(Path::new("repo/sub"), Duration::from_millis(200))
        .unwrap();
    assert!(observed.is_cancelled());
    assert!(
        dropped.load(Ordering::Acquire),
        "stop_under must wait for the read ticket to drop"
    );
    dropper.join().unwrap();
    drop(pause);
}

/// A session repository keeps running when a pause covers its submodule.
#[test]
fn a_session_repository_survives_a_submodule_pause() {
    let running = registry();
    let repo = Path::new("repo");
    let session = running.register(repo, Work::Write).unwrap();
    let read = running.register(repo, Work::Read).unwrap();
    let observed = read.stop_flag();

    let pause = running
        .stop_under(Path::new("repo/sub"), Duration::from_millis(50))
        .unwrap();
    assert!(!observed.is_cancelled());
    assert!(running.register(repo, Work::Read).is_ok());
    assert!(matches!(
        running.register(Path::new("repo/sub"), Work::Read),
        Err(GitError::Paused)
    ));
    assert!(matches!(
        running.register(repo, Work::Write),
        Err(GitError::Paused)
    ));
    drop(pause);
    drop(session);
    drop(read);
}

/// A read that never drops gives `GitBusy` at the deadline and leaves no pause.
#[test]
fn a_read_that_never_drops_gives_busy_and_leaves_no_pause() {
    let running = registry();
    let repo = Path::new("repo");
    let ticket = running.register(repo, Work::Read).unwrap();
    let Err(busy) = running.stop_under(repo, Duration::from_millis(50)) else {
        panic!("a read that never drops must hold the pause");
    };
    assert_eq!(busy.repo, PathBuf::from(repo));
    assert!(running.register(repo, Work::Read).is_ok());
    drop(ticket);
}

/// A covered write is not stopped, and its reads stop only after it drops.
#[test]
fn a_covered_write_is_waited_for_before_its_reads_stop() {
    let running = registry();
    let repo = Path::new("repo");
    let session = running.register(repo, Work::Write).unwrap();
    let write_flag = session.stop_flag();
    let read = running.register(Path::new("repo/sub"), Work::Read).unwrap();
    let observed = read.stop_flag();
    let waiting = observed.clone();
    let (started_tx, started_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();
    let stopper = thread::spawn(move || {
        started_tx.send(()).unwrap();
        result_tx
            .send(running.stop_under(repo, Duration::from_millis(200)))
            .unwrap();
    });
    let (release_tx, release_rx) = mpsc::channel();
    let dropper = thread::spawn(move || {
        release_rx.recv().unwrap();
        assert!(
            !waiting.is_cancelled(),
            "the read must not stop while the write session lives"
        );
        drop(session);
        while !waiting.is_cancelled() {
            thread::yield_now();
        }
        drop(read);
    });

    started_rx.recv().unwrap();
    thread::sleep(Duration::from_millis(5));
    assert!(
        !observed.is_cancelled(),
        "the read must not be flagged while the write session lives"
    );
    release_tx.send(()).unwrap();
    let pause = result_rx
        .recv()
        .unwrap()
        .expect("the write session must let the pause through");
    assert!(!write_flag.is_cancelled());
    assert!(observed.is_cancelled());
    dropper.join().unwrap();
    stopper.join().unwrap();
    drop(pause);
}

/// A write that outlives the deadline leaves its reads running and installs no pause.
#[test]
fn a_write_that_outlives_the_deadline_changes_nothing() {
    let running = registry();
    let repo = Path::new("repo");
    let session = running.register(repo, Work::Write).unwrap();
    let read = running.register(Path::new("repo/sub"), Work::Read).unwrap();
    let observed = read.stop_flag();
    let Err(busy) = running.stop_under(repo, Duration::from_millis(50)) else {
        panic!("a live write session must hold the pause");
    };
    assert_eq!(busy.repo, PathBuf::from(repo));
    assert!(!observed.is_cancelled());
    assert!(
        running.register(Path::new("repo/sub"), Work::Read).is_ok(),
        "a timed-out pause must be removed"
    );
    drop(read);
    drop(session);
}

/// A wait that cannot be added to `Instant` still returns when nothing blocks it.
#[test]
fn a_huge_wait_does_not_panic() {
    let running = registry();
    let started = Instant::now();
    let pause = running
        .stop_under(Path::new("repo"), Duration::MAX)
        .unwrap();
    assert!(
        started.elapsed() < Duration::from_millis(50),
        "an empty pause must return at once"
    );
    drop(pause);
}

/// The debug form of a pause names its id.
#[test]
fn a_pause_debug_prints_its_id() {
    let running = registry();
    let pause = running
        .stop_under(Path::new("repo"), Duration::from_millis(50))
        .unwrap();
    let text = format!("{pause:?}");
    assert!(text.contains("id"), "{text}");
    drop(pause);
}

/// One deadline bounds the write wait and the read wait together.
#[test]
fn one_deadline_bounds_the_write_and_read_waits() {
    let running = registry();
    let repo = Path::new("repo");
    let session = running.register(repo, Work::Write).unwrap();
    let read = running.register(Path::new("repo/sub"), Work::Read).unwrap();
    let dropper = thread::spawn(move || {
        thread::sleep(Duration::from_millis(80));
        drop(session);
    });

    let started = Instant::now();
    let Err(busy) = running.stop_under(repo, Duration::from_millis(100)) else {
        panic!("the read that never drops must end the pause");
    };
    let elapsed = started.elapsed();
    assert!(elapsed >= Duration::from_millis(75), "waited {elapsed:?}");
    assert!(elapsed <= Duration::from_millis(150), "waited {elapsed:?}");
    assert_eq!(busy.repo, PathBuf::from("repo/sub"));
    dropper.join().unwrap();
    drop(read);
}

/// A pause over one folder leaves children elsewhere untouched.
#[test]
fn tickets_outside_the_path_are_untouched() {
    let running = registry();
    let outside = running.register(Path::new("other"), Work::Read).unwrap();
    let observed = outside.stop_flag();
    let pause = running
        .stop_under(Path::new("repo"), Duration::from_millis(50))
        .unwrap();
    assert!(!observed.is_cancelled());
    drop(pause);
    drop(outside);
}
