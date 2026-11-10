//! ICO tests: the size check uses the entry the decoder selects and its embedded image.

use navo_ui::compare_diff::{ImageLimit, ImageScores, PairKind};

use crate::compare::diff::image::{oversize_decodes, reset_oversize_decodes};
use crate::compare::diff::pair::PairDiff;

use super::fixtures::{png_with_header_size, rgba_png};
use super::support::{ScriptSource, channel, image, named_input, opened, request};

/// One ICO directory entry: width and height bytes (0 means 256), bit depth and image data.
struct Entry {
    side: u8,
    bits: u16,
    data: Vec<u8>,
}

/// An ICO file holding `entries` in order, each image right after the directory.
fn ico(entries: &[Entry]) -> Vec<u8> {
    let count = u16::try_from(entries.len()).unwrap();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&count.to_le_bytes());
    let mut offset = 6 + 16 * u32::from(count);
    for entry in entries {
        let length = u32::try_from(entry.data.len()).unwrap();
        bytes.extend_from_slice(&[entry.side, entry.side, 0, 0]);
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&entry.bits.to_le_bytes());
        bytes.extend_from_slice(&length.to_le_bytes());
        bytes.extend_from_slice(&offset.to_le_bytes());
        offset += length;
    }
    for entry in entries {
        bytes.extend_from_slice(&entry.data);
    }
    bytes
}

/// Run one `left.ico` against `right.png` and return the image scores.
fn compare_with_png(id: u64, icon: Vec<u8>, png: Vec<u8>) -> ImageScores {
    let left = ScriptSource::new(icon, usize::MAX, 0);
    let right = ScriptSource::new(png, usize::MAX, 0);
    let (sink, receiver) = channel();
    let control = PairDiff::start(
        request(id),
        named_input(&left, &right, "left.ico", "right.png"),
        sink,
    );
    assert_eq!(opened(&receiver), PairKind::Image);
    let (scores, _) = image(&receiver, id);
    drop(control);
    scores
}

#[test]
fn an_ico_is_sized_by_the_entry_the_decoder_selects() {
    let icon = ico(&[
        Entry {
            side: 0,
            bits: 8,
            data: rgba_png(256, 256, 255),
        },
        Entry {
            side: 32,
            bits: 32,
            data: rgba_png(32, 32, 255),
        },
    ]);
    let scores = compare_with_png(60, icon, rgba_png(32, 32, 255));
    assert!(
        matches!(scores, ImageScores::Scored { ssim, .. } if ssim > 0.999),
        "the 32-bit 32x32 entry wins on bit depth, so no size mismatch: {scores:?}"
    );
}

#[test]
fn an_ico_entry_embedding_a_larger_png_is_too_large_without_a_decoder() {
    reset_oversize_decodes();
    let icon = ico(&[Entry {
        side: 0,
        bits: 32,
        data: png_with_header_size(9_000, 9_000),
    }]);
    let scores = compare_with_png(61, icon, rgba_png(32, 32, 255));
    assert_eq!(
        scores,
        ImageScores::TooLarge {
            reason: ImageLimit::Pixels,
            equal: false
        }
    );
    assert_eq!(
        oversize_decodes(),
        0,
        "no decoder for the embedded 81 MP PNG"
    );
}
