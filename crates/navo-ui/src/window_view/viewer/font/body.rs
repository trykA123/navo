use std::any::Any;

use gpui_kit::{AnyView, FocusHandle, Focusable};

use super::FontView;
use crate::prelude::*;
use crate::window_view::viewer::body::{BodyKeys, ViewerBody};

pub(in crate::window_view::viewer) struct FontBody(pub Entity<FontView>);

impl ViewerBody for FontBody {
    fn view(&self) -> AnyView {
        self.0.clone().into()
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.0.read(cx).focus_handle(cx)
    }

    fn keys(&self, _: &App) -> BodyKeys {
        BodyKeys::Normal(vec![("↑ ↓", "Scroll")])
    }

    fn find_again(&self, _: bool, _: &mut Window, _: &mut App) {}

    fn set_note(&self, note: SharedString, cx: &mut App) {
        self.0.update(cx, |font, cx| font.set_note(note, cx));
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
