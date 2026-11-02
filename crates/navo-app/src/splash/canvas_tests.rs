use super::*;

fn rgb(pixel: u32) -> u32 {
    pixel & 0x00FF_FFFF
}

fn alpha(pixel: u32) -> u32 {
    pixel >> 24
}

fn at(canvas: &Canvas, x: usize, y: usize) -> u32 {
    canvas.pixels()[y * canvas.width() + x]
}

#[test]
fn every_embedded_mask_builds_a_canvas_at_common_scales() {
    for scale in [1.0, 1.25, 1.5, 2.0, 3.0] {
        for look in [LIGHT, DARK] {
            assert!(Canvas::new(scale, look).is_some(), "{scale}");
        }
    }
}

#[test]
fn the_canvas_size_follows_the_display_scale() {
    let canvas = Canvas::new(1.5, LIGHT).unwrap();
    assert_eq!((canvas.width(), canvas.height()), (480, 360));
    assert_eq!(canvas.pixels().len(), 480 * 360);
}

#[test]
fn the_corners_are_transparent_and_the_body_is_opaque_surface() {
    let canvas = Canvas::new(2.0, DARK).unwrap();
    assert_eq!(at(&canvas, 0, 0), 0);
    let body = at(&canvas, 40, 440);
    assert_eq!(alpha(body), 255);
    assert_eq!(rgb(body), DARK.surface);
}

#[test]
fn the_bar_shows_its_colour_at_full_opacity_and_fades_toward_the_tile() {
    let mut canvas = Canvas::new(2.0, LIGHT).unwrap();
    let centre = (355, 193);
    assert_eq!(rgb(at(&canvas, centre.0, centre.1)), BAR);
    canvas.set_bar(0.3);
    let faded = at(&canvas, centre.0, centre.1);
    assert_eq!(rgb(faded), rgb(blend(TILE, BAR, 0.3)));
    assert_eq!(alpha(faded), 255);
}

#[test]
fn fading_the_bar_changes_no_pixel_outside_it() {
    let full = Canvas::new(1.25, LIGHT).unwrap();
    let mut faded = Canvas::new(1.25, LIGHT).unwrap();
    faded.set_bar(0.3);
    faded.set_bar(1.0);
    assert!(full.pixels() == faded.pixels());
}

#[test]
fn the_status_line_sits_inside_the_opaque_body() {
    let canvas = Canvas::new(1.0, LIGHT).unwrap();
    let [left, top, right, bottom] = canvas.status();
    assert!(left > 12 && right < 308 && top > 160 && bottom < 240);
    for (x, y) in [(left, top), (right - 1, bottom - 1)] {
        assert_eq!(alpha(at(&canvas, x as usize, y as usize)), 255);
    }
}

#[test]
fn making_text_opaque_touches_only_its_rectangle() {
    let mut pixels = vec![0x0012_3456_u32; 6 * 4];
    make_opaque(&mut pixels, 6, [1, 1, 3, 3]);
    let opaque: Vec<usize> = (0..pixels.len())
        .filter(|i| alpha(pixels[*i]) == 255)
        .collect();
    assert_eq!(opaque, vec![7, 8, 13, 14]);
    assert!(pixels.iter().all(|pixel| rgb(*pixel) == 0x12_3456));
}

#[test]
fn a_damaged_mask_is_refused() {
    assert!(parse(b"P5\n# at 0 0\n4 4\n255\nshort").is_none());
    assert!(parse(b"P6\n# at 0 0\n1 1\n255\nx").is_none());
    assert!(parse(b"P5\n# at 0 0\n1 1\n255\nx").is_some());
}

fn frame(bar: f32, is_slow: bool) -> Frame {
    Frame { bar, is_slow }
}

#[test]
fn the_first_frame_copies_the_whole_canvas() {
    let canvas = Canvas::new(1.25, LIGHT).unwrap();
    let rows = canvas.changed_rows(None, frame(1.0, false));
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0], 0..canvas.height());
}

#[test]
fn a_bar_step_copies_only_the_bar_rows() {
    let canvas = Canvas::new(1.5, DARK).unwrap();
    let rows = canvas.changed_rows(Some(frame(1.0, true)), frame(0.6, true));
    assert_eq!(rows.len(), 1);
    assert!(rows[0].len() < canvas.height() / 4, "{rows:?}");
}

#[test]
fn the_slow_line_appearing_also_copies_the_status_rows() {
    let canvas = Canvas::new(1.0, LIGHT).unwrap();
    let rows = canvas.changed_rows(Some(frame(0.6, false)), frame(0.5, true));
    let [_, top, _, bottom] = canvas.status();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1], top as usize..bottom as usize);
}

#[test]
fn the_bar_rows_never_overlap_the_status_rows() {
    for scale in [1.0, 1.25, 1.5, 2.0, 3.0] {
        let canvas = Canvas::new(scale, LIGHT).unwrap();
        let rows = canvas.changed_rows(Some(frame(0.6, false)), frame(0.5, true));
        assert!(rows[0].end <= rows[1].start, "{scale}: {rows:?}");
    }
}

#[test]
fn copying_rows_changes_nothing_outside_them() {
    let from = vec![7_u32; 5 * 4];
    let mut to = vec![0_u32; 5 * 4];
    copy_rows(&mut to, &from, 5, &Vec::from_iter([1..2, 3..9]));
    let copied: Vec<usize> = (0..to.len()).filter(|i| to[*i] == 7).collect();
    assert_eq!(copied, vec![5, 6, 7, 8, 9, 15, 16, 17, 18, 19]);
}
