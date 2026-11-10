//! Cancellation tests for the image pair worker: gated reads, no right-side open, join.

use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use navo_ui::compare_diff::PairKind;

use super::fixtures::png;
use super::support::{CHUNK, ScriptSource, blocked_input, channel, named_input, opened, request};
use crate::compare::diff::image::probe;
use crate::compare::diff::pair::PairDiff;

const PAD: usize = 2 * CHUNK;

/// A PNG padded past one read chunk, so the full read needs a second chunk.
fn padded_png() -> Vec<u8> {
    let mut bytes = png(8, 8, None);
    bytes.resize(bytes.len() + PAD, 0);
    bytes
}

#[test]
fn cancel_between_chunks_never_opens_the_other_side_and_joins() {
    let bytes = padded_png();
    let left = ScriptSource::new(bytes.clone(), 2, CHUNK);
    let right = ScriptSource::new(bytes, usize::MAX, 0);
    let (sink, receiver) = channel();
    let control = PairDiff::start(request(10), blocked_input(&left, &right), sink);
    assert_eq!(opened(&receiver), PairKind::Image);
    left.wait_blocked();
    let release = Arc::clone(&left);
    let (release_tx, release_rx) = mpsc::channel();
    let helper = thread::spawn(move || {
        let _ = release_rx.recv();
        release.release();
    });
    control.close_for_test();
    let _ = release_tx.send(());
    assert!(control.wait_for_exit(), "worker did not finish in 5 s");
    helper.join().unwrap();
    assert_eq!(
        right.opens(),
        1,
        "the cancelled snapshot never opened the right side"
    );
    drop(control);
}

#[test]
fn cancel_during_the_left_hash_never_opens_the_right_side() {
    let bytes = png(8, 8, None);
    let left = ScriptSource::new(bytes.clone(), 1, 0);
    let right = ScriptSource::new(bytes, usize::MAX, 0);
    let (sink, _receiver) = channel();
    let control = PairDiff::start(request(25), blocked_input(&left, &right), sink);
    left.wait_blocked();
    let release = Arc::clone(&left);
    let (release_tx, release_rx) = mpsc::channel();
    let helper = thread::spawn(move || {
        let _ = release_rx.recv();
        release.release();
    });
    control.close_for_test();
    let _ = release_tx.send(());
    assert!(control.wait_for_exit(), "worker did not finish in 5 s");
    helper.join().unwrap();
    assert_eq!(
        right.opens(),
        0,
        "the cancelled hash never opens the right side"
    );
    drop(control);
}

#[test]
fn dropping_the_pair_returns_before_a_blocked_image_read_finishes() {
    let bytes = padded_png();
    let left = ScriptSource::new(bytes.clone(), 2, CHUNK);
    let right = ScriptSource::new(bytes, usize::MAX, 0);
    let (sink, receiver) = channel();
    let control = PairDiff::start(request(11), blocked_input(&left, &right), sink);
    assert_eq!(opened(&receiver), PairKind::Image);
    left.wait_blocked();
    let started = Instant::now();
    drop(control);
    assert!(started.elapsed() < Duration::from_millis(500));
    left.release();
}

#[test]
fn cancel_during_the_left_decode_never_decodes_the_right_side() {
    let name = "cancel-in-left-decode.png";
    probe::cancel_after_left_decode(name);
    let bytes = png(8, 8, None);
    let left = ScriptSource::new(bytes.clone(), usize::MAX, 0);
    let right = ScriptSource::new(bytes, usize::MAX, 0);
    let (sink, receiver) = channel();
    let control = PairDiff::start(
        request(26),
        named_input(&left, &right, name, "right.png"),
        sink,
    );
    assert_eq!(opened(&receiver), PairKind::Image);
    assert!(control.wait_for_exit(), "worker did not finish in 5 s");
    assert!(
        !probe::right_decoded(name),
        "a cancel during the left decode stops before the right decode"
    );
    assert!(
        receiver.try_recv().is_err(),
        "a cancelled pair sends nothing"
    );
}
