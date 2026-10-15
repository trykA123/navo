use gpui_kit::{point, size};
use image::ImageFormat;

use super::decode::{Picture, decode_image};
use super::fixtures::{BADGE_SVG, BLUE, RED, encoded, photo};
use super::zoom::{Viewport, Zoom};

fn pixel(picture: &Picture, x: u32, y: u32) -> [u8; 3] {
    let width = picture.pixels.size(0).width.0 as u32;
    let bytes = picture.pixels.as_bytes(0).expect("one frame");
    let at = ((y * width + x) * 4) as usize;
    [bytes[at + 2], bytes[at + 1], bytes[at]]
}

fn near(found: [u8; 3], wanted: [u8; 3]) -> bool {
    found
        .iter()
        .zip(wanted)
        .all(|(found, wanted)| found.abs_diff(wanted) < 40)
}

#[test]
fn every_raster_format_decodes_with_its_size_and_name() {
    let formats = [
        (ImageFormat::Png, "PNG"),
        (ImageFormat::Jpeg, "JPEG"),
        (ImageFormat::Gif, "GIF"),
        (ImageFormat::WebP, "WebP"),
        (ImageFormat::Bmp, "BMP"),
        (ImageFormat::Ico, "ICO"),
        (ImageFormat::Tiff, "TIFF"),
    ];
    for (format, name) in formats {
        let bytes = encoded(format, 32, 16);
        let picture =
            decode_image(&bytes, "picture").unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(
            (
                picture.width,
                picture.height,
                picture.format,
                picture.vector
            ),
            (32, 16, name, false)
        );
        assert_eq!(picture.bytes, bytes.len() as u64);
        assert!(near(pixel(&picture, 2, 8), RED), "{name} left half");
        assert!(near(pixel(&picture, 30, 8), BLUE), "{name} right half");
    }
}

#[test]
fn svg_renders_from_its_name_or_its_tag_with_the_intrinsic_size() {
    for name in ["badge.svg", "badge"] {
        let picture = decode_image(BADGE_SVG.as_bytes(), name).expect("svg");
        assert_eq!(
            (picture.width, picture.height, picture.format),
            (24, 12, "SVG")
        );
        assert!(picture.vector);
        let shown = picture.pixels.size(0);
        assert_eq!((shown.width.0, shown.height.0), (2048, 1024));
        assert!(near(pixel(&picture, 100, 500), RED));
        let right = picture.pixels.as_bytes(0).expect("frame");
        let at = ((500 * 2048 + 1900) * 4) as usize;
        assert_eq!(right[at + 3], 0, "the right half is transparent");
    }
}

#[test]
fn exif_orientation_turns_the_photo_and_the_fields_are_listed() {
    let picture = decode_image(&photo(32, 16, 6), "shot.jpg").expect("jpeg");
    assert_eq!((picture.width, picture.height), (16, 32));
    assert!(
        near(pixel(&picture, 8, 4), RED),
        "the left half is now on top"
    );
    assert!(near(pixel(&picture, 8, 28), BLUE));
    let fields: Vec<String> = picture
        .exif
        .iter()
        .map(|entry| format!("{}: {}", entry.label, entry.value))
        .collect();
    assert_eq!(
        fields,
        [
            "Camera: FUJIFILM X-T5",
            "Lens: XF23mmF1.4 R LM WR",
            "Taken: 2026-10-15 09:30:00",
            "Exposure: 1/250 s · f/1.4",
            "ISO: 400",
            "Focal length: 23 mm",
            "GPS: 44.42667° N, 26.10000° E",
            "Orientation: Turned 90° right",
        ]
    );
}

#[test]
fn a_photo_without_exif_lists_nothing() {
    let picture = decode_image(&encoded(ImageFormat::Png, 4, 4), "plain.png").expect("png");
    assert!(picture.exif.is_empty());
}

#[test]
fn damaged_or_foreign_bytes_give_a_message_not_a_panic() {
    let mut damaged = encoded(ImageFormat::Png, 32, 16);
    damaged.truncate(40);
    let error = decode_image(&damaged, "broken.png")
        .err()
        .expect("an error");
    assert!(error.starts_with("Cannot show this image"), "{error}");
    let error = decode_image(b"just words", "notes.txt")
        .err()
        .expect("an error");
    assert_eq!(error, "This file is not an image Ferrite can show.");
    let error = decode_image(b"<svg", "broken.svg").err().expect("an error");
    assert!(error.starts_with("Cannot show this SVG"), "{error}");
}

#[test]
fn a_huge_image_is_shrunk_for_display_but_keeps_its_size() {
    let bytes = encoded(ImageFormat::Png, 9000, 10);
    let picture = decode_image(&bytes, "strip.png").expect("png");
    assert_eq!((picture.width, picture.height), (9000, 10));
    let shown = picture.pixels.size(0);
    assert_eq!(shown.width.0, 8192);
}

fn viewport(width: f32, height: f32) -> Viewport {
    Viewport {
        origin: point(0., 0.),
        size: size(width, height),
        device: 1.,
    }
}

#[test]
fn fit_never_enlarges_a_raster_image_but_does_enlarge_a_vector() {
    let room = viewport(800., 400.);
    assert_eq!(Zoom::new(2000, 500, false).percent(&room), 40);
    assert_eq!(Zoom::new(100, 50, false).percent(&room), 100);
    assert_eq!(Zoom::new(100, 50, true).percent(&room), 800);
    let crisp = Viewport { device: 2., ..room };
    assert_eq!(Zoom::new(2000, 500, false).percent(&crisp), 80);
}

#[test]
fn zooming_keeps_the_point_under_the_pointer_still() {
    let room = viewport(800., 400.);
    let mut zoom = Zoom::new(1600, 800, false);
    zoom.zoom_to(1., point(400., 200.), &room);
    assert_eq!(zoom.pan, point(0., 0.));
    zoom.zoom_by(2., point(200., 100.), &room);
    assert_eq!(zoom.label(&room), "200 %");
    assert_eq!(zoom.pan, point(200., 100.));
    let (origin, _) = zoom.placement(&room);
    assert_eq!(origin, point(-1000., -500.));
}

#[test]
fn panning_stops_at_the_edges_and_a_small_side_stays_centred() {
    let room = viewport(800., 400.);
    let mut zoom = Zoom::new(1000, 300, false);
    zoom.zoom_to(1., point(400., 200.), &room);
    zoom.pan_by(point(500., 500.), &room);
    assert_eq!(zoom.pan, point(100., 0.));
    zoom.pan_by(point(-900., -500.), &room);
    assert_eq!(zoom.pan, point(-100., 0.));
    zoom.fit();
    zoom.pan_by(point(50., 50.), &room);
    assert_eq!(zoom.pan, point(0., 0.));
}
