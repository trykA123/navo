//! The compare tab's event handling: Folders and compare-screen events decide which view shows
//! and which pair opens (H3).

use gpui_kit::{Focusable as _, WeakEntity};
use sidewise_kit::prelude::*;
use sidewise_l10n::t;
use sidewise_ui::browser::FolderSession;
use sidewise_ui::compare_diff::{PairRequest, UndoStart};
use sidewise_ui::window_view::{CompareScreen, ScreenEvent};

use super::compare_run::RunKind;
use super::compare_tab::{
    ClearedViewed, CompareRequest, CompareTab, Rewrote, SidesOpened, TabView,
};
use super::folders_view::FoldersEvent;
use super::pair_requests::{neighbour, pair_request, stream_one};
use crate::layers::toast;

impl CompareTab {
    /// Subscribes to the Folders view and the compare screen; for two files, opens their pair
    /// at once. Needs the window, so the shell calls it right after the entity is made.
    pub fn attach(
        &mut self,
        request: &CompareRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let folders = cx.subscribe_in(&self.folders, window, |tab, _, event, window, cx| {
            tab.on_folders_event(event, window, cx);
        });
        let screen = cx.subscribe_in(&self.screen, window, |tab, _, event, window, cx| {
            tab.on_screen_event(event, window, cx);
        });
        let large = cx.subscribe_in(&self.large, window, |tab, _, event, window, cx| {
            tab.on_large_event(event, window, cx);
        });
        self.subscriptions.extend([folders, screen, large]);
        if self.run.kind() == RunKind::Files {
            let roots = self.run.roots();
            let path = super::compare_tab::path_of(&request.left)
                .file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
            let pair = PairRequest {
                id: 0,
                path,
                left: Some(roots[0].clone()),
                right: Some(roots[1].clone()),
            };
            self.show_pair(pair, window, cx);
        }
        self.attached = true;
        self.note_opened(cx);
    }

    /// Tells the shell once per run that both sides opened (a failed side never does).
    /// Waits for `attach`: before it nobody listens.
    pub(super) fn note_opened(&mut self, cx: &mut Context<Self>) {
        if self.attached && !self.opened_sent && self.failure.is_none() && self.run.opened() {
            self.opened_sent = true;
            cx.emit(SidesOpened);
        }
    }

    /// A Folders row opened: shows the pair of that row, when its row has arrived.
    fn on_folders_event(
        &mut self,
        event: &FoldersEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            FoldersEvent::OpenPair(id) => {
                let asked = self.run.session().and_then(|session| session.pair_of(*id));
                if let Some(asked) = asked {
                    self.show_pair(pair_request(asked), window, cx);
                }
            }
            FoldersEvent::OpenStream(id) => {
                let items = self
                    .run
                    .session()
                    .and_then(|session| stream_one(session, *id));
                if let Some(items) = items {
                    self.show_stream(items, window, cx);
                }
            }
        }
    }

    /// The compare screen closed, asked to walk or wrote a file; settings and resync have no
    /// place in the alpha.
    fn on_screen_event(
        &mut self,
        event: &ScreenEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ScreenEvent::Closed { reveal } => {
                self.view = TabView::Folders;
                if let Some(path) = reveal {
                    self.folders
                        .update(cx, |view, cx| view.select_path(path, cx));
                }
                self.folders.read(cx).focus_handle(cx).focus(window, cx);
                cx.notify();
            }
            ScreenEvent::WalkFile { from, step } => {
                let next = self
                    .run
                    .session()
                    .and_then(|session: &FolderSession| neighbour(session, (*from, *step)));
                if let Some(pair) = next {
                    self.show_pair(pair, window, cx);
                }
            }
            ScreenEvent::Wrote { text, job, reload } => {
                self.toast_write(text, job.as_deref(), cx);
                if !reload.is_empty() {
                    cx.emit(Rewrote(reload.clone()));
                }
            }
            ScreenEvent::ViewedCleared { text, cleared } => {
                toast::notify(text.clone(), cx);
                if *cleared {
                    cx.emit(ClearedViewed);
                }
            }
            ScreenEvent::OpenSettings(_) | ScreenEvent::Resync => {}
        }
    }

    /// Toasts a finished copy or undo. A copy's toast has Undo, which undoes copy `job` on this
    /// tab's screen only while it is the newest copy (NC148, NC166); otherwise it says why.
    fn toast_write(&self, text: &str, job: Option<&str>, cx: &mut Context<Self>) {
        let Some(job) = job.map(str::to_owned) else {
            toast::notify(text.to_owned(), cx);
            return;
        };
        let screen = self.screen.downgrade();
        let undo = move |cx: &mut App| undo_from_toast(&screen, &job, cx);
        toast::notify_undo(text.to_owned(), undo, cx);
    }

    /// Opens `request` in the file diff and shows it; the screen moves focus to the diff.
    fn show_pair(&mut self, request: PairRequest, window: &mut Window, cx: &mut Context<Self>) {
        let id = request.id;
        let opened = self.screen.update(cx, |screen, cx| {
            screen.show_pair((request, None), window, cx)
        });
        if opened {
            self.view = TabView::File;
            self.shown = Some(id);
            cx.notify();
        }
    }
}

/// The toast's Undo: undoes copy `job` on `screen` while it is the newest copy; otherwise
/// toasts why not (tab closed, a newer copy exists, or it is already undone). `Nothing` cannot
/// come back here: with a job id the host answers `Gone` for an empty stack.
fn undo_from_toast(screen: &WeakEntity<CompareScreen>, job: &str, cx: &mut App) {
    let started = screen.update(cx, |screen, cx| screen.undo_write(job, cx));
    let id = match started {
        Err(_) => "compare-undo-tab-closed",
        Ok(UndoStart::NotNewest) => "compare-undo-not-newest",
        Ok(UndoStart::Gone) => "compare-undo-gone",
        Ok(UndoStart::Started | UndoStart::Nothing) => return,
    };
    // The toast host is mid-update while it runs this Undo, so the reply waits a turn.
    cx.defer(move |cx| {
        toast::notify(t!(id), cx);
    });
}
