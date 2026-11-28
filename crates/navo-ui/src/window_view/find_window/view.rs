//! Placeholder view for the floating find-results window.

use gpui_kit::base::TestSupportExt;
use gpui_kit::{Subscription, WeakEntity};
use navo_l10n::t;

use super::super::NavoWindow;
use crate::find_results::book::FindBook;
use crate::prelude::*;
use crate::tokens;

/// Root content view for the floating find-results window.
///
/// Both `NavoWindow` and `FindWindow` are opened via `gpui_kit::open_window`, which wraps the
/// view in `gpui_kit::base::Root`, so `navo_window` is stored as `AnyWindowHandle`.
pub(in crate::window_view) struct FindWindow {
    navo: WeakEntity<NavoWindow>,
    navo_window: AnyWindowHandle,
    book: Entity<FindBook>,
    pinned: bool,
    _book_watch: Subscription,
}

impl FindWindow {
    /// Builds the find-results window view and subscribes to changes in `book`.
    pub(super) fn new(
        navo: WeakEntity<NavoWindow>,
        navo_window: AnyWindowHandle,
        book: Entity<FindBook>,
        pinned: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let _book_watch = cx.observe(&book, |_, _, cx| cx.notify());
        Self {
            navo,
            navo_window,
            book,
            pinned,
            _book_watch,
        }
    }

    /// Returns the weak handle to the owning `NavoWindow`.
    pub(super) fn navo(&self) -> &WeakEntity<NavoWindow> {
        &self.navo
    }

    /// Returns the owning `NavoWindow`'s window handle.
    pub(super) fn navo_window(&self) -> AnyWindowHandle {
        self.navo_window
    }

    /// Returns the shared find-results book entity.
    pub(super) fn book(&self) -> &Entity<FindBook> {
        &self.book
    }

    /// Returns whether this window is currently pinned above other windows.
    pub(super) fn is_pinned(&self) -> bool {
        self.pinned
    }

    /// Updates the mirrored pin state and schedules a repaint.
    pub(super) fn set_pinned(&mut self, pinned: bool, cx: &mut Context<Self>) {
        self.pinned = pinned;
        cx.notify();
    }
}

impl Render for FindWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = tokens::current(cx);
        let tabs = self.book.read(cx).len();
        div()
            .id("find-window")
            .test_support()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.colors.background)
            .text_color(theme.colors.text)
            .font_family(theme.typography.ui_font.clone())
            .text_size(theme.typography.body)
            .child(self.title_bar(cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .gap(theme.spacing.sm)
                    .p(theme.spacing.md)
                    .child(
                        div()
                            .id("find-window-title")
                            .test_support()
                            .child(t!("find-title-empty")),
                    )
                    .child(
                        div()
                            .id("find-window-tabs")
                            .test_support()
                            .child(tabs.to_string()),
                    ),
            )
    }
}
