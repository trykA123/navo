use std::any::Any;

use gpui_kit::{AnyView, FocusHandle, Focusable};

use super::{PdfView, Phase};
use crate::prelude::*;
use crate::window_view::viewer::body::{BodyKeys, ViewerBody};

pub(in crate::window_view::viewer) struct PdfBody(pub Entity<PdfView>);

impl PdfBody {
    #[cfg(test)]
    pub(in crate::window_view::viewer) fn of(body: &dyn ViewerBody) -> Option<&Self> {
        body.as_any().downcast_ref::<Self>()
    }
}

impl ViewerBody for PdfBody {
    fn view(&self) -> AnyView {
        self.0.clone().into()
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        let view = self.0.read(cx);
        match view.phase {
            Phase::Locked { .. } | Phase::Unlocking { .. } => {
                view.inputs.password.read(cx).focus_handle(cx)
            }
            _ => view.focus.clone(),
        }
    }

    fn keys(&self, cx: &App) -> BodyKeys {
        self.0.read(cx).keys()
    }

    fn find_again(&self, backward: bool, window: &mut Window, cx: &mut App) {
        self.0
            .update(cx, |view, cx| view.step(backward, window, cx));
    }

    fn set_note(&self, note: SharedString, cx: &mut App) {
        self.0.update(cx, |view, cx| view.set_note(note, cx));
    }

    fn is_typing(&self, cx: &App) -> bool {
        matches!(
            self.0.read(cx).phase,
            Phase::Locked { .. } | Phase::Unlocking { .. }
        )
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
