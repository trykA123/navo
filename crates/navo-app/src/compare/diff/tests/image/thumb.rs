//! Tests for image card thumbnails: scores, size notes, caps, bounds and BGRA pixels.

use std::fs;

use navo_compare::model::CancelToken;
use navo_ui::compare_diff::{CardDetail, ImageCard, THUMB_SIDE};

use super::fixtures::png;
use super::support::{PatternSource, local_input, pattern_input};
use crate::compare::diff::image::thumb::card;

/// The runner's image card, or a panic when it returned none or a failure.
fn image_card(detail: Option<CardDetail>) -> ImageCard {
    match detail.expect("an image detail") {
        CardDetail::Image(card) => card,
        other => panic!("unexpected detail {other:?}"),
    }
}

#[test]
fn a_scored_pair_gets_both_thumbnails_and_the_similarity() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("left.png"), png(8, 8, None)).unwrap();
    fs::write(dir.path().join("right.png"), png(8, 8, Some((3, 4)))).unwrap();
    let input = local_input(dir.path(), "left.png", "right.png");
    let card = image_card(card(&input, &CancelToken::new()));
    assert!(card.similarity.is_some());
    assert!(card.note.is_none());
    let left = card.left.expect("a left thumbnail");
    let right = card.right.expect("a right thumbnail");
    assert_eq!(
        (left.width, left.height),
        (8, 8),
        "a small image is not scaled up"
    );
    assert_eq!((right.width, right.height), (8, 8));
}

#[test]
fn different_sizes_still_get_thumbnails_and_the_sizes_note() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("left.png"), png(8, 8, None)).unwrap();
    fs::write(dir.path().join("right.png"), png(10, 6, None)).unwrap();
    let input = local_input(dir.path(), "left.png", "right.png");
    let card = image_card(card(&input, &CancelToken::new()));
    assert!(card.similarity.is_none());
    assert_eq!(card.note, Some("compare-image-note-sizes"));
    let left = card.left.expect("a left thumbnail");
    let right = card.right.expect("a right thumbnail");
    assert!(left.width.max(left.height) <= THUMB_SIDE);
    assert!(right.width.max(right.height) <= THUMB_SIDE);
}

#[test]
fn an_encoded_over_cap_pair_gets_no_thumbnails() {
    let size = 64 * 1024 * 1024 + 1;
    let left = PatternSource::new(size, 7);
    let right = PatternSource::new(size, 7);
    let input = pattern_input(&left, &right);
    let card = image_card(card(&input, &CancelToken::new()));
    assert!(card.similarity.is_none());
    assert_eq!(card.note, Some("compare-image-note-encoded"));
    assert!(card.left.is_none());
    assert!(card.right.is_none());
}

#[test]
fn a_thumbnail_fits_240_pixels_and_keeps_the_aspect() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("left.png"), png(400, 200, None)).unwrap();
    fs::write(dir.path().join("right.png"), png(400, 200, None)).unwrap();
    let input = local_input(dir.path(), "left.png", "right.png");
    let card = image_card(card(&input, &CancelToken::new()));
    let left = card.left.expect("a left thumbnail");
    assert_eq!((left.width, left.height), (240, 120));
}

#[test]
fn thumbnail_pixels_are_bgra() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("left.png"), png(8, 8, None)).unwrap();
    fs::write(dir.path().join("right.png"), png(8, 8, Some((3, 4)))).unwrap();
    let input = local_input(dir.path(), "left.png", "right.png");
    let card = image_card(card(&input, &CancelToken::new()));
    let left = card.left.expect("a left thumbnail");
    assert_eq!(&left.bgra[..4], &[30, 20, 10, 255]);
}
