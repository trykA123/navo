//! Image pair diff: one bounded snapshot per side, then scores on the pair worker (V37).
//!
//! A side over 64 MiB encoded or 64 MP is hashed, never decoded; dropping the pair cancels.

use std::io::{ErrorKind, Read};

use navo_compare::image::{ImageOutcome, ImageSize as CompareSize};
use navo_compare::model::{CancelToken, ReadHint};
use navo_ui::compare_diff::{
    DiffSink, DiffUpdate, ImageAlpha, ImageLimit, ImageMask, ImageScores, ImageSize,
};

use super::load::{LoadInput, Side};

mod header;
mod ico;
#[cfg(test)]
pub(crate) mod probe;

pub(crate) use self::header::too_large;
#[cfg(test)]
pub(crate) use self::header::{oversize_decodes, reset_oversize_decodes};

/// The largest encoded side that is decoded, in bytes read; a larger one is hashed only.
const MAX_READ: usize = 64 * 1024 * 1024;
/// Read in 1 MiB chunks so cancel is checked often.
const CHUNK: usize = 1024 * 1024;

/// Why an image pair could not be scored. `Cancelled` means the pair was dropped.
enum Failure {
    Cancelled,
    Unsupported,
    TooLarge(ImageLimit, bool),
    Read,
    Decode,
    Compare,
}

/// One side's snapshot and content hash; `bytes` is absent when it was too large.
struct Snapshot {
    bytes: Option<Vec<u8>>,
    hash: [u8; 32],
    oversize: bool,
}

/// Compare an opened image pair and report `Image`, or `Failed` when a side cannot be read.
/// Returns false when the pair was closed and no update must be sent.
pub(crate) fn diff(input: &LoadInput, request: u64, sink: &DiffSink, cancel: &CancelToken) -> bool {
    let result = compare(input, cancel);
    if cancel.is_cancelled() {
        return false;
    }
    match result {
        Ok((scores, mask)) => sink(DiffUpdate::Image {
            request,
            scores,
            mask,
        }),
        Err(Failure::Cancelled) => return false,
        Err(Failure::Unsupported) => sink(DiffUpdate::Image {
            request,
            scores: ImageScores::Unsupported {
                format: header::unsupported_label(input),
            },
            mask: None,
        }),
        Err(Failure::TooLarge(reason, equal)) => sink(DiffUpdate::Image {
            request,
            scores: ImageScores::TooLarge { reason, equal },
            mask: None,
        }),
        Err(Failure::Read | Failure::Decode | Failure::Compare) => sink(DiffUpdate::Failed {
            request,
            text: navo_l10n::tr("compare-could-not-start"),
        }),
    }
    true
}

/// Snapshot and hash both sides once, check dimensions, then decode the snapshots.
fn compare(
    input: &LoadInput,
    cancel: &CancelToken,
) -> Result<(ImageScores, Option<ImageMask>), Failure> {
    let left = input.left.as_ref().ok_or(Failure::Read)?;
    let right = input.right.as_ref().ok_or(Failure::Read)?;
    let left_snapshot = read_snapshot(left, cancel)?;
    let right_snapshot = read_snapshot(right, cancel)?;
    let equal = left_snapshot.hash == right_snapshot.hash;
    if left_snapshot.oversize || right_snapshot.oversize {
        return Ok((
            ImageScores::TooLarge {
                reason: ImageLimit::Encoded,
                equal,
            },
            None,
        ));
    }
    let left_bytes = left_snapshot.bytes.ok_or(Failure::Read)?;
    let right_bytes = right_snapshot.bytes.ok_or(Failure::Read)?;
    let left_format = header::format_of_snapshot(&left_bytes, left)?;
    let right_format = header::format_of_snapshot(&right_bytes, right)?;
    let formats = (left_format, right_format);
    if let Some(scores) = size_fallback(&left_bytes, &right_bytes, formats, equal)? {
        return Ok((scores, None));
    }
    let first = match header::decode(&left_bytes, left_format) {
        Ok(image) => image,
        Err(Failure::TooLarge(reason, _)) => {
            return Ok((ImageScores::TooLarge { reason, equal }, None));
        }
        Err(other) => return Err(other),
    };
    #[cfg(test)]
    probe::left_decoded(left, cancel);
    if cancel.is_cancelled() {
        return Err(Failure::Cancelled);
    }
    #[cfg(test)]
    probe::right_decoding(left);
    let second = header::decode(&right_bytes, right_format)?;
    if cancel.is_cancelled() {
        return Err(Failure::Cancelled);
    }
    let outcome = navo_compare::image::compare(&first, &second).map_err(|_| Failure::Compare)?;
    Ok(convert(outcome, &left_bytes, &right_bytes))
}

/// The fallback for a pair over a pixel cap or of different sizes, read from the headers the
/// decoders will use; `None` when the pair can be decoded and scored.
fn size_fallback(
    left_bytes: &[u8],
    right_bytes: &[u8],
    (left_format, right_format): (::image::ImageFormat, ::image::ImageFormat),
    equal: bool,
) -> Result<Option<ImageScores>, Failure> {
    let left = header::dimensions(left_bytes, left_format)?;
    let right = header::dimensions(right_bytes, right_format)?;
    if let Some(reason) = size_limit(left, right, left_format, right_format) {
        return Ok(Some(ImageScores::TooLarge { reason, equal }));
    }
    Ok((left != right).then_some(ImageScores::SizeMismatch { left, right }))
}

/// The pixel cap that applies to a same-format pair, if any.
fn size_limit(
    left: ImageSize,
    right: ImageSize,
    left_format: ::image::ImageFormat,
    right_format: ::image::ImageFormat,
) -> Option<ImageLimit> {
    if too_large(left)
        || too_large(right)
        || header::webp_over_cap(left_format, left)
        || header::webp_over_cap(right_format, right)
    {
        Some(ImageLimit::Pixels)
    } else {
        None
    }
}

/// Read and hash one side in 1 MiB chunks, keeping bytes only up to 64 MiB read; the listed
/// size is only a capacity hint, since the file may have changed since listing.
fn read_snapshot((source, entry): &Side, cancel: &CancelToken) -> Result<Snapshot, Failure> {
    if cancel.is_cancelled() {
        return Err(Failure::Cancelled);
    }
    let mut oversize = false;
    let mut bytes = Vec::with_capacity(capacity(entry.size));
    let mut reader = source
        .open(&entry.rel, ReadHint::Buffered)
        .map_err(|_| Failure::Read)?;
    let mut hasher = blake3::Hasher::new();
    let mut chunk = vec![0u8; CHUNK];
    loop {
        if cancel.is_cancelled() {
            return Err(Failure::Cancelled);
        }
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(count) => {
                hasher.update(&chunk[..count]);
                keep(&mut bytes, &mut oversize, &chunk[..count]);
            }
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(_) => return Err(Failure::Read),
        }
    }
    Ok(Snapshot {
        bytes: (!oversize).then_some(bytes),
        hash: *hasher.finalize().as_bytes(),
        oversize,
    })
}

/// Append a chunk while the snapshot is under the byte cap; clear it on overflow.
fn keep(bytes: &mut Vec<u8>, oversize: &mut bool, chunk: &[u8]) {
    if *oversize {
        return;
    }
    if bytes.len().saturating_add(chunk.len()) > MAX_READ {
        *oversize = true;
        bytes.clear();
    } else {
        bytes.extend_from_slice(chunk);
    }
}

/// A capacity hint from the listed size, never over the read limit.
fn capacity(size: u64) -> usize {
    usize::try_from(size).unwrap_or(usize::MAX).min(MAX_READ)
}

/// Turn the engine's outcome into UI scores and a mask; fallbacks carry no mask.
fn convert(
    outcome: ImageOutcome,
    left_bytes: &[u8],
    right_bytes: &[u8],
) -> (ImageScores, Option<ImageMask>) {
    match outcome {
        ImageOutcome::Scored { scores, diff } => {
            let alpha = scores.alpha.map(|alpha| ImageAlpha {
                max_delta: alpha.max_delta,
                mean_delta: alpha.mean_delta,
                differs: alpha.differs,
            });
            let mask = diff.to_luma8();
            let mask = ImageMask {
                width: mask.width(),
                height: mask.height(),
                pixels: mask.into_raw(),
            };
            let scores = ImageScores::Scored {
                ssim: scores.ssim,
                rgb_min_rms_similarity: scores.rgb_min_rms_similarity,
                luma_hellinger_similarity: scores.luma_hellinger_similarity,
                alpha,
            };
            (scores, Some(mask))
        }
        ImageOutcome::HashOnly => {
            let equal = blake3::hash(left_bytes) == blake3::hash(right_bytes);
            (
                ImageScores::TooLarge {
                    reason: ImageLimit::Pixels,
                    equal,
                },
                None,
            )
        }
        ImageOutcome::SizeMismatch { first, second } => (
            ImageScores::SizeMismatch {
                left: ui_size(first),
                right: ui_size(second),
            },
            None,
        ),
    }
}

/// Convert the engine's dimensions to the UI type.
fn ui_size(size: CompareSize) -> ImageSize {
    ImageSize {
        width: size.width,
        height: size.height,
    }
}
