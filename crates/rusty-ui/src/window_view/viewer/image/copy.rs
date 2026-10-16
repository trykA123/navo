use std::io::Cursor;

use gpui_kit::{ClipboardItem, Image, RenderImage};
use image::{ImageFormat, RgbaImage};

use super::ImageView;
use crate::prelude::*;

const NOT_COPIED: &str = "Cannot copy this image.";

struct Png {
    bytes: Vec<u8>,
    width: u32,
    height: u32,
}

fn png_of(pixels: &RenderImage) -> Option<Png> {
    let size = pixels.size(0);
    let width = u32::try_from(size.width.0).ok()?;
    let height = u32::try_from(size.height.0).ok()?;
    let mut rgba = pixels.as_bytes(0)?.to_vec();
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let image = RgbaImage::from_raw(width, height, rgba)?;
    let mut bytes = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .ok()?;
    Some(Png {
        bytes,
        width,
        height,
    })
}

impl ImageView {
    pub(super) fn copy_image(&mut self, cx: &mut Context<Self>) {
        let pixels = self.picture.pixels.clone();
        self.copying = Some(cx.spawn(async move |this, cx| {
            let png = cx.background_spawn(async move { png_of(&pixels) }).await;
            let _ = this.update(cx, |view, cx| {
                view.copying = None;
                view.copied(png, cx);
            });
        }));
    }

    fn copied(&mut self, png: Option<Png>, cx: &mut Context<Self>) {
        let Some(png) = png else {
            return self.set_note(NOT_COPIED.into(), cx);
        };
        let is_smaller = png.width < self.picture.width || png.height < self.picture.height;
        let note = if is_smaller {
            format!(
                "Copied the image at {} × {}, the size shown.",
                png.width, png.height
            )
        } else {
            "Copied the image.".to_owned()
        };
        let image = Image::from_bytes(gpui_kit::ImageFormat::Png, png.bytes);
        cx.write_to_clipboard(ClipboardItem::new_image(&image));
        self.set_note(note.into(), cx);
    }
}
