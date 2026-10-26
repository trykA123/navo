use gpui_kit::{point, size};
use image::ImageFormat;

use super::decode::{Picture, decode_image};
use super::fixtures::{
    BADGE_SVG, BLUE, CORNERS, RED, corners_png, encoded, gzip, nested_svg, photo, png_claiming,
};
use super::svg::unzipped;
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
    assert_eq!(error, "This file is not an image Navo can show.");
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

#[test]
fn a_gzipped_svg_that_inflates_past_the_limit_is_refused() {
    let head: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><!--"#;
    let bomb = gzip(&[head, &[b'a'; 1024], b"--></svg>"], 2048);
    let error = unzipped(&bomb, 1024 * 1024).expect_err("refused");
    assert!(error.starts_with("This SVG is too large"), "{error}");
    let plain = unzipped(&bomb, 4 * 1024 * 1024).expect("fits");
    assert!(plain.starts_with(b"<svg"));
    let twice = gzip(&[&gzip(&[BADGE_SVG.as_bytes()], 1)], 1);
    assert!(unzipped(&twice, 1024 * 1024).is_err());
    let zipped = gzip(&[BADGE_SVG.as_bytes()], 1);
    let picture = decode_image(&zipped, "badge.svgz").expect("svgz");
    assert_eq!((picture.width, picture.height), (24, 12));
}

#[test]
fn a_deeply_nested_svg_gives_a_message_even_on_a_small_stack() {
    let deep = nested_svg(1000);
    let outcome = std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(move || decode_image(&deep, "deep.svg").err())
        .expect("thread")
        .join()
        .expect("no overflow");
    let error = outcome.expect("refused");
    assert!(error.contains("nested too deeply"), "{error}");
    assert!(decode_image(&nested_svg(8), "fine.svg").is_ok());
}

#[test]
fn a_raster_needing_more_than_the_decode_budget_is_refused() {
    let error = decode_image(&png_claiming(9000, 9000), "wide.png")
        .err()
        .expect("refused");
    assert_eq!(error, "This image is too large to show here.");
}

#[test]
fn every_exif_orientation_puts_the_corners_in_place() {
    let [r, g, b, w] = CORNERS;
    let table = [
        (1, [r, g, b, w]),
        (2, [g, r, w, b]),
        (3, [w, b, g, r]),
        (4, [b, w, r, g]),
        (5, [r, b, g, w]),
        (6, [b, r, w, g]),
        (7, [w, g, b, r]),
        (8, [g, w, r, b]),
    ];
    for (turn, wanted) in table {
        let picture = decode_image(&corners_png(turn), "corners.png").expect("png");
        let (width, height) = if turn >= 5 { (8, 16) } else { (16, 8) };
        assert_eq!((picture.width, picture.height), (width, height), "{turn}");
        let found = [
            pixel(&picture, 0, 0),
            pixel(&picture, width - 1, 0),
            pixel(&picture, 0, height - 1),
            pixel(&picture, width - 1, height - 1),
        ];
        assert_eq!(found, wanted, "orientation {turn}");
    }
}

#[test]
fn angle_brackets_inside_cdata_do_not_count_as_nesting() {
    let style = format!("<style><![CDATA[{}]]></style>", "a<b>".repeat(300));
    let svg = BADGE_SVG.replace("<rect", &format!("{style}<rect"));
    assert!(decode_image(svg.as_bytes(), "styled.svg").is_ok());
}
