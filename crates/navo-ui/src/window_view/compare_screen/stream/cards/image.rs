//! The image card's body and status words: both thumbnails in the card's fixed box, with the
//! score the pair got, or the note that says why there is none (D26.5, D26.6).

use std::sync::Arc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{ObjectFit, RenderImage, StyledImage, img};
use navo_l10n::{t, tr};

use super::DetailCtx;
use crate::compare_diff::card::LINE_HEIGHT;
use crate::compare_diff::{CARD_HUNKS, ImageCard};
use crate::prelude::*;

/// The image card's body: a box `CARD_HUNKS × LINE_HEIGHT` px tall with one half per side, each
/// showing its texture whole (`ObjectFit::Contain`). A side with no uploaded texture draws
/// nothing and the card surface of the frame shows through (D26.5).
pub(super) fn image_body(ctx: &DetailCtx<'_>, id: u64) -> Div {
    let (left, right) = ctx.thumbs.pair(id);
    div()
        .flex()
        .w_full()
        .flex_none()
        .h(px(CARD_HUNKS as f32 * LINE_HEIGHT))
        .child(half(left, 0))
        .child(half(right, 1))
}

/// One thumbnail half, found in tests as `("compare-card-thumb", at)` inside the card's row.
fn half(image: Option<&Arc<RenderImage>>, at: usize) -> AnyElement {
    div()
        .id(("compare-card-thumb", at))
        .test_support()
        .flex()
        .flex_1()
        .min_w_0()
        .items_center()
        .justify_center()
        .when_some(image.cloned(), |side, image| {
            side.child(img(image).size_full().object_fit(ObjectFit::Contain))
        })
        .into_any_element()
}

/// What an image card says on its status line: its score, the note that stands in for a score,
/// or that the job has not answered yet. The card always shows the score when it has one, whatever
/// the View menu picked (D26.6).
pub(super) fn image_status(card: &ImageCard) -> String {
    match (card.similarity, card.note) {
        (Some(percent), _) => t!("compare-image-similar", percent = percent),
        (None, Some(note)) => tr(note),
        (None, None) => tr("compare-screen-comparing"),
    }
}
