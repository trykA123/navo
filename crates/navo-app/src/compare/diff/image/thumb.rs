//! Image card details: the pair's score or note plus both bounded BGRA thumbnails (D26.5, D26.6).

use std::sync::Arc;

use ::image::DynamicImage;
use navo_compare::model::CancelToken;
use navo_ui::compare_diff::image_body::{notice_of, similarity_of};
use navo_ui::compare_diff::{CardDetail, ImageCard, ImageScores, THUMB_SIDE, Thumb};

use super::header;
use super::{Failure, Prepared, prepare};
use crate::compare::diff::load::LoadInput;

/// Build the image card detail: score, note and both thumbnails, or the failure text.
pub(crate) fn card(input: &LoadInput, cancel: &CancelToken) -> Option<CardDetail> {
    if cancel.is_cancelled() {
        return None;
    }
    match prepare(input, cancel) {
        Ok(Prepared::Fallback(scores)) => Some(image_card(scores, None, None)),
        Ok(Prepared::Mismatch {
            scores,
            left,
            right,
        }) => {
            let left = decode_thumb(&left);
            if cancel.is_cancelled() {
                return None;
            }
            let right = decode_thumb(&right);
            Some(image_card(scores, left, right))
        }
        Ok(Prepared::Decoded {
            first,
            second,
            left_bytes,
            right_bytes,
        }) => match compare_scores(&first, &second, &left_bytes, &right_bytes) {
            Ok(scores) => Some(image_card(
                scores,
                Some(thumb(&first)),
                Some(thumb(&second)),
            )),
            Err(_) => Some(failed()),
        },
        Err(Failure::Cancelled) => None,
        Err(Failure::Unsupported) => Some(image_card(
            ImageScores::Unsupported {
                format: header::unsupported_label(input),
            },
            None,
            None,
        )),
        Err(Failure::TooLarge(reason, equal)) => Some(image_card(
            ImageScores::TooLarge { reason, equal },
            None,
            None,
        )),
        Err(Failure::Read | Failure::Decode | Failure::Compare) => Some(failed()),
    }
}

/// Score a decoded pair as the file screen does; a mask is not needed for a card.
fn compare_scores(
    first: &DynamicImage,
    second: &DynamicImage,
    left_bytes: &[u8],
    right_bytes: &[u8],
) -> Result<ImageScores, Failure> {
    let outcome = navo_compare::image::compare(first, second).map_err(|_| Failure::Compare)?;
    Ok(super::convert(outcome, left_bytes, right_bytes).0)
}

/// The card's score, note and thumbnails; fallbacks carry no thumbnails.
fn image_card(scores: ImageScores, left: Option<Thumb>, right: Option<Thumb>) -> CardDetail {
    CardDetail::Image(ImageCard {
        similarity: similarity_of(&scores),
        note: notice_of(&scores),
        left,
        right,
    })
}

/// Decode one mismatch side for its thumbnail; a side the decoder rejects gets none.
fn decode_thumb((bytes, format): &(Vec<u8>, ::image::ImageFormat)) -> Option<Thumb> {
    header::decode(bytes, *format)
        .ok()
        .map(|image| thumb(&image))
}

/// One thumbnail: at most `THUMB_SIDE` per side, as BGRA bytes in a shared slice. An image that
/// already fits keeps its own size, because `thumbnail` would scale it up and blur it.
fn thumb(image: &DynamicImage) -> Thumb {
    let fits = image.width() <= THUMB_SIDE && image.height() <= THUMB_SIDE;
    let mut rgba = if fits {
        image.to_rgba8()
    } else {
        image.thumbnail(THUMB_SIDE, THUMB_SIDE).to_rgba8()
    };
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Thumb {
        width: rgba.width(),
        height: rgba.height(),
        bgra: Arc::from(rgba.into_raw()),
    }
}

/// The translated text for a pair the image worker could not start.
fn failed() -> CardDetail {
    CardDetail::Failed(navo_l10n::tr("compare-could-not-start"))
}
