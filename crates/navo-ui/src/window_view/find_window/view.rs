//! Placeholder view for the floating find-results window.

use gpui_kit::base::TestSupportExt;
use gpui_kit::{FocusHandle, Subscription, WeakEntity};
use navo_l10n::t;

use super::super::NavoWindow;
use super::keys;
use super::query_row::QueryEdit;
use crate::find_results::book::FindBook;
use crate::prelude::*;
use crate::tokens;

/// Key context for the floating find-results window.
const FIND_RESULTS_CONTEXT: &str = "FindResults";

/// Root content view for the floating find-results window.
///
/// Both `NavoWindow` and `FindWindow` are opened via `gpui_kit::open_window`, which wraps the
/// view in `gpui_kit::base::Root`, so `navo_window` is stored as `AnyWindowHandle`.
pub(in crate::window_view) struct FindWindow {
    navo: WeakEntity<NavoWindow>,
    navo_window: AnyWindowHandle,
    book: Entity<FindBook>,
    focus: FocusHandle,
    pinned: bool,
    pub(super) edit: Option<QueryEdit>,
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
        let _book_watch = cx.observe(&book, |view, book, cx| {
            let active_id = book.read(cx).active().map(|tab| tab.id);
            if view
                .edit
                .as_ref()
                .is_some_and(|edit| Some(edit.tab_id()) != active_id)
            {
                view.edit = None;
            }
            cx.notify();
        });
        Self {
            navo,
            navo_window,
            book,
            focus: cx.focus_handle(),
            pinned,
            edit: None,
            _book_watch,
        }
    }

    /// Moves keyboard focus inside the results window to its main focus handle.
    pub(super) fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
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

    /// Activates the owning Navo window and focuses its active pane, ignoring a gone owner.
    fn return_to_navo(&self, cx: &mut Context<Self>) {
        let navo = self.navo.clone();
        let _ = cx.update_window(self.navo_window, |_, window, cx| {
            window.activate_window();
            let _ = navo.update(cx, |navo, cx| navo.focus_core_pane(window, cx));
        });
    }

    fn back_to_navo(&mut self, _: &keys::BackToNavo, _: &mut Window, cx: &mut Context<Self>) {
        self.return_to_navo(cx);
    }

    fn edit_query(&mut self, _: &keys::EditQuery, window: &mut Window, cx: &mut Context<Self>) {
        self.start_edit(window, cx);
    }

    fn run_again(&mut self, _: &keys::RunAgain, window: &mut Window, cx: &mut Context<Self>) {
        if !self.commit_edit(window, cx) {
            self.run_active(cx);
        }
    }

    fn on_toggle_case(&mut self, _: &keys::ToggleCase, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_case(cx);
    }

    fn on_toggle_regex(&mut self, _: &keys::ToggleRegex, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_regex(cx);
    }

    fn stop(&mut self, _: &keys::Stop, window: &mut Window, cx: &mut Context<Self>) {
        if self.cancel_edit(window, cx) {
            return;
        }
        if !self.stop_active(cx) {
            self.return_to_navo(cx);
        }
    }
}

impl Render for FindWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = tokens::current(cx).clone();
        let tabs = self.book.read(cx).len();
        div()
            .id("find-window")
            .key_context(FIND_RESULTS_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::back_to_navo))
            .on_action(cx.listener(Self::edit_query))
            .on_action(cx.listener(Self::run_again))
            .on_action(cx.listener(Self::on_toggle_case))
            .on_action(cx.listener(Self::on_toggle_regex))
            .on_action(cx.listener(Self::stop))
            .test_support()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.colors.background)
            .text_color(theme.colors.text)
            .font_family(theme.typography.ui_font.clone())
            .text_size(theme.typography.body)
            .child(self.title_bar(cx))
            .child(self.query_row(window, cx))
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
