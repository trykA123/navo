//! Keys inside the compare tab (H4): digits switch between its views, Escape steps back, F5
//! compares again.
//! A child of `screens`; the shell binds them through `shell_keys::bind_keys`.

use gpui_kit::{App, KeyBinding};
use sidewise_kit::prelude::*;
use sidewise_ui::compare_diff::{PairRequest, StreamList};

use super::compare_tab::{CONTEXT, CompareTab, OWN, TabView, compare_hooks};
use super::pair_requests::stream_request;

gpui_kit::actions!(
    compare_tab,
    [OpenFolders, OpenFile, OpenStream, OpenLarge, Back, Refresh]
);

/// Binds the tab keys. The digits bind to `OWN`, set only while Folders or Large show, so File and
/// Stream keep their digits. `1` (new tab) binds in `shell_keys`, next to `NewTab`; Escape binds in `CompareTab`.
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("2", OpenFolders, Some(OWN)),
        KeyBinding::new("3", OpenFile, Some(OWN)),
        KeyBinding::new("4", OpenStream, Some(OWN)),
        KeyBinding::new("5", OpenLarge, Some(OWN)),
        KeyBinding::new("f5", Refresh, Some(OWN)),
        KeyBinding::new("escape", Back, Some(CONTEXT)),
    ]);
}

impl CompareTab {
    /// Adds the tab's key handlers to its root element.
    pub(super) fn with_keys(&self, root: Div, cx: &mut Context<Self>) -> Div {
        root.on_action(cx.listener(|tab, _: &OpenFolders, window, cx| tab.open_folders(window, cx)))
            .on_action(cx.listener(|tab, _: &OpenFile, window, cx| tab.open_file(window, cx)))
            .on_action(cx.listener(|tab, _: &OpenStream, window, cx| tab.open_stream(window, cx)))
            .on_action(cx.listener(|tab, _: &OpenLarge, window, cx| tab.open_large(window, cx)))
            .on_action(cx.listener(|tab, _: &Back, window, cx| tab.back(window, cx)))
            .on_action(cx.listener(|tab, _: &Refresh, window, cx| tab.refresh(window, cx)))
    }

    /// `2`: shows the Folders view.
    pub(super) fn open_folders(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.auto_large = false;
        self.view = TabView::Folders;
        self.focus_view(window, cx);
        cx.notify();
    }

    /// `3`: shows the open file diff; does nothing while no pair is open.
    fn open_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.screen.read(cx).is_file_open() {
            return;
        }
        self.view = TabView::File;
        self.focus_view(window, cx);
        cx.notify();
    }

    /// `4`: opens the Stream over the session's differences; does nothing before the session
    /// exists or when the screen has no diff hooks.
    fn open_stream(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.run.session() else {
            return;
        };
        self.show_stream(stream_request(session), window, cx);
    }

    /// Opens `items` in the Stream and shows it; does nothing when the screen has no diff hooks.
    pub(super) fn show_stream(
        &mut self,
        items: (StreamList, Vec<PairRequest>),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let opened = self
            .screen
            .update(cx, |screen, cx| screen.open_stream(items, window, cx));
        if opened {
            self.view = TabView::Stream;
            cx.notify();
        }
    }

    /// `5`: shows the Large view. A user choice, so it stays when the session finishes.
    fn open_large(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.auto_large = false;
        self.view = TabView::Large;
        self.focus_view(window, cx);
        cx.notify();
    }

    /// F5: compares again and reads every file again (Recheck all), so a Git side shows its
    /// branch's newest commit. The Folders rows are
    /// cleared at once (old row ids mean nothing to the new session); a restart starts at the
    /// root with no filter, as intended. Clears a failure, so a fixed side can be retried.
    pub(crate) fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(hooks) = compare_hooks(cx) else {
            return;
        };
        self.run.restart(&*hooks, true);
        self.folders
            .update(cx, |view, cx| view.set_rows(vec![], cx));
        self.sync_large(cx);
        if self.failure.take().is_some() {
            self.view = TabView::Folders;
            self.focus_view(window, cx);
        }
        cx.notify();
    }

    /// Escape: Large goes back to Folders. Folders passes it on, so the shell can dismiss a tip.
    /// File and Stream own Escape inside the screen (`ScreenEvent::Closed`); a stray one passes on.
    fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.view == TabView::Large {
            self.open_folders(window, cx);
        } else {
            cx.propagate();
        }
    }
}
