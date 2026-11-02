use std::any::Any;

use gpui_kit::{AnyView, FocusHandle};

use super::ArchiveView;
use crate::prelude::*;
use crate::window_view::viewer::body::{BodyKeys, Key, ViewerBody};

const LISTING: [Key; 2] = [("Enter", "Open"), ("Type", "Filter")];
const FILTERING: [Key; 3] = [
    ("Enter", "Open"),
    ("Backspace", "Erase"),
    ("Esc", "Clear filter"),
];
const BACK: Key = ("Esc", "Back to the list");

pub(in crate::window_view::viewer) struct ArchiveBody(pub Entity<ArchiveView>);

impl ViewerBody for ArchiveBody {
    fn view(&self) -> AnyView {
        self.0.clone().into()
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.0.read(cx).focus_target(cx)
    }

    fn keys(&self, cx: &App) -> BodyKeys {
        let archive = self.0.read(cx);
        if let Some(nested) = &archive.nested {
            return match nested.keys(cx) {
                BodyKeys::Modal(keys) => BodyKeys::Modal(keys),
                BodyKeys::Normal(mut keys) => {
                    keys.push(BACK);
                    BodyKeys::Modal(keys)
                }
            };
        }
        if archive.is_typing() {
            BodyKeys::Modal(FILTERING.to_vec())
        } else {
            BodyKeys::Normal(LISTING.to_vec())
        }
    }

    fn find_again(&self, backward: bool, window: &mut Window, cx: &mut App) {
        let nested = self
            .0
            .read(cx)
            .nested
            .as_ref()
            .map(|nested| nested.0.clone());
        if let Some(paged) = nested {
            paged.update(cx, |paged, cx| paged.find_again(backward, window, cx));
        }
    }

    fn set_note(&self, note: SharedString, cx: &mut App) {
        self.0.update(cx, |archive, cx| archive.set_note(note, cx));
    }

    fn is_typing(&self, cx: &App) -> bool {
        self.0.read(cx).is_typing()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
