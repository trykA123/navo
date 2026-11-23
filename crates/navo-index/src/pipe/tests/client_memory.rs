//! Behaviour tests for the query client's per-frame reply reader, including memory answers.

use super::super::{
    Hit, MemoryReport, Part, PipeError, Reply, ReplyState, Response, Status, VolumeStatus, accept,
};

/// A current-generation MemoryV1 frame completes the reply at once with its report.
#[test]
fn a_current_generation_memory_frame_yields_the_report() {
    let mut report = MemoryReport::default();
    report.add(Part::Process, 4096, 0);
    let mut state = ReplyState::default();
    let reply = accept(
        Response::MemoryV1 {
            generation: 7,
            report: report.clone(),
        },
        7,
        &mut state,
    )
    .unwrap()
    .expect("memory answer must complete the reply");
    assert_eq!(reply.memory, Some(report));
    assert!(reply.hits.is_empty());
    assert_eq!(reply.status, Status::default());
}

/// An older-generation MemoryV1 frame is skipped and the hits/status pair still completes.
#[test]
fn an_old_generation_memory_frame_is_skipped() {
    let mut state = ReplyState::default();
    let skipped = accept(
        Response::MemoryV1 {
            generation: 6,
            report: MemoryReport::default(),
        },
        7,
        &mut state,
    )
    .unwrap();
    assert!(skipped.is_none());
    let hits = accept(
        Response::Hits {
            generation: 7,
            hits: Vec::new(),
        },
        7,
        &mut state,
    )
    .unwrap();
    assert!(hits.is_none());
    let reply = accept(
        Response::Status {
            generation: 7,
            status: Status::default(),
        },
        7,
        &mut state,
    )
    .unwrap()
    .expect("status completes the hits/status pair");
    assert!(reply.memory.is_none());
}

/// Hits then Status still form the same reply as before the memory arm.
#[test]
fn hits_then_status_form_the_same_reply_as_before() {
    let hits = vec![Hit {
        path: r"C:\work\a.rs".into(),
        flags: 1,
        score: 9,
        indices: vec![0],
    }];
    let status = Status {
        volumes: vec![VolumeStatus {
            root: r"C:\".into(),
            entries: 3,
            ..VolumeStatus::default()
        }],
    };
    let mut state = ReplyState::default();
    let first = accept(
        Response::Hits {
            generation: 7,
            hits: hits.clone(),
        },
        7,
        &mut state,
    )
    .unwrap();
    assert!(first.is_none());
    let reply = accept(
        Response::Status {
            generation: 7,
            status: status.clone(),
        },
        7,
        &mut state,
    )
    .unwrap()
    .expect("status completes the reply");
    assert_eq!(
        reply,
        Reply {
            hits,
            status,
            ..Reply::default()
        }
    );
}

/// A frame this reader does not accept still ends in InvalidResponse.
#[test]
fn an_unknown_frame_is_still_refused() {
    let mut state = ReplyState::default();
    let result = accept(
        Response::VersionV1 {
            generation: 7,
            revision: 1,
            build: "test".into(),
        },
        7,
        &mut state,
    );
    assert!(matches!(result, Err(PipeError::InvalidResponse)));
}
