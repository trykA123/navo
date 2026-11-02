use ::image::{DynamicImage, Rgb, RgbImage, Rgba, RgbaImage};

use super::*;

fn solid(width: u32, height: u32) -> DynamicImage {
    RgbImage::from_pixel(width, height, Rgb([10, 20, 30])).into()
}

fn solid_rgba(width: u32, height: u32, alpha: u8) -> DynamicImage {
    RgbaImage::from_pixel(width, height, Rgba([10, 20, 30, alpha])).into()
}

#[test]
fn identical_images_score_one_and_leave_the_mask_empty() {
    let image = solid(8, 8);
    let ImageOutcome::Scored { scores, diff } =
        compare(&image, &image).expect("identical images are scored")
    else {
        panic!("identical images must be scored");
    };

    assert!((scores.ssim - 1.0).abs() < 1e-12, "ssim {}", scores.ssim);
    assert!(
        (scores.rgb_min_rms_similarity - 1.0).abs() < 1e-12,
        "rgb rms {}",
        scores.rgb_min_rms_similarity
    );
    assert!(
        (scores.luma_hellinger_similarity - 1.0).abs() < 1e-12,
        "luma histogram {}",
        scores.luma_hellinger_similarity
    );
    assert!(scores.alpha.is_none());
    assert!(diff.to_luma8().pixels().all(|pixel| pixel[0] == 0));
}

#[test]
fn one_changed_pixel_lowers_scores_and_marks_only_that_pixel() {
    let first = solid(8, 8);
    let mut changed = RgbImage::from_pixel(8, 8, Rgb([10, 20, 30]));
    changed.put_pixel(3, 4, Rgb([250, 0, 0]));
    let second: DynamicImage = changed.into();

    let ImageOutcome::Scored { scores, diff } =
        compare(&first, &second).expect("same-size images are scored")
    else {
        panic!("same-size images must be scored");
    };

    assert!(scores.ssim < 1.0, "ssim {}", scores.ssim);
    assert!(
        scores.rgb_min_rms_similarity < 1.0,
        "rgb rms {}",
        scores.rgb_min_rms_similarity
    );
    assert!(
        scores.luma_hellinger_similarity < 1.0,
        "luma histogram {}",
        scores.luma_hellinger_similarity
    );

    let mask = diff.to_luma8();
    let changed_pixels: Vec<_> = mask
        .enumerate_pixels()
        .filter(|(_, _, pixel)| pixel[0] != 0)
        .map(|(x, y, _)| (x, y))
        .collect();
    assert_eq!(changed_pixels, vec![(3, 4)]);
}

#[test]
fn alpha_only_changes_are_reported_and_masked() {
    let first = solid_rgba(8, 8, 255);
    let mut changed = RgbaImage::from_pixel(8, 8, Rgba([10, 20, 30, 255]));
    changed.put_pixel(3, 4, Rgba([10, 20, 30, 0]));
    let second: DynamicImage = changed.into();

    let ImageOutcome::Scored { scores, diff } =
        compare(&first, &second).expect("images with the same RGB are scored")
    else {
        panic!("images with the same RGB must be scored");
    };

    assert!((scores.ssim - 1.0).abs() < 1e-12, "ssim {}", scores.ssim);
    assert!(
        (scores.rgb_min_rms_similarity - 1.0).abs() < 1e-12,
        "rgb rms {}",
        scores.rgb_min_rms_similarity
    );
    assert!(
        (scores.luma_hellinger_similarity - 1.0).abs() < 1e-12,
        "luma histogram {}",
        scores.luma_hellinger_similarity
    );

    let alpha = scores.alpha.expect("both images have alpha");
    assert!(alpha.differs);
    assert_eq!(alpha.max_delta, 255);
    assert!(
        (alpha.mean_delta - 255.0 / 64.0).abs() < 1e-12,
        "mean alpha delta {}",
        alpha.mean_delta
    );

    let mask = diff.to_luma8();
    let marked: Vec<_> = mask
        .enumerate_pixels()
        .filter(|(_, _, pixel)| pixel[0] != 0)
        .map(|(x, y, _)| (x, y))
        .collect();
    assert_eq!(marked, vec![(3, 4)]);
    assert_eq!(mask.get_pixel(3, 4)[0], 255);
}

#[test]
fn an_opaque_alpha_channel_matches_rgb() {
    let rgb: DynamicImage = RgbImage::from_pixel(8, 8, Rgb([10, 20, 30])).into();
    let rgba = solid_rgba(8, 8, 255);

    let ImageOutcome::Scored { scores, diff } =
        compare(&rgb, &rgba).expect("an opaque alpha channel is scored")
    else {
        panic!("an opaque alpha channel must be scored");
    };

    assert!((scores.ssim - 1.0).abs() < 1e-12, "ssim {}", scores.ssim);
    assert!(
        (scores.rgb_min_rms_similarity - 1.0).abs() < 1e-12,
        "rgb rms {}",
        scores.rgb_min_rms_similarity
    );
    assert!(
        (scores.luma_hellinger_similarity - 1.0).abs() < 1e-12,
        "luma histogram {}",
        scores.luma_hellinger_similarity
    );

    let alpha = scores.alpha.expect("one image carries alpha");
    assert!(!alpha.differs);
    assert_eq!(alpha.max_delta, 0);
    assert_eq!(alpha.mean_delta, 0.0);
    assert!(diff.to_luma8().pixels().all(|pixel| pixel[0] == 0));
}

#[test]
fn different_sizes_report_both_dimensions() {
    let first = solid(8, 8);
    let second = solid(9, 8);
    let ImageOutcome::SizeMismatch {
        first: first_size,
        second: second_size,
    } = compare(&first, &second).expect("different sizes are modelled")
    else {
        panic!("different sizes must report a size mismatch");
    };

    assert_eq!(first_size, ImageSize::new(8, 8));
    assert_eq!(second_size, ImageSize::new(9, 8));
}

#[test]
fn images_over_the_cap_fall_back_to_hash_only() {
    let at_cap = ImageSize::new(8000, 8000);
    assert_eq!(at_cap.pixels(), 64_000_000);
    assert!(!at_cap.is_too_large());
    assert_eq!(precheck(at_cap, at_cap), Precheck::Score);

    let over_cap = ImageSize::new(8001, 8000);
    assert!(over_cap.is_too_large());
    assert_eq!(precheck(over_cap, ImageSize::new(1, 1)), Precheck::HashOnly);
    assert_eq!(precheck(ImageSize::new(1, 1), over_cap), Precheck::HashOnly);
}

#[test]
fn an_image_without_pixels_is_an_error() {
    let empty = DynamicImage::ImageRgb8(RgbImage::new(0, 0));
    let error = compare(&empty, &empty).expect_err("an image without pixels cannot be scored");
    assert!(matches!(error, ImageError::Empty));
}
