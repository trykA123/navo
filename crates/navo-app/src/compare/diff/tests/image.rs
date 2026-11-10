//! Tests for image pair workers: scores, fallbacks, limits and cancellation.

mod fixtures;
mod support;

use std::fs;

use navo_ui::compare_diff::{DiffUpdate, ImageAlpha, ImageLimit, ImageScores, ImageSize, PairKind};

use crate::compare::diff::image::too_large;
use crate::compare::diff::pair::PairDiff;
use fixtures::{
    bmp_header, bmp_short_data, gif_huge_screen, jpeg_with_metadata, png, png_truncated,
    png_with_header_size, rgba_png, tiff_ifd_at_end, webp_huge_header, webp_lossless,
};
use support::{
    ScriptSource, blocked_input, channel, image, local_input, named_input, opened, request,
};

mod cancel;
mod ico;
mod limits;
mod sniff;

#[test]
fn scores_and_mask_come_after_opened() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("left.png"), png(8, 8, None)).unwrap();
    fs::write(dir.path().join("right.png"), png(8, 8, Some((3, 4)))).unwrap();
    let (sink, receiver) = channel();
    let control = PairDiff::start(
        request(7),
        local_input(dir.path(), "left.png", "right.png"),
        sink,
    );
    assert_eq!(opened(&receiver), PairKind::Image);
    let (scores, mask) = image(&receiver, 7);
    let ImageScores::Scored {
        ssim,
        rgb_min_rms_similarity,
        ..
    } = scores
    else {
        panic!("same-size images are scored");
    };
    assert!(ssim < 1.0, "one red pixel lowers ssim");
    assert!(
        rgb_min_rms_similarity < 1.0,
        "one red pixel lowers the RGB score"
    );
    let mask = mask.expect("a scored pair carries a mask");
    assert_eq!((mask.width, mask.height), (8, 8));
    assert!(
        mask.pixels[4 * 8 + 3] > 200,
        "the changed pixel is strongly marked"
    );
    assert_eq!(mask.pixels[0], 0, "an equal pixel is not marked");
    drop(control);
}

#[test]
fn an_alpha_only_difference_is_scored_and_masked() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("left.png"), rgba_png(8, 8, 255)).unwrap();
    fs::write(dir.path().join("right.png"), rgba_png(8, 8, 0)).unwrap();
    let (sink, receiver) = channel();
    let control = PairDiff::start(
        request(12),
        local_input(dir.path(), "left.png", "right.png"),
        sink,
    );
    assert_eq!(opened(&receiver), PairKind::Image);
    let (scores, mask) = image(&receiver, 12);
    let ImageScores::Scored { ssim, alpha, .. } = scores else {
        panic!("an alpha-only change is scored");
    };
    assert!((ssim - 1.0).abs() < 1e-12, "RGB is equal");
    let alpha = alpha.expect("both images carry alpha");
    assert!(alpha.differs, "the alpha channels differ");
    assert_eq!(alpha.max_delta, 255);
    assert_eq!(
        alpha,
        ImageAlpha {
            max_delta: 255,
            mean_delta: 255.0,
            differs: true,
        }
    );
    let mask = mask.expect("an alpha change carries a mask");
    assert!(mask.pixels.iter().any(|&pixel| pixel > 0));
    drop(control);
}

#[test]
fn different_sizes_report_both_dimensions_without_a_mask() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("left.png"), png(8, 8, None)).unwrap();
    fs::write(dir.path().join("right.png"), png(9, 8, None)).unwrap();
    let (sink, receiver) = channel();
    let control = PairDiff::start(
        request(8),
        local_input(dir.path(), "left.png", "right.png"),
        sink,
    );
    assert_eq!(opened(&receiver), PairKind::Image);
    let (scores, mask) = image(&receiver, 8);
    assert_eq!(
        scores,
        ImageScores::SizeMismatch {
            left: ImageSize {
                width: 8,
                height: 8
            },
            right: ImageSize {
                width: 9,
                height: 8
            },
        }
    );
    assert!(mask.is_none(), "a size mismatch has no mask");
    drop(control);
}

#[test]
fn images_over_64_megapixels_report_too_large_with_the_hash() {
    let dir = tempfile::tempdir().unwrap();
    let header = bmp_header(8_001, 8_000);
    let mut other = header.clone();
    if let Some(last) = other.last_mut() {
        *last = 1;
    }
    fs::write(dir.path().join("left.bmp"), &header).unwrap();
    fs::write(dir.path().join("right.bmp"), &other).unwrap();
    let (sink, receiver) = channel();
    let control = PairDiff::start(
        request(9),
        local_input(dir.path(), "left.bmp", "right.bmp"),
        sink,
    );
    assert_eq!(opened(&receiver), PairKind::Image);
    let (scores, mask) = image(&receiver, 9);
    assert_eq!(
        scores,
        ImageScores::TooLarge {
            reason: ImageLimit::Pixels,
            equal: false
        }
    );
    assert!(mask.is_none(), "a too-large result has no mask");
    drop(control);
}

#[test]
fn an_svg_pair_is_unsupported_not_failed() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("left.svg"),
        b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1\" height=\"1\" />",
    )
    .unwrap();
    fs::write(
        dir.path().join("right.svg"),
        b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"2\" height=\"1\" />",
    )
    .unwrap();
    let (sink, receiver) = channel();
    let control = PairDiff::start(
        request(14),
        local_input(dir.path(), "left.svg", "right.svg"),
        sink,
    );
    assert_eq!(opened(&receiver), PairKind::Image);
    let (scores, mask) = image(&receiver, 14);
    assert_eq!(scores, ImageScores::Unsupported { format: "SVG" });
    assert!(mask.is_none());
    drop(control);
}

#[test]
fn a_malformed_header_is_failed() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("left.png"), png_truncated()).unwrap();
    fs::write(dir.path().join("right.png"), png_truncated()).unwrap();
    let (sink, receiver) = channel();
    let control = PairDiff::start(
        request(15),
        local_input(dir.path(), "left.png", "right.png"),
        sink,
    );
    assert_eq!(opened(&receiver), PairKind::Image);
    match receiver.recv_timeout(support::WAIT).unwrap() {
        DiffUpdate::Failed { request, .. } => assert_eq!(request, 15),
        other => panic!("expected Failed, got {other:?}"),
    }
    drop(control);
}

#[test]
fn a_png_header_claiming_huge_dimensions_is_too_large() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = png_with_header_size(100_000, 100_000);
    fs::write(dir.path().join("left.png"), &bytes).unwrap();
    fs::write(dir.path().join("right.png"), &bytes).unwrap();
    let (sink, receiver) = channel();
    let control = PairDiff::start(
        request(16),
        local_input(dir.path(), "left.png", "right.png"),
        sink,
    );
    assert_eq!(opened(&receiver), PairKind::Image);
    let (scores, mask) = image(&receiver, 16);
    assert_eq!(
        scores,
        ImageScores::TooLarge {
            reason: ImageLimit::Pixels,
            equal: true
        }
    );
    assert!(mask.is_none(), "a huge header is never decoded");
    drop(control);
}

#[test]
fn a_header_that_disagrees_with_the_data_is_failed() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = bmp_short_data(8, 8);
    fs::write(dir.path().join("left.bmp"), &bytes).unwrap();
    fs::write(dir.path().join("right.bmp"), &bytes).unwrap();
    let (sink, receiver) = channel();
    let control = PairDiff::start(
        request(17),
        local_input(dir.path(), "left.bmp", "right.bmp"),
        sink,
    );
    assert_eq!(opened(&receiver), PairKind::Image);
    match receiver.recv_timeout(support::WAIT).unwrap() {
        DiffUpdate::Failed { request, .. } => assert_eq!(request, 17),
        other => panic!("expected Failed, got {other:?}"),
    }
    drop(control);
}

#[test]
fn one_snapshot_per_side_serves_dimensions_and_decode() {
    let left = ScriptSource::new(png(8, 8, None), usize::MAX, 0);
    let right = ScriptSource::new(png(8, 8, Some((3, 4))), usize::MAX, 0);
    let (sink, receiver) = channel();
    let control = PairDiff::start(request(18), blocked_input(&left, &right), sink);
    assert_eq!(opened(&receiver), PairKind::Image);
    let (scores, _) = image(&receiver, 18);
    assert!(matches!(scores, ImageScores::Scored { .. }));
    assert_eq!(left.opens(), 2, "hash plus one snapshot for the left side");
    assert_eq!(
        right.opens(),
        2,
        "hash plus one snapshot for the right side"
    );
    drop(control);
}

#[test]
fn a_jpeg_with_large_metadata_before_sof_compares() {
    let left = ScriptSource::new(jpeg_with_metadata(false), usize::MAX, 0);
    let right = ScriptSource::new(jpeg_with_metadata(true), usize::MAX, 0);
    let (sink, receiver) = channel();
    let control = PairDiff::start(
        request(19),
        named_input(&left, &right, "left.jpg", "right.jpg"),
        sink,
    );
    assert_eq!(opened(&receiver), PairKind::Image);
    let (scores, _) = image(&receiver, 19);
    assert!(matches!(scores, ImageScores::Scored { .. }));
    assert_eq!(left.opens(), 2, "the whole snapshot was read once");
    drop(control);
}

#[test]
fn a_tiff_ifd_at_the_end_of_the_file_compares() {
    let left = ScriptSource::new(tiff_ifd_at_end(), usize::MAX, 0);
    let right = ScriptSource::new(tiff_ifd_at_end(), usize::MAX, 0);
    let (sink, receiver) = channel();
    let control = PairDiff::start(
        request(20),
        named_input(&left, &right, "left.tiff", "right.tiff"),
        sink,
    );
    assert_eq!(opened(&receiver), PairKind::Image);
    let (scores, _) = image(&receiver, 20);
    assert!(matches!(scores, ImageScores::Scored { .. }));
    drop(control);
}

#[test]
fn a_gif_with_a_huge_logical_screen_is_too_large() {
    let bytes = gif_huge_screen();
    let left = ScriptSource::new(bytes.clone(), usize::MAX, 0);
    let right = ScriptSource::new(bytes, usize::MAX, 0);
    let (sink, receiver) = channel();
    let control = PairDiff::start(
        request(21),
        named_input(&left, &right, "left.gif", "right.gif"),
        sink,
    );
    assert_eq!(opened(&receiver), PairKind::Image);
    let (scores, mask) = image(&receiver, 21);
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
fn a_webp_header_over_64_megapixels_is_too_large() {
    let bytes = webp_huge_header();
    let left = ScriptSource::new(bytes.clone(), usize::MAX, 0);
    let right = ScriptSource::new(bytes, usize::MAX, 0);
    let (sink, receiver) = channel();
    let control = PairDiff::start(
        request(22),
        named_input(&left, &right, "left.webp", "right.webp"),
        sink,
    );
    assert_eq!(opened(&receiver), PairKind::Image);
    let (scores, mask) = image(&receiver, 22);
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
fn a_lossless_webp_under_the_limit_compares() {
    let left = ScriptSource::new(webp_lossless(false), usize::MAX, 0);
    let right = ScriptSource::new(webp_lossless(true), usize::MAX, 0);
    let (sink, receiver) = channel();
    let control = PairDiff::start(
        request(23),
        named_input(&left, &right, "left.webp", "right.webp"),
        sink,
    );
    assert_eq!(opened(&receiver), PairKind::Image);
    let (scores, _) = image(&receiver, 23);
    assert!(matches!(scores, ImageScores::Scored { .. }));
    drop(control);
}

#[test]
fn an_svgz_pair_is_unsupported_not_failed() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("left.svgz"), b"not really an svg").unwrap();
    fs::write(dir.path().join("right.svgz"), b"not really an svg").unwrap();
    let (sink, receiver) = channel();
    let control = PairDiff::start(
        request(24),
        local_input(dir.path(), "left.svgz", "right.svgz"),
        sink,
    );
    assert_eq!(opened(&receiver), PairKind::Image);
    let (scores, mask) = image(&receiver, 24);
    assert_eq!(scores, ImageScores::Unsupported { format: "SVGZ" });
    assert!(mask.is_none());
    drop(control);
}

#[test]
fn the_64_megapixel_boundary_is_exclusive() {
    let at_cap = ImageSize {
        width: 8_000,
        height: 8_000,
    };
    let over_cap = ImageSize {
        width: 8_001,
        height: 8_000,
    };
    assert!(!too_large(at_cap));
    assert!(too_large(over_cap));
}
