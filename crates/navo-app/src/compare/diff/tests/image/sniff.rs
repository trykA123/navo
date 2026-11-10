//! Format sniffing tests: the bytes pick the decoder and its cap, the name is only a fallback,
//! and neither naming a raster format gives Unsupported.

use navo_ui::compare_diff::{DiffUpdate, ImageLimit, ImageScores, PairKind};

use crate::compare::diff::pair::PairDiff;

use super::fixtures::{png, webp_with_size};
use super::support::{ScriptSource, WAIT, channel, named_input, opened, request};

/// Run one pair of in-memory sides under these names and return the update after `Opened`.
fn run(
    id: u64,
    (left, left_name): (Vec<u8>, &str),
    (right, right_name): (Vec<u8>, &str),
) -> DiffUpdate {
    let left = ScriptSource::new(left, usize::MAX, 0);
    let right = ScriptSource::new(right, usize::MAX, 0);
    let (sink, receiver) = channel();
    let control = PairDiff::start(
        request(id),
        named_input(&left, &right, left_name, right_name),
        sink,
    );
    assert_eq!(opened(&receiver), PairKind::Image);
    let update = receiver.recv_timeout(WAIT).unwrap();
    drop(control);
    update
}

#[test]
fn webp_bytes_named_svg_get_the_webp_cap() {
    let bytes = webp_with_size(7_072, 7_072);
    let update = run(70, (bytes.clone(), "photo.svg"), (bytes, "copy.svg"));
    assert!(
        matches!(
            update,
            DiffUpdate::Image {
                scores: ImageScores::TooLarge {
                    reason: ImageLimit::Pixels,
                    equal: true
                },
                ..
            }
        ),
        "the bytes say WebP, whatever the name: {update:?}"
    );
}

#[test]
fn png_bytes_named_dat_are_scored() {
    let bytes = png(8, 8, None);
    let update = run(71, (bytes.clone(), "left.png"), (bytes, "x.dat"));
    assert!(
        matches!(
            update,
            DiffUpdate::Image {
                scores: ImageScores::Scored { ssim, .. },
                ..
            } if ssim > 0.999
        ),
        "a PNG with an unknown extension diffs as a PNG: {update:?}"
    );
}

#[test]
fn a_truncated_webp_with_a_known_extension_fails_cleanly() {
    let mut bytes = webp_with_size(64, 64);
    bytes.truncate(20);
    let update = run(72, (bytes.clone(), "left.webp"), (bytes, "right.webp"));
    assert!(
        matches!(update, DiffUpdate::Failed { request: 72, .. }),
        "a short header is a decode failure, not a panic: {update:?}"
    );
}

#[test]
fn unknown_bytes_with_an_unknown_extension_are_unsupported() {
    let update = run(
        73,
        (png(8, 8, None), "left.png"),
        (b"not an image at all".to_vec(), "x.dat"),
    );
    assert!(
        matches!(
            update,
            DiffUpdate::Image {
                scores: ImageScores::Unsupported { .. },
                ..
            }
        ),
        "neither the bytes nor the name name a raster format: {update:?}"
    );
}
