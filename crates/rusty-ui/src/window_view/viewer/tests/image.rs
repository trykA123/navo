use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, Point, TestAppContext};
use image::ImageFormat;

use super::find::bottom_keys;
use super::{Log, press_only, redraw, text_of, visible, window_with};
use crate::format;
use crate::window_view::FerriteWindow;
use crate::window_view::tests::{fake, press, settle};
use crate::window_view::viewer::image::ImageBody;
use crate::window_view::viewer::image::fixtures::{encoded, photo};
use crate::window_view::viewer::{FileHooks, FileText, Head, Kinds};

#[derive(Clone, Default)]
struct Disk {
    files: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    loads: Arc<AtomicUsize>,
    log: Log,
}

fn name_of(location: &rusty_core::Location) -> String {
    location
        .name()
        .map(|name| name.to_text())
        .unwrap_or_default()
}

impl Disk {
    fn with(files: &[(&str, Vec<u8>)]) -> Self {
        let disk = Self::default();
        let mut stored = disk.files.lock().expect("disk");
        for (name, bytes) in files {
            stored.insert((*name).to_owned(), bytes.clone());
        }
        drop(stored);
        disk
    }

    fn bytes_of(&self, location: &rusty_core::Location) -> Vec<u8> {
        let files = self.files.lock().expect("disk");
        files.get(&name_of(location)).cloned().unwrap_or_default()
    }

    fn hooks(&self) -> FileHooks {
        let (head, read, whole) = (self.clone(), self.clone(), self.clone());
        FileHooks {
            head: Arc::new(move |location| {
                head.log.heads.lock().expect("log").push(name_of(location));
                let bytes = head.bytes_of(location);
                Ok(Head {
                    len: bytes.len() as u64,
                    bytes: bytes.into_iter().take(8 * 1024).collect(),
                })
            }),
            read: Arc::new(move |location| {
                let bytes = read.bytes_of(location);
                Ok(FileText::plain(String::from_utf8_lossy(&bytes)))
            }),
            bytes: Arc::new(move |location, limit| {
                whole.loads.fetch_add(1, Ordering::Relaxed);
                let bytes = whole.bytes_of(location);
                if bytes.len() as u64 > limit {
                    return Err("Too large.".to_owned());
                }
                Ok(bytes)
            }),
            ..FileHooks::none()
        }
    }

    fn loads(&self) -> usize {
        self.loads.load(Ordering::Relaxed)
    }
}

type Rig = (AnyWindowHandle, Entity<FerriteWindow>, Disk);

fn rig(cx: &mut TestAppContext, extra: &[&str], files: &[(&str, Vec<u8>)]) -> Rig {
    let fs = fake();
    for name in extra {
        fs.add_file(&format!("home/{name}"), 4);
    }
    let disk = Disk::with(files);
    let (window, view) = window_with(cx, fs, disk.hooks(), Kinds::builtin());
    (window, view, disk)
}

fn open_first(cx: &mut TestAppContext, rig: &Rig) {
    let (window, view, _) = rig;
    press(cx, *window, view, "down");
    press(cx, *window, view, "f3");
    redraw(cx, *window);
}

fn image_view(
    cx: &mut TestAppContext,
    view: &Entity<FerriteWindow>,
) -> Option<Entity<crate::window_view::viewer::ImageView>> {
    view.read_with(cx, |view, _| {
        let body = view.viewer.as_ref()?.body.as_ref()?;
        ImageBody::of(body.as_ref()).map(|image| image.0.clone())
    })
}

fn pan(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> Point<f32> {
    let image = image_view(cx, view).expect("an image is open");
    image.read_with(cx, |image, _| image.pan())
}

fn title(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> String {
    view.read_with(cx, |view, _| {
        view.viewer
            .as_ref()
            .map(|viewer| viewer.title.to_string())
            .unwrap_or_default()
    })
}

fn presses(cx: &mut TestAppContext, rig: &Rig, key: &str, times: usize) {
    for _ in 0..times {
        press(cx, rig.0, &rig.1, key);
    }
}

#[gpui_kit::test]
fn a_png_decodes_in_the_background_then_shows_its_size_and_format(cx: &mut TestAppContext) {
    let png = encoded(ImageFormat::Png, 300, 200);
    let meta = format!("300 × 200 · PNG · {}", format::size(png.len() as u64));
    let rig = rig(cx, &[], &[("a.txt", png)]);
    let (window, view, disk) = &rig;
    press(cx, *window, view, "down");
    press_only(cx, *window, "f3");
    assert_eq!(text_of(cx, *window, "viewer-status"), "Opening…");
    assert_eq!(disk.loads(), 0);
    settle(cx, view);
    redraw(cx, *window);
    assert_eq!(disk.loads(), 1);
    assert_eq!(text_of(cx, *window, "image-meta"), meta);
    assert_eq!(text_of(cx, *window, "image-zoom"), "Fit · 100 %");
    let keys = bottom_keys(cx, *window);
    for key in [
        "0 Fit",
        "1 100 %",
        "I Info",
        "Alt+1 Text",
        "3 Hex",
        "N Next file",
    ] {
        assert!(keys.contains(&key.to_owned()), "{key} in {keys:?}");
    }
}

#[gpui_kit::test]
fn fit_actual_size_and_zoom_keys_change_the_zoom(cx: &mut TestAppContext) {
    let rig = rig(cx, &[], &[("a.txt", encoded(ImageFormat::Png, 3000, 2000))]);
    open_first(cx, &rig);
    let (window, view, _) = &rig;
    let zoom = |cx: &mut TestAppContext| text_of(cx, *window, "image-zoom");
    let fitted = zoom(cx);
    assert!(
        fitted.starts_with("Fit · ") && fitted != "Fit · 100 %",
        "{fitted}"
    );
    press(cx, *window, view, "1");
    assert_eq!(zoom(cx), "100 %");
    press(cx, *window, view, "+");
    assert_eq!(zoom(cx), "125 %");
    press(cx, *window, view, "-");
    press(cx, *window, view, "-");
    assert_eq!(zoom(cx), "80 %");
    press(cx, *window, view, "=");
    assert_eq!(zoom(cx), "100 %");
    press(cx, *window, view, "ctrl-0");
    assert_eq!(zoom(cx), fitted);
    press(cx, *window, view, "1");
    press(cx, *window, view, "0");
    assert_eq!(zoom(cx), fitted);
}

#[gpui_kit::test]
fn arrows_pan_and_stop_at_the_edges(cx: &mut TestAppContext) {
    let rig = rig(cx, &[], &[("a.txt", encoded(ImageFormat::Png, 3000, 2000))]);
    open_first(cx, &rig);
    let view = rig.1.clone();
    presses(cx, &rig, "left", 3);
    assert_eq!(
        pan(cx, &view),
        Point::default(),
        "a fitted image does not pan"
    );
    press(cx, rig.0, &view, "1");
    presses(cx, &rig, "left", 40);
    let left = pan(cx, &view);
    assert!(left.x > 0., "{left:?}");
    press(cx, rig.0, &view, "left");
    assert_eq!(pan(cx, &view), left);
    presses(cx, &rig, "right", 80);
    assert_eq!(pan(cx, &view).x, -left.x);
    presses(cx, &rig, "up", 40);
    let top = pan(cx, &view).y;
    assert!(top > 0.);
    presses(cx, &rig, "down", 80);
    assert_eq!(pan(cx, &view).y, -top);
}

#[gpui_kit::test]
fn next_and_previous_skip_files_that_are_not_images_and_stop_at_the_ends(cx: &mut TestAppContext) {
    let png = encoded(ImageFormat::Png, 8, 8);
    let files = [
        ("a.txt", b"hello\n".to_vec()),
        ("b.png", png.clone()),
        ("c.txt", b"words\n".to_vec()),
        ("d.jpg", encoded(ImageFormat::Jpeg, 8, 8)),
        ("e.txt", b"more\n".to_vec()),
    ];
    let rig = rig(cx, &["b.png", "c.txt", "d.jpg", "e.txt"], &files);
    open_first(cx, &rig);
    let view = rig.1.clone();
    let mut seen = Vec::new();
    for key in ["n", "n", "n", "backspace", "backspace", "space", "p"] {
        press(cx, rig.0, &view, key);
        seen.push(title(cx, &view));
    }
    assert_eq!(
        seen,
        [
            "b.png", "d.jpg", "d.jpg", "b.png", "b.png", "d.jpg", "b.png"
        ]
    );
    assert!(image_view(cx, &view).is_some());
}

fn exif_rows(cx: &mut TestAppContext, window: AnyWindowHandle) -> Vec<String> {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        (0usize..)
            .map_while(|index| window.try_find(("image-exif", index)))
            .filter_map(|found| found.label().map(str::to_owned))
            .collect()
    })
    .expect("window")
}

#[gpui_kit::test]
fn i_shows_the_exif_panel_and_the_orientation_is_applied(cx: &mut TestAppContext) {
    let rig = rig(cx, &[], &[("a.txt", photo(32, 16, 6))]);
    open_first(cx, &rig);
    let (window, view, _) = &rig;
    assert!(text_of(cx, *window, "image-meta").starts_with("16 × 32 · JPEG"));
    assert!(!visible(cx, *window, "image-info"));
    press(cx, *window, view, "i");
    assert!(visible(cx, *window, "image-info"));
    let rows = exif_rows(cx, *window);
    assert_eq!(
        rows.first().map(String::as_str),
        Some("Camera: FUJIFILM X-T5")
    );
    assert!(rows.contains(&"ISO: 400".to_owned()), "{rows:?}");
    assert!(
        rows.contains(&"Orientation: Turned 90° right".to_owned()),
        "{rows:?}"
    );
    press(cx, *window, view, "i");
    assert!(!visible(cx, *window, "image-info"));
}

#[gpui_kit::test]
fn a_damaged_image_shows_a_message_and_hex_still_opens(cx: &mut TestAppContext) {
    let mut damaged = encoded(ImageFormat::Png, 32, 16);
    damaged.truncate(40);
    let rig = rig(cx, &[], &[("a.txt", damaged)]);
    open_first(cx, &rig);
    let (window, view, _) = &rig;
    let message = text_of(cx, *window, "viewer-message");
    assert!(message.starts_with("Cannot show this image"), "{message}");
    assert!(image_view(cx, view).is_none());
    assert!(bottom_keys(cx, *window).contains(&"3 Hex".to_owned()));
}
