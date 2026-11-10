//! Tests for the image size and encoded caps: huge headers, WebP and large-side hashing.

use navo_ui::compare_diff::{DiffUpdate, ImageLimit, ImageScores, PairKind};

use crate::compare::diff::image::{oversize_decodes, reset_oversize_decodes};
use crate::compare::diff::pair::PairDiff;

use super::fixtures::{gif_huge_screen, jpeg_huge_header, png, tiff_with_size, webp_with_size};
use super::support::{
    PatternSource, ScriptSource, WAIT, channel, image, named_input, opened, pattern_input, request,
    sized_input,
};

#[test]
fn huge_headers_are_too_large_without_a_decoder() {
    reset_oversize_decodes();
    let cases = [
        ("jpg", jpeg_huge_header(10_000, 10_000)),
        ("gif", gif_huge_screen()),
        ("tiff", tiff_with_size(10_000, 10_000)),
        ("webp", webp_with_size(16_000, 16_000)),
    ];
    for (index, (extension, bytes)) in cases.into_iter().enumerate() {
        let id = 30 + u64::try_from(index).unwrap();
        let left = ScriptSource::new(bytes.clone(), usize::MAX, 0);
        let right = ScriptSource::new(bytes, usize::MAX, 0);
        let left_name = format!("left.{extension}");
        let right_name = format!("right.{extension}");
        let (sink, receiver) = channel();
        let control = PairDiff::start(
            request(id),
            named_input(&left, &right, &left_name, &right_name),
            sink,
        );
        assert_eq!(opened(&receiver), PairKind::Image);
        let (scores, mask) = image(&receiver, id);
        assert_eq!(
            scores,
            ImageScores::TooLarge {
                reason: ImageLimit::Pixels,
                equal: true
            },
            "{extension} must be sized without a decoder"
        );
        assert!(mask.is_none());
        drop(control);
    }
    assert_eq!(oversize_decodes(), 0, "no decoder for an oversized header");
}

#[test]
fn a_webp_over_48_megapixels_is_too_large() {
    let bytes = webp_with_size(6_929, 6_929);
    let left = ScriptSource::new(bytes.clone(), usize::MAX, 0);
    let right = ScriptSource::new(bytes, usize::MAX, 0);
    let (sink, receiver) = channel();
    let control = PairDiff::start(
        request(35),
        named_input(&left, &right, "left.webp", "right.webp"),
        sink,
    );
    assert_eq!(opened(&receiver), PairKind::Image);
    let (scores, mask) = image(&receiver, 35);
    assert_eq!(
        scores,
        ImageScores::TooLarge {
            reason: ImageLimit::Pixels,
            equal: true
        }
    );
    assert!(mask.is_none());
    drop(control);
}

#[test]
fn a_webp_named_png_gets_the_webp_cap() {
    let bytes = webp_with_size(7_072, 7_072);
    let left = ScriptSource::new(bytes.clone(), usize::MAX, 0);
    let right = ScriptSource::new(bytes, usize::MAX, 0);
    let (sink, receiver) = channel();
    let control = PairDiff::start(
        request(39),
        named_input(&left, &right, "photo.png", "copy.png"),
        sink,
    );
    assert_eq!(opened(&receiver), PairKind::Image);
    let update = receiver.recv_timeout(WAIT).unwrap();
    assert!(
        matches!(
            update,
            DiffUpdate::Image {
                request: 39,
                scores: ImageScores::TooLarge {
                    reason: ImageLimit::Pixels,
                    equal: true
                },
                mask: None
            }
        ),
        "the bytes say WebP, so the 48 MP cap applies: {update:?}"
    );
    drop(control);
}

#[test]
fn a_webp_between_16_and_48_megapixels_reaches_the_decoder() {
    let bytes = webp_with_size(4_000, 4_001);
    let left = ScriptSource::new(bytes.clone(), usize::MAX, 0);
    let right = ScriptSource::new(bytes, usize::MAX, 0);
    let (sink, receiver) = channel();
    let control = PairDiff::start(
        request(38),
        named_input(&left, &right, "left.webp", "right.webp"),
        sink,
    );
    assert_eq!(opened(&receiver), PairKind::Image);
    let update = receiver.recv_timeout(WAIT).unwrap();
    assert!(
        matches!(update, DiffUpdate::Failed { request: 38, .. }),
        "a header-only 16 MP WebP is decoded and fails, not capped: {update:?}"
    );
    drop(control);
}

#[test]
fn sides_over_64_mib_are_still_hashed() {
    let size = 64 * 1024 * 1024 + 1;
    let same_left = PatternSource::new(size, 7);
    let same_right = PatternSource::new(size, 7);
    let (sink, receiver) = channel();
    let control = PairDiff::start(request(36), pattern_input(&same_left, &same_right), sink);
    assert_eq!(opened(&receiver), PairKind::Image);
    let (scores, _) = image(&receiver, 36);
    assert_eq!(
        scores,
        ImageScores::TooLarge {
            reason: ImageLimit::Encoded,
            equal: true
        }
    );
    drop(control);

    let different_left = PatternSource::new(size, 7);
    let different_right = PatternSource::new(size, 8);
    let (sink, receiver) = channel();
    let control = PairDiff::start(
        request(37),
        pattern_input(&different_left, &different_right),
        sink,
    );
    assert_eq!(opened(&receiver), PairKind::Image);
    let (scores, _) = image(&receiver, 37);
    assert_eq!(
        scores,
        ImageScores::TooLarge {
            reason: ImageLimit::Encoded,
            equal: false
        }
    );
    drop(control);
}

#[test]
fn a_listed_size_over_64_mib_is_decided_by_the_bytes_read() {
    let bytes = png(8, 8, None);
    let left = ScriptSource::new(bytes.clone(), usize::MAX, 0);
    let right = ScriptSource::new(bytes, usize::MAX, 0);
    let (sink, receiver) = channel();
    let control = PairDiff::start(
        request(13),
        sized_input(&left, &right, 65 * 1024 * 1024),
        sink,
    );
    assert_eq!(opened(&receiver), PairKind::Image);
    let (scores, mask) = image(&receiver, 13);
    assert!(
        matches!(scores, ImageScores::Scored { ssim, .. } if ssim > 0.999),
        "a file that shrank since listing diffs normally: {scores:?}"
    );
    assert!(mask.is_some());
    drop(control);
}
