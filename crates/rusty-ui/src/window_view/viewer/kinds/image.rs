use super::{Load, Loaded, Slot, Sniff, ViewerKind};
use crate::prelude::*;
use crate::window_view::viewer::body::{Attach, ViewerBody};
use crate::window_view::viewer::image::{IMAGE_LIMIT, ImageBody, ImageView, Picture, decode_image};

pub(in crate::window_view::viewer) const IMAGE_ID: &str = "image";
const MAGIC: u8 = 90;
const NAMED: u8 = 80;
const SVG_INSIDE: u8 = 40;
const RASTER: [&str; 12] = [
    "png", "apng", "jpg", "jpeg", "jpe", "jfif", "gif", "webp", "bmp", "ico", "tif", "tiff",
];
const VECTOR: [&str; 2] = ["svg", "svgz"];

fn extension(name: &str) -> Option<String> {
    let (_, extension) = name.rsplit_once('.')?;
    Some(extension.to_ascii_lowercase())
}

fn named(name: &str, list: &[&str]) -> bool {
    extension(name).is_some_and(|extension| list.contains(&extension.as_str()))
}

pub(in crate::window_view::viewer) fn is_image_name(name: &str) -> bool {
    named(name, &RASTER) || named(name, &VECTOR)
}

fn is_icon(head: &[u8]) -> bool {
    head.starts_with(&[0, 0, 1, 0]) && head.get(4..6).is_some_and(|count| count != [0, 0])
}

fn has_magic(sniff: &Sniff<'_>) -> bool {
    let head = sniff.head;
    head.starts_with(b"\x89PNG\r\n\x1a\n")
        || head.starts_with(&[0xFF, 0xD8, 0xFF])
        || head.starts_with(b"GIF87a")
        || head.starts_with(b"GIF89a")
        || (head.starts_with(b"RIFF") && head.get(8..12) == Some(b"WEBP"))
        || head.starts_with(b"II*\0")
        || head.starts_with(b"MM\0*")
        || is_icon(head)
        || (head.starts_with(b"BM") && sniff.looks_binary())
}

fn has_svg_tag(head: &[u8]) -> bool {
    head.windows(4).any(|window| window == b"<svg")
}

pub(super) struct Image;

impl ViewerKind for Image {
    fn id(&self) -> &'static str {
        IMAGE_ID
    }

    fn name(&self) -> &'static str {
        "Image"
    }

    fn slot(&self) -> Slot {
        Slot::Rich
    }

    fn score(&self, sniff: &Sniff<'_>) -> u8 {
        if has_magic(sniff) || named(sniff.name, &VECTOR) {
            MAGIC
        } else if named(sniff.name, &RASTER) && sniff.looks_binary() {
            NAMED
        } else if has_svg_tag(sniff.head) {
            SVG_INSIDE
        } else {
            0
        }
    }

    fn load(&self, load: Load<'_>) -> Result<Loaded, String> {
        let bytes = (load.hooks.bytes)(load.location, IMAGE_LIMIT)?;
        Ok(Box::new(decode_image(&bytes, load.sniff.name)?))
    }

    fn attach(
        &self,
        attach: Attach,
        _: &mut Window,
        cx: &mut App,
    ) -> Result<Box<dyn ViewerBody>, String> {
        let picture = attach
            .loaded
            .downcast::<Picture>()
            .map_err(|_| "The image view got data it cannot show.".to_owned())?;
        let title = attach.host.title;
        let view = cx.new(|cx| ImageView::new(title, *picture, cx));
        Ok(Box::new(ImageBody(view)))
    }
}
