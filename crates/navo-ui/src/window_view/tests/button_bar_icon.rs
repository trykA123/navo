use std::sync::Arc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext as _, Bounds, Context, IntoElement, ParentElement, Point, Render, Styled,
    TestAppContext, Window, WindowBounds, WindowOptions, div, px, size,
};
use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder, Rgba, RgbaImage};
use navo_icons::registry::registry;
use navo_icons::spec::{BrandColour, Shape, TONES, Tone};

use crate::tokens::{self, Mode, Theme, contrast_ratio};
use crate::window_view::button_bar::BarIcon;
use crate::window_view::button_bar::icon::{
    brand_or_theme, icon_element, letter_colours, tone_colour,
};
use crate::window_view::button_bar::raster_cache::{
    MAX_CACHE_ENTRIES, RasterCache, bucket_px, png_image, svg_image,
};

fn rgb_to_hsla([r, g, b]: [u8; 3]) -> gpui_kit::Hsla {
    gpui_kit::Hsla::from(gpui_kit::Rgba {
        r: f32::from(r) / 255.0,
        g: f32::from(g) / 255.0,
        b: f32::from(b) / 255.0,
        a: 1.0,
    })
}

fn hsla_to_rgb(color: gpui_kit::Hsla) -> [u8; 3] {
    let rgba = gpui_kit::Rgba::from(color);
    [
        (rgba.r.clamp(0.0, 1.0) * 255.0).round() as u8,
        (rgba.g.clamp(0.0, 1.0) * 255.0).round() as u8,
        (rgba.b.clamp(0.0, 1.0) * 255.0).round() as u8,
    ]
}

fn sample_png(color: [u8; 4], side: u32) -> Arc<[u8]> {
    let img = RgbaImage::from_pixel(side, side, Rgba(color));
    let mut out = Vec::new();
    PngEncoder::new(&mut out)
        .write_image(img.as_raw(), side, side, ExtendedColorType::Rgba8)
        .expect("encode png fixture");
    Arc::from(out)
}

fn sample_colour_svg(hex: &str) -> Arc<[u8]> {
    let doc = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 16 16\">\
         <rect width=\"16\" height=\"16\" fill=\"{hex}\"/>\
         </svg>"
    );
    Arc::from(doc.into_bytes())
}

#[test]
fn button_bar_icon_tone_colour_maps_all_twelve_tones() {
    for mode in [Mode::Light, Mode::Dark] {
        let colors = Theme::new(mode).colors;
        let expected = [
            (Tone::Accent, colors.accent),
            (Tone::Ok, colors.ok),
            (Tone::Warn, colors.warn),
            (Tone::Folder, colors.folder),
            (Tone::KindCode, colors.kind_code),
            (Tone::KindAuto, colors.kind_auto),
            (Tone::KindImage, colors.kind_image),
            (Tone::KindArchive, colors.kind_archive),
            (Tone::KindDoc, colors.kind_doc),
            (Tone::KindApp, colors.kind_app),
            (Tone::Muted, colors.muted),
            (Tone::Text, colors.text),
        ];
        assert_eq!(expected.len(), TONES.len());
        for (tone, want) in expected {
            assert_eq!(tone_colour(tone, &colors), want, "{mode:?} {tone:?}");
        }
    }
}

#[test]
fn button_bar_icon_brand_or_theme_enforces_contrast() {
    for mode in [Mode::Light, Mode::Dark] {
        let colors = Theme::new(mode).colors;
        let rail_rgb = hsla_to_rgb(colors.rail);
        assert_eq!(
            brand_or_theme(Some([0, 0, 0]), BrandColour::Theme, &colors),
            colors.text
        );
        assert_eq!(
            brand_or_theme(None, BrandColour::Brand, &colors),
            colors.text
        );
        assert_eq!(
            brand_or_theme(Some(rail_rgb), BrandColour::Brand, &colors),
            colors.text
        );

        let high_contrast = match mode {
            Mode::Light => [0, 0, 0],
            Mode::Dark => [255, 255, 255],
        };
        let expected = rgb_to_hsla(high_contrast);
        assert!(contrast_ratio(expected, colors.rail) >= 3.0);
        assert_eq!(
            brand_or_theme(Some(high_contrast), BrandColour::Brand, &colors),
            expected
        );
    }
}

#[test]
fn button_bar_icon_letter_colours_handles_all_five_shapes() {
    let light = Theme::new(Mode::Light).colors;
    let dark_tone = rgb_to_hsla([20, 24, 30]);
    let light_tone = rgb_to_hsla([240, 244, 248]);

    for shape in [Shape::Solid, Shape::Circle] {
        let (fill, text) = letter_colours(shape, dark_tone, &light);
        assert_eq!(fill, Some(dark_tone));
        assert_eq!(text, light.background);

        let (fill, text) = letter_colours(shape, light_tone, &light);
        assert_eq!(fill, Some(light_tone));
        assert_eq!(text, light.text);
    }

    assert_eq!(
        letter_colours(Shape::Soft, light.accent, &light),
        (Some(light.accent.opacity(0.16)), light.accent)
    );
    assert_eq!(
        letter_colours(Shape::Outline, light.ok, &light),
        (None, light.ok)
    );
    assert_eq!(
        letter_colours(Shape::None, light.warn, &light),
        (None, light.warn)
    );
}

#[gpui_kit::test]
fn button_bar_icon_raster_cache_snaps_buckets_and_evicts_lru(cx: &mut TestAppContext) {
    assert_eq!(bucket_px(px(16.), 1.0), 16);
    assert_eq!(bucket_px(px(16.), 1.25), 24);
    assert_eq!(bucket_px(px(16.), 1.5), 24);
    assert_eq!(bucket_px(px(16.), 1.75), 32);
    assert_eq!(bucket_px(px(16.), 2.0), 32);
    assert_eq!(bucket_px(px(16.), 3.0), 32);

    cx.update(|cx| {
        let svg = sample_colour_svg("#2563eb");
        let first = svg_image(&svg, px(16.), 1.0, cx).expect("svg decode 1x");
        let again = svg_image(&svg, px(16.), 1.0, cx).expect("svg cache hit");
        assert!(Arc::ptr_eq(&first, &again));
        assert_eq!((first.size(0).width.0, first.size(0).height.0), (16, 16));

        let hi = svg_image(&svg, px(16.), 2.0, cx).expect("svg decode 2x");
        assert!(!Arc::ptr_eq(&first, &hi));
        assert_eq!((hi.size(0).width.0, hi.size(0).height.0), (32, 32));

        let mut pngs = Vec::with_capacity(MAX_CACHE_ENTRIES);
        for idx in 0..MAX_CACHE_ENTRIES {
            let png = sample_png([idx as u8, 80, 180, 255], 32);
            let decoded = png_image(&png, px(16.), 1.5, cx).expect("png decode");
            assert_eq!(
                (decoded.size(0).width.0, decoded.size(0).height.0),
                (24, 24)
            );
            pngs.push(png);
        }
        assert_eq!(cx.default_global::<RasterCache>().len(), MAX_CACHE_ENTRIES);

        let touched = png_image(&pngs[0], px(16.), 1.5, cx).expect("touch entry 0");
        let extra = sample_png([255, 10, 10, 255], 32);
        let _ = png_image(&extra, px(16.), 1.5, cx).expect("evict entry 1");

        let cache = cx.default_global::<RasterCache>();
        assert_eq!(cache.len(), MAX_CACHE_ENTRIES);
        assert!(cache.contains(&pngs[0], 24));
        assert!(!cache.contains(&pngs[1], 24));
        drop(touched);
    });
}

struct AllIconsView {
    icons: Vec<BarIcon>,
}

impl Render for AllIconsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut row = div().flex().gap(px(4.));
        for icon in &self.icons {
            row = row.child(icon_element(icon, px(16.), cx));
        }
        row
    }
}

fn sample_icon_set() -> (Vec<BarIcon>, Arc<[u8]>, Arc<[u8]>) {
    let mono_svg = registry().body("lucide", "folder").unwrap_or_else(|| {
        Arc::from(
            b"<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\"><path d=\"M4 4h16v16H4z\" fill=\"#000000\"/></svg>".as_slice(),
        )
    });
    let colour_svg = sample_colour_svg("#16a34a");
    let png_bytes = sample_png([220, 38, 38, 255], 32);

    let icons = vec![
        BarIcon::Mono {
            svg: Arc::clone(&mono_svg),
            tone: Some(Tone::Accent),
            brand: None,
        },
        BarIcon::Mono {
            svg: Arc::clone(&mono_svg),
            tone: None,
            brand: Some([24, 119, 242]),
        },
        BarIcon::Colour {
            svg: Arc::clone(&colour_svg),
        },
        BarIcon::Raster {
            png: Arc::clone(&png_bytes),
        },
        BarIcon::Letters {
            letters: String::from("AB"),
            shape: Shape::Soft,
            tone: Tone::Accent,
        },
        BarIcon::Letters {
            letters: String::from("C"),
            shape: Shape::Circle,
            tone: Tone::Ok,
        },
        BarIcon::Letters {
            letters: String::from("O"),
            shape: Shape::Outline,
            tone: Tone::Warn,
        },
        BarIcon::Letters {
            letters: String::from("S"),
            shape: Shape::Solid,
            tone: Tone::Folder,
        },
        BarIcon::Letters {
            letters: String::from("N"),
            shape: Shape::None,
            tone: Tone::Muted,
        },
        BarIcon::Glyph {
            svg: Arc::clone(&mono_svg),
            tone: Tone::Text,
            dot: Some(Tone::Warn),
        },
        BarIcon::Glyph {
            svg: mono_svg,
            tone: Tone::Muted,
            dot: None,
        },
        BarIcon::Missing { dashed: true },
        BarIcon::Missing { dashed: false },
    ];
    (icons, colour_svg, png_bytes)
}

#[gpui_kit::test]
fn button_bar_icon_draws_each_kind(cx: &mut TestAppContext) {
    let (icons, colour_svg, png_bytes) = sample_icon_set();

    for mode in [Mode::Light, Mode::Dark] {
        let icons_for_window = icons.clone();
        let window = cx.update(|cx| {
            gpui_kit::init(cx);
            tokens::apply(mode, cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: Point::default(),
                    size: size(px(320.), px(64.)),
                })),
                ..Default::default()
            };
            gpui_kit::open_window(options, cx, |_, cx| {
                cx.new(|_| AllIconsView {
                    icons: icons_for_window,
                })
            })
            .expect("open test window")
            .0
        });
        let bucket = cx
            .update_window(window, |_, window, cx| {
                window.render_frame(cx);
                bucket_px(px(16.), window.scale_factor())
            })
            .expect("render frame");

        cx.read(|cx| {
            let cache = cx.try_global::<RasterCache>().expect("raster cache");
            assert!(cache.contains(&colour_svg, bucket));
            assert!(cache.contains(&png_bytes, bucket));
        });
    }
}
