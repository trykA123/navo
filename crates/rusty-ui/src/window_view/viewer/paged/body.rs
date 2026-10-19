use std::any::Any;

use gpui_kit::{AnyView, FocusHandle, Focusable};
use rusty_core::encoding::TextEncoding;

use super::{PagedKind, PagedView};
use crate::prelude::*;
use crate::settings::Values;
use crate::window_view::viewer::Paged;
use crate::window_view::viewer::body::{BodyAct, BodyKeys, Carried, Host, MenuEntry, ViewerBody};
use crate::window_view::viewer::menu::marking_entries;
use crate::window_view::viewer::parts::{PartSet, TEXT};
use crate::window_view::viewer::search::memory::MemoryLink;
use crate::window_view::viewer::{OpenGoTo, ShowHex, ToggleFollow};

pub(in crate::window_view::viewer) struct PagedBody(pub Entity<PagedView>);

impl PagedBody {
    pub(in crate::window_view::viewer) fn new(
        host: Host,
        paged: Paged,
        kind: PagedKind,
        cx: &mut App,
    ) -> Self {
        let link = MemoryLink {
            hooks: host.hooks,
            location: host.location,
            stamp: paged.stamp,
            carried: host.carried,
            crlf: None,
        };
        Self(cx.new(|cx| {
            let mut view = PagedView::new(host.title, paged, kind, cx);
            view.attach_memory(link, cx);
            view
        }))
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
            None => BodyKeys::Normal(paged.reading_keys()),
        }
    }

    fn find_again(&self, backward: bool, window: &mut Window, cx: &mut App) {
        self.0
            .update(cx, |paged, cx| paged.find_again(backward, window, cx));
    }

    fn menu(&self, _: bool, cx: &mut App) -> Vec<MenuEntry> {
        let is_text = self.0.read(cx).kind == PagedKind::Text;
        let mut entries = marking_entries();
        entries.push(MenuEntry::Break);
        if is_text {
            entries.push(MenuEntry::Run("Go to line", Box::new(OpenGoTo), false));
        }
        entries.push(MenuEntry::Run("Follow", Box::new(ToggleFollow), false));
        if is_text {
            entries.push(MenuEntry::Run("Open as Hex", Box::new(ShowHex), false));
        }
        entries
    }

    fn set_note(&self, note: SharedString, cx: &mut App) {
        self.0.update(cx, |paged, cx| paged.set_note(note, cx));
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn act(&self, act: BodyAct, _: &mut Window, cx: &mut App) {
        self.0.update(cx, |paged, cx| paged.act(act, cx));
    }

    fn go_to_line(&self, line: u64, _: &mut Window, cx: &mut App) {
        self.0.update(cx, |paged, cx| paged.go_to_line(line, cx));
    }

    fn parts(&self, cx: &App) -> Option<&'static PartSet> {
        (self.0.read(cx).kind == PagedKind::Text).then_some(&TEXT)
    }

    fn show_parts(&self, values: &Values, cx: &mut App) {
        self.0
            .update(cx, |paged, cx| paged.show_text_parts(values, cx));
    }

    fn encoding(&self, cx: &App) -> Option<TextEncoding> {
        let paged = self.0.read(cx);
        (paged.kind == PagedKind::Text).then(|| paged.source.layout().text)
    }

    fn carry(&self, cx: &App) -> Option<Carried> {
        let marking = self.0.read(cx).marking_now();
        Some(Carried {
            marks: marking.marks.set.clone(),
            bookmarks: Some(marking.bookmarks.clone()),
        })
    }
}
