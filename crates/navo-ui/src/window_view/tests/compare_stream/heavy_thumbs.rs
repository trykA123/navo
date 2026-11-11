//! The Stream's GPU thumbnails when the item list changes: a resync that drops an image pair and
//! a new Stream over the old one must both give the textures back (D26.3).

use gpui_kit::TestAppContext;

use super::super::compare_screen::{Rig, rig};
use super::super::held::gpu_images;
use super::entry::row;
use super::heavy::image_detail;
use crate::browser::RowStatus;
use crate::compare_diff::PairKind;

impl Rig {
    /// Opens a Stream of two files whose first pair is an image with both thumbnails uploaded.
    fn open_with_image(&self, cx: &mut TestAppContext) {
        self.open_stream(cx, 2);
        self.send_heavy(cx, 1, PairKind::Image);
        self.send_detail(cx, 1, image_detail(2));
        assert_eq!(gpu_images(cx).items, 2, "both thumbs are on the GPU");
    }
}

#[gpui_kit::test]
fn compare_stream_a_resync_that_drops_an_image_item_frees_its_thumbnails(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_with_image(cx);
    // The compare now says `a.txt` is the same: a resync leaves the Stream with no worker update.
    rig.send_rows(cx, vec![row(1, "a.txt", RowStatus::Same)]);
    rig.frame(cx);
    assert_eq!(
        gpu_images(cx).items,
        0,
        "an item that left the list loses its thumbnails (D26.3)"
    );
}

#[gpui_kit::test]
fn compare_stream_a_new_stream_frees_the_old_one_thumbnails(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_with_image(cx);
    // Opening a Stream again replaces the old one, textures and all.
    rig.open_stream(cx, 2);
    rig.frame(cx);
    assert_eq!(
        gpu_images(cx).items,
        0,
        "the replaced Stream gave its thumbnails back (D26.3)"
    );
}
