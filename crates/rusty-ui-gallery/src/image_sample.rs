use std::io::Cursor;

use image::{ImageFormat, Rgba, RgbaImage};
use rusty_ui::prelude::*;
use rusty_ui::window_view::{ExifEntry, ImageView, decode_image};

const WIDTH: u32 = 1600;
const HEIGHT: u32 = 1000;
const NAME: &str = "carpathians-dawn.png";

fn mix(from: [f32; 3], to: [f32; 3], amount: f32) -> [f32; 3] {
    let amount = amount.clamp(0., 1.);
    [0, 1, 2].map(|at| from[at] + (to[at] - from[at]) * amount)
}

fn ridge(x: f32, scale: f32, phase: f32) -> f32 {
    (x * scale + phase).sin() * 0.5 + (x * scale * 2.3 + phase * 1.7).sin() * 0.25
}

fn colour(x: f32, y: f32) -> [f32; 3] {
    let sky = mix([46., 64., 120.], [246., 170., 120.], y / 0.62);
    let sun = ((x - 0.62).powi(2) + (y - 0.5).powi(2)).sqrt();
    let sky = mix(sky, [255., 236., 196.], 1. - sun / 0.07);
    let far = 0.55 + ridge(x, 7., 1.) * 0.05;
    let near = 0.68 + ridge(x, 4., 3.) * 0.07;
    if y > near {
        mix([40., 52., 70.], [20., 26., 38.], (y - near) / 0.3)
    } else if y > far {
        mix([92., 98., 132.], [70., 76., 108.], (y - far) / 0.15)
    } else {
        sky
    }
}

fn alpha(x: f32, y: f32) -> f32 {
    let (dx, dy) = ((x - 0.5) * 1.6, y - 0.5);
    let distance = (dx * dx + dy * dy).sqrt();
    ((0.48 - distance) / 0.01).clamp(0., 1.)
}

fn landscape() -> RgbaImage {
    RgbaImage::from_fn(WIDTH, HEIGHT, |x, y| {
        let (x, y) = (x as f32 / WIDTH as f32, y as f32 / HEIGHT as f32);
        let [r, g, b] = colour(x, y);
        Rgba([r as u8, g as u8, b as u8, (alpha(x, y) * 255.) as u8])
    })
}

fn entry(label: &'static str, value: &str) -> ExifEntry {
    ExifEntry {
        label,
        value: value.to_owned(),
    }
}

fn details() -> Vec<ExifEntry> {
    vec![
        entry("Camera", "FUJIFILM X-T5"),
        entry("Lens", "XF23mmF1.4 R LM WR"),
        entry("Taken", "2026-10-15 07:12:40"),
        entry("Exposure", "1/500 s · f/8"),
        entry("ISO", "160"),
        entry("Focal length", "23 mm"),
        entry("GPS", "45.59812° N, 25.46021° E"),
        entry("Orientation", "Normal"),
    ]
}

pub fn image_view(cx: &mut App) -> Result<Entity<ImageView>, String> {
    let mut png = Cursor::new(Vec::new());
    landscape()
        .write_to(&mut png, ImageFormat::Png)
        .map_err(|error| error.to_string())?;
    let mut picture = decode_image(png.get_ref(), NAME)?;
    picture.exif = details();
    Ok(cx.new(|cx| ImageView::new(NAME, picture, cx)))
}
