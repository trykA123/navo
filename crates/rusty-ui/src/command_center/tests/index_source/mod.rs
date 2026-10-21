mod pipe;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use rusty_index::pipe::{Hit as IndexHit, PipeError, Reply, Status, VolumeStatus};
use rusty_index::table::Flags;

use crate::command_center::{
    Cancel, Group, Hit, HitSink, IndexHooks, IndexMode, IndexOptions, IndexSource, Indexing, Query,
    RecentSource, Request, ResultSource, Results, Row, Scope, Target,
};
use crate::window_view::Frecent;

fn request(generation: u64) -> Request {
    Request {
        query: Query::parse("budget"),
        generation,
        here: None,
        limit: 500,
    }
}

fn reply() -> Reply {
    Reply {
        hits: vec![IndexHit {
            path: r"C:\Docs\budget.xlsx".into(),
            flags: Flags::NONE.bits(),
            score: 80,
            indices: vec![0, 1, 2, 3, 4, 5],
        }],
        status: Status::default(),
    }
}

fn hooks() -> IndexHooks {
    IndexHooks {
        host: Arc::new(|_, _, _| reply()),
        ..IndexHooks::none()
    }
}

fn source(mode: IndexMode, hooks: IndexHooks) -> IndexSource {
    IndexSource::new(
        IndexOptions {
            mode,
            ..IndexOptions::default()
        },
        hooks,
        Arc::new(|_| Vec::new()),
    )
}

fn run(source: &IndexSource) -> Vec<Hit> {
    let request = request(1);
    let sink = HitSink::new(1);
    source.search(&request, &sink, &Cancel::new());
    sink.drain(1)
}

#[test]
fn auto_falls_back_when_the_pipe_is_absent_or_refuses() {
    for error in [PipeError::NotAvailable, PipeError::InvalidResponse] {
        let error = std::sync::Mutex::new(Some(error));
        let mut hooks = hooks();
        hooks.pipe =
            crate::command_center::index_source::PipeQueries::new(Arc::new(move |_, _, _| {
                Err(error.lock().unwrap().take().unwrap())
            }));
        let hits = run(&source(IndexMode::Auto, hooks));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].source, "index");
    }
}

#[test]
fn service_without_a_pipe_returns_nothing_and_never_calls_the_host() {
    let mut hooks = hooks();
    hooks.host = Arc::new(|_, _, _| panic!("service must not use the host"));
    assert!(run(&source(IndexMode::Service, hooks)).is_empty());
}

#[test]
fn off_does_not_query_either_backend() {
    let mut hooks = hooks();
    hooks.host = Arc::new(|_, _, _| panic!("off must not use the host"));
    hooks.pipe = crate::command_center::index_source::PipeQueries::new(Arc::new(|_, _, _| {
        panic!("off must not use the pipe")
    }));
    assert!(run(&source(IndexMode::Off, hooks)).is_empty());
}

#[test]
fn no_rows_exist_before_the_first_complete_snapshot() {
    let mut hooks = hooks();
    hooks.host = Arc::new(|_, _, _| Reply::default());
    assert!(run(&source(IndexMode::Walk, hooks)).is_empty());
}

#[test]
fn recent_wins_deduplication_and_keeps_the_higher_score() {
    let load = Arc::new(|_| {
        vec![Frecent {
            location: r"C:\Docs\budget.xlsx".into(),
            is_folder: false,
            weight: 100.0,
        }]
    });
    let source = IndexSource::new(IndexOptions::default(), hooks(), load.clone());
    let mut hits = run(&source);
    let score = hits[0].score;
    hits.extend(RecentSource::new(load).matches(&request(1).query));
    let rows = Results::merge(hits, Scope::Files);
    let kept: Vec<_> = rows
        .iter()
        .filter_map(|row| match row {
            Row::Hit(hit) => Some(hit),
            _ => None,
        })
        .collect();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].source, "recent");
    assert!(kept[0].score >= score);
}

#[test]
fn indexing_note_uses_the_count_without_a_percentage() {
    let mut hooks = hooks();
    hooks.host = Arc::new(|_, _, _| Reply {
        status: Status {
            volumes: vec![VolumeStatus {
                root: "C:".into(),
                entries: 12_345,
                progress: Some(0),
                errors: Vec::new(),
            }],
        },
        ..Reply::default()
    });
    let source = source(IndexMode::Walk, hooks);
    run(&source);
    let note = source.indexing(&request(1)).unwrap();
    assert_eq!(
        note,
        Indexing {
            root: "C:".into(),
            count: 12_345
        }
    );
    assert!(note.text().contains("12,345"));
    assert!(!note.text().contains('%'));
}

#[test]
fn shown_index_rows_load_metadata_once_after_search() {
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let mut hooks = hooks();
    hooks.metadata =
        crate::command_center::index_source::IndexMetadata::new(Arc::new(move |location| {
            count.fetch_add(1, Ordering::SeqCst);
            Ok(rusty_fs::FsEntry {
                name: location.name().unwrap(),
                kind: rusty_fs::EntryKind::File,
                size: 42,
                modified_ms: Some(123),
                attributes: rusty_fs::Attributes::default(),
            })
        }));
    let source = source(IndexMode::Walk, hooks);
    let hits = run(&source);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    source.shown(&hits, &Cancel::new());
    source.shown(&hits, &Cancel::new());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let Target::Open { location, .. } = &hits[0].target else {
        panic!("open target")
    };
    let metadata = source.metadata(location).unwrap();
    assert_eq!((metadata.size, metadata.modified_ms), (42, Some(123)));
    assert_eq!(hits[0].group, Group::Files);
}
