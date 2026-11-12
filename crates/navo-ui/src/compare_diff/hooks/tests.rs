//! Tests for the diff contract: types build, controls drop without blocking, a fake pushes updates.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, PoisonError};

use super::*;
use crate::compare_diff::CompareOptions;
use crate::compare_diff::fake::FakeDiffHooks;
use crate::compare_diff::image_body::{notice_of, similarity_of};
use crate::settings::Values;

fn request(id: u64) -> PairRequest {
    PairRequest {
        id,
        path: "a/b.txt".into(),
        left: None,
        right: None,
    }
}

fn facts() -> PairFacts {
    PairFacts {
        left_encoding: "enc-utf8",
        right_encoding: "enc-utf8",
        left_endings: "eol-lf",
        right_endings: "eol-lf",
        raw_equal: false,
        display_equal: false,
        fallbacks: vec![],
    }
}

fn hunk() -> HunkView {
    HunkView {
        rows: 0..1,
        left_bytes: 0..3,
        right_bytes: 0..4,
        digest: Some([7; 16]),
        added: 1,
        removed: 1,
    }
}

#[test]
fn types_build_and_compare() {
    let key = ViewedKey([1; 64]);
    let text = PairText {
        left_starts: Vec::new(),
        right_starts: Vec::new(),
        left: vec!["a".into()],
        right: vec!["b".into()],
        rows: vec![
            DiffRow::Changed(0, 0),
            DiffRow::Deleted(1),
            DiffRow::Inserted(1),
        ],
        hunks: vec![hunk()],
        facts: facts(),
        viewed: key,
    };
    assert_eq!(text.clone(), text);
    let card = Card {
        kind: PairKind::Large { left: 1, right: 2 },
        rows: vec![
            CardRow::Line {
                row: DiffRow::Equal(0, 0),
                hunk: 0,
                left: Some("x".into()),
                right: None,
            },
            CardRow::Gap { lines: 4 },
            CardRow::More { hunks: 2 },
        ],
        hunks: vec![hunk()],
        added: 1,
        removed: 1,
        viewed_key: key,
        viewed: false,
        viewable: true,
    };
    assert_eq!(card.clone(), card);
    assert_ne!(Layout::Unified, Layout::SideBySide);
    assert_ne!(
        PairKind::OneSided { left: true },
        PairKind::Unavailable("x")
    );
    let spans = RowSpans {
        row: 0,
        left: vec![0..1, 2..3],
        right: vec![],
    };
    assert_eq!(spans.clone(), spans);
}

#[test]
fn dropping_a_control_does_not_block() {
    let hooks = FakeDiffHooks::default();
    let sink: DiffSink = Arc::new(|_| {});
    let control = hooks.open_pair(
        request(1),
        &CompareOptions::from_values(&Values::default()),
        sink,
    );
    control.spans(0..10);
    drop(control);
    assert!(hooks.calls.dropped.load(Ordering::SeqCst));
    let stream = hooks.open_stream(
        &CompareOptions::from_values(&Values::default()),
        Arc::new(|_| {}),
    );
    stream.set_items(1, vec![(request(1), true), (request(2), false)]);
    stream.search(3, "x".into());
    drop(stream);
    let calls = &hooks.calls;
    assert_eq!(
        *calls.items.lock().unwrap_or_else(PoisonError::into_inner),
        [(1, 2)]
    );
    assert_eq!(
        calls
            .searches
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len(),
        1
    );
    assert_eq!(
        calls
            .spans
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len(),
        1
    );
}

#[test]
fn an_update_flows_through_the_fake() {
    let hooks = FakeDiffHooks::default();
    let seen: Arc<Mutex<Vec<DiffUpdate>>> = Arc::default();
    let store = seen.clone();
    let sink: DiffSink = Arc::new(move |update| {
        store
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(update);
    });
    let _control = hooks.open_pair(
        request(9),
        &CompareOptions::from_values(&Values::default()),
        sink,
    );
    let push = hooks
        .calls
        .pair_sink
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
        .expect("sink stored");
    push(DiffUpdate::Opened {
        request: 9,
        kind: PairKind::Binary,
        text: None,
    });
    let got = seen.lock().unwrap_or_else(PoisonError::into_inner);
    assert_eq!(
        *got,
        [DiffUpdate::Opened {
            request: 9,
            kind: PairKind::Binary,
            text: None
        }]
    );
    assert_eq!(
        hooks
            .calls
            .requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)[0]
            .id,
        9
    );
}

#[test]
fn similarity_of_rounds_the_structural_score() {
    let scored = ImageScores::Scored {
        ssim: 0.987,
        rgb_min_rms_similarity: 0.9,
        luma_hellinger_similarity: 0.9,
        alpha: None,
    };
    assert_eq!(similarity_of(&scored), Some(99));
    assert_eq!(similarity_of(&ImageScores::HashOnly { equal: true }), None);
}

#[test]
fn notice_of_names_each_fallback() {
    assert_eq!(
        notice_of(&ImageScores::HashOnly { equal: false }),
        Some("compare-image-note-hash")
    );
    assert_eq!(
        notice_of(&ImageScores::SizeMismatch {
            left: ImageSize {
                width: 2,
                height: 1,
            },
            right: ImageSize {
                width: 1,
                height: 1,
            },
        }),
        Some("compare-image-note-sizes")
    );
    assert_eq!(
        notice_of(&ImageScores::Unsupported { format: "svg" }),
        Some("compare-image-note-format")
    );
    for (reason, id) in [
        (ImageLimit::Encoded, "compare-image-note-encoded"),
        (ImageLimit::Pixels, "compare-image-note-pixels"),
        (ImageLimit::Decode, "compare-image-note-decode"),
    ] {
        assert_eq!(
            notice_of(&ImageScores::TooLarge {
                reason,
                equal: false,
            }),
            Some(id)
        );
    }
    let scored = ImageScores::Scored {
        ssim: 1.0,
        rgb_min_rms_similarity: 1.0,
        luma_hellinger_similarity: 1.0,
        alpha: None,
    };
    assert_eq!(notice_of(&scored), None);
}

#[test]
fn a_detail_update_compares_by_value() {
    let card = LargeCard {
        hunks: vec![LargeHunk {
            left_rows: 0..2,
            right_rows: 0..2,
            left_bytes: 0..8,
            right_bytes: 0..9,
        }],
        found: 1,
        progress: None,
        scan: CardScan::Full,
    };
    let update = StreamUpdate::Detail {
        generation: 3,
        id: 7,
        detail: Arc::new(CardDetail::Large(card.clone())),
    };
    assert_eq!(
        update,
        StreamUpdate::Detail {
            generation: 3,
            id: 7,
            detail: Arc::new(CardDetail::Large(card)),
        }
    );
    assert_ne!(
        update,
        StreamUpdate::Detail {
            generation: 3,
            id: 8,
            detail: Arc::new(CardDetail::Failed("gone".into())),
        }
    );
}
