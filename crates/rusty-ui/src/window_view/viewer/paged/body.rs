use std::any::Any;

use gpui_kit::{AnyView, FocusHandle, Focusable};

use super::{PagedKind, PagedView};
use crate::prelude::*;
use crate::window_view::viewer::Paged;
use crate::window_view::viewer::body::{BodyKeys, ViewerBody};

pub(in crate::window_view::viewer) struct PagedBody(pub Entity<PagedView>);

impl PagedBody {
    pub(in crate::window_view::viewer) fn new(
        title: SharedString,
        paged: Paged,
        kind: PagedKind,
        cx: &mut App,
    ) -> Self {
        Self(cx.new(|cx| PagedView::new(title, paged, kind, cx)))
    }

    pub(in crate::window_view::viewer) fn of(body: &dyn ViewerBody) -> Option<&Self> {
        body.as_any().downcast_ref::<Self>()
    }

    pub(in crate::window_view::viewer) fn show_as(&self, kind: PagedKind, cx: &mut App) {
        self.0.update(cx, |paged, cx| paged.set_kind(kind, cx));
    }
}

impl ViewerBody for PagedBody {
    fn view(&self) -> AnyView {
        self.0.clone().into()
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.0.read(cx).focus_handle(cx)
    }

    fn keys(&self, cx: &App) -> BodyKeys {
        let paged = self.0.read(cx);
        match paged.bar_keys() {
            Some(keys) => BodyKeys::Modal(keys.to_vec()),
            None => BodyKeys::Normal(vec![("Ctrl+F", "Find"), ("F3", "Next"), paged.go_to_key()]),
        }
    }

    fn find_again(&self, backward: bool, window: &mut Window, cx: &mut App) {
        self.0
            .update(cx, |paged, cx| paged.find_again(backward, window, cx));
    }

    fn set_note(&self, note: SharedString, cx: &mut App) {
        self.0.update(cx, |paged, cx| paged.set_note(note, cx));
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
