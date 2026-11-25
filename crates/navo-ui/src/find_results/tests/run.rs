//! Background run tests for the find results window.

use std::ops::ControlFlow;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use navo_core::Location;
use navo_core::find::{FileHits, FindQuery};
use navo_core::{EntryKind, ProviderPath};
use navo_fs::find::{FindError, FindProgress, FoundBatch, FoundFile};
use navo_fs::{Attributes, FsEntry};

use crate::command_center::content_source::ContentWalk;
use crate::find_results::run::{FindRun, RunEvent, start};

const WAIT: Duration = Duration::from_secs(10);

fn batch(name: &str) -> FoundBatch {
    FoundBatch {
        files: vec![FoundFile {
            relative: ProviderPath::from_text(name),
            entry: FsEntry {
                name: ProviderPath::from_text(name),
                kind: EntryKind::File,
                size: 1,
                modified_ms: None,
                attributes: Attributes::default(),
            },
            hits: FileHits::default(),
        }],
        progress: FindProgress::default(),
    }
}

/// A walk that fails for roots named `bad` and finds one file elsewhere, recording each call.
fn fake(calls: Arc<Mutex<Vec<(String, usize)>>>) -> ContentWalk {
    Arc::new(move |root, query, _, sink| {
        calls
            .lock()
            .expect("the call log is not poisoned")
            .push((root.as_str().to_owned(), query.within.len()));
        if root.as_str().ends_with("bad") {
            return Err(FindError::Invalid("broken".into()));
        }
        let _ = sink(batch("found.txt"));
        Ok(FindProgress {
            scanned: 3,
            ..FindProgress::default()
        })
    })
}

fn query(within: &[&str]) -> FindQuery {
    let mut query = FindQuery::parse("needle");
    query.within = within.iter().map(|value| (*value).to_owned()).collect();
    query
}

fn events(run: &FindRun) -> Vec<RunEvent> {
    let mut events = Vec::new();
    loop {
        let event = run.receive.recv_timeout(WAIT).expect("the run ends");
        let done = matches!(event, RunEvent::Done(_));
        events.push(event);
        if done {
            return events;
        }
    }
}

fn location(text: &str) -> Location {
    Location::new(text).expect("a valid location")
}

#[test]
fn two_roots_one_failing_report_the_failure_and_finish() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let (run, roots) = start(
        fake(Arc::clone(&calls)),
        location("/tmp/navo-run/base"),
        query(&["one", "bad", "../../../../up"]),
    );
    assert_eq!(
        roots,
        vec![
            location("/tmp/navo-run/base/one"),
            location("/tmp/navo-run/base/bad")
        ]
    );
    let events = events(&run);
    assert!(
        matches!(&events[0], RunEvent::RootFailed { root: 2, .. }),
        "the unresolved root fails at once: {events:?}"
    );
    assert!(matches!(&events[1], RunEvent::Batch { root: 0, batch } if batch.files.len() == 1));
    assert!(
        matches!(&events[2], RunEvent::RootFailed { root: 1, error } if error.contains("broken"))
    );
    assert!(matches!(&events[3], RunEvent::Done(Ok(progress)) if progress.scanned == 3));
    assert_eq!(events.len(), 4);
    let calls = calls.lock().expect("the call log is not poisoned");
    assert_eq!(
        *calls,
        vec![
            ("/tmp/navo-run/base/one".to_owned(), 0),
            ("/tmp/navo-run/base/bad".to_owned(), 0)
        ]
    );
}

#[test]
fn empty_within_walks_the_base_and_its_failure_is_the_run_error() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let (run, roots) = start(
        fake(Arc::clone(&calls)),
        location("/tmp/navo-run/bad"),
        query(&[]),
    );
    assert_eq!(roots, vec![location("/tmp/navo-run/bad")]);
    let events = events(&run);
    assert!(
        matches!(&events[..], [RunEvent::Done(Err(error))] if error.contains("broken")),
        "{events:?}"
    );
}

#[test]
fn a_single_unresolvable_root_is_the_run_error() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let (run, roots) = start(
        fake(Arc::clone(&calls)),
        location("/tmp/navo-run"),
        query(&["../../../../up"]),
    );
    assert!(roots.is_empty());
    let events = events(&run);
    assert!(
        matches!(&events[..], [RunEvent::Done(Err(_))]),
        "{events:?}"
    );
    assert!(
        calls
            .lock()
            .expect("the call log is not poisoned")
            .is_empty()
    );
}

#[test]
fn dropping_the_run_cancels_the_walk() {
    let (exit_send, exit) = mpsc::channel();
    let walk: ContentWalk = Arc::new(move |_, _, cancel, sink| {
        while sink(batch("again.txt")) == ControlFlow::Continue(()) {}
        let _ = exit_send.send(cancel.is_cancelled());
        Err(FindError::Cancelled)
    });
    let (run, _) = start(walk, location("/tmp/navo-run"), query(&[]));
    assert!(matches!(
        run.receive.recv_timeout(WAIT),
        Ok(RunEvent::Batch { root: 0, .. })
    ));
    drop(run);
    assert_eq!(exit.recv_timeout(WAIT), Ok(true));
}
