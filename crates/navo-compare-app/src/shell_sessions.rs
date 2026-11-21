//! The session book wiring of the shell: records compares, reopens stored sessions, syncs the rows.
//! A child module of `shell_tabs`, so it reads the shell's private fields.

use std::path::Path;

use gpui_kit::Global;
use navo_compare_cli::args::SideSpec;
use navo_compare_profile::book::{SessionBook, Visit, now_ms};
use navo_compare_profile::recent::StartRows;
use navo_compare_profile::session::{SideKind, SideRef};
use sidewise_kit::prelude::*;

use super::{Shell, TabKind};
use crate::layers::toast;
use crate::screens::compare_tab::CompareRequest;
use crate::start_tabs::{StartChoice, StartMode};

/// The app's session book as a GPUI global; absent when the profile did not open and in tests that install none.
struct Sessions(SessionBook);
impl Global for Sessions {}

/// The stored rows, or no rows without a book.
pub(super) fn stored_rows(cx: &App) -> StartRows {
    cx.try_global::<Sessions>()
        .map(|sessions| sessions.0.rows().clone())
        .unwrap_or_default()
}

/// Unix seconds now.
fn now_s() -> i64 {
    i64::try_from(now_ms() / 1000).unwrap_or(i64::MAX)
}

/// Turns a start side into the side a session stores.
fn side_ref(spec: &SideSpec) -> SideRef {
    match spec {
        SideSpec::Path(path) => SideRef::path(path.clone()),
        SideSpec::GitRev { repo, rev } => SideRef::git(repo.clone(), rev.clone()),
        SideSpec::GitUrl { url, rev } => SideRef::remote(url.clone(), rev.clone()),
    }
}

/// Turns a stored side back into the start side that opens it.
fn side_spec(side: SideRef) -> SideSpec {
    match (side.rev, side.url) {
        (Some(rev), Some(url)) if side.kind == SideKind::Remote => SideSpec::GitUrl { url, rev },
        (Some(rev), _) if side.kind == SideKind::Git => SideSpec::GitRev {
            repo: side.path,
            rev,
        },
        _ => SideSpec::Path(side.path),
    }
}

impl Shell {
    /// Opens the session book (`explicit` folder, else the platform folders) as the app global; Err is the text to print.
    pub(crate) fn install_sessions(explicit: Option<&Path>, cx: &mut App) -> Result<(), String> {
        let book = SessionBook::open(explicit, now_s())?;
        cx.set_global(Sessions(book));
        Ok(())
    }

    /// Pushes the stored rows into the sidebar and into every start tab.
    pub(crate) fn sync_rows(&self, cx: &mut Context<Self>) {
        let rows = stored_rows(cx);
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.set_rows(rows.clone(), cx));
        let screens = self.tabs.iter().filter_map(|tab| match &tab.kind {
            TabKind::Start(screen) => Some(screen.clone()),
            TabKind::Compare(_) => None,
        });
        for screen in screens.collect::<Vec<_>>() {
            let rows = rows.clone();
            screen.update(cx, |screen, cx| {
                screen.rows = rows;
                screen.selected = None;
                cx.notify();
            });
        }
    }

    /// Deletes unnamed recent session `id`, refreshes the rows and toasts "Removed from Recent" with an Undo
    /// that saves it back. An error toasts its text and changes nothing.
    pub(in crate::shell) fn forget_row(&mut self, id: &str, cx: &mut Context<Self>) {
        if !cx.has_global::<Sessions>() {
            return;
        }
        let session = match cx.global_mut::<Sessions>().0.forget(id, now_s()) {
            Ok(session) => session,
            Err(text) => return self.toast(text, cx),
        };
        self.sync_rows(cx);
        let shell = cx.weak_entity();
        let undo = move |cx: &mut App| {
            let restored = cx.global_mut::<Sessions>().0.restore(&session, now_s());
            let _ = shell.update(cx, |shell, cx| match restored {
                Ok(()) => shell.sync_rows(cx),
                Err(text) => shell.toast(text, cx),
            });
        };
        toast::notify_undo("Removed from Recent", undo, cx);
    }

    /// Records the compare in the book (no book: nothing); a save error shows as a toast.
    /// The tab asks for it once its run opened both sides (`SidesOpened`).
    pub(super) fn remember(&mut self, request: &CompareRequest, cx: &mut Context<Self>) {
        if !cx.has_global::<Sessions>() {
            return;
        }
        let visit = Visit {
            left: side_ref(&request.left),
            right: side_ref(&request.right),
            respect_gitignore: request.choice.respect_gitignore,
        };
        if let Err(text) = cx.global_mut::<Sessions>().0.record(&visit, now_ms()) {
            self.toast(text, cx);
        }
        self.sync_rows(cx);
    }

    /// The index of a tab that already shows a compare of the same two sides and the same
    /// gitignore choice as `request`.
    fn compare_like(&self, request: &CompareRequest, cx: &App) -> Option<usize> {
        self.tabs.iter().position(|tab| match &tab.kind {
            TabKind::Compare(open) => {
                let open = open.read(cx).request();
                open.left == request.left
                    && open.right == request.right
                    && open.choice.respect_gitignore == request.choice.respect_gitignore
            }
            TabKind::Start(_) => false,
        })
    }

    /// Opens stored session `id` in the tab at `index`, or switches to the tab that already shows
    /// it; a bad session shows one toast and the rows refresh.
    pub(in crate::shell) fn reopen(
        &mut self,
        (index, id): (usize, &str),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !cx.has_global::<Sessions>() {
            return;
        }
        match cx.global::<Sessions>().0.load(id) {
            Ok(visit) => {
                let choice = StartChoice {
                    mode: StartMode::Folder,
                    respect_gitignore: visit.respect_gitignore,
                };
                let request = CompareRequest {
                    left: side_spec(visit.left),
                    right: side_spec(visit.right),
                    choice,
                };
                if let Some(open) = self.compare_like(&request, cx) {
                    return self.select_tab(open, window, cx);
                }
                let _ = self.open_compare((index, request), window, cx);
            }
            Err(text) => {
                self.toast(text, cx);
                let _ = cx.global_mut::<Sessions>().0.refresh(now_s());
                self.sync_rows(cx);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use navo_compare_cli::args::SideSpec;

    use super::{side_ref, side_spec};

    #[test]
    fn a_path_side_round_trips() {
        let spec = SideSpec::Path(PathBuf::from("/tmp/left"));
        assert_eq!(side_spec(side_ref(&spec)), spec);
    }

    #[test]
    fn a_git_side_round_trips() {
        let spec = SideSpec::GitRev {
            repo: PathBuf::from("/tmp/repo"),
            rev: "main".to_string(),
        };
        assert_eq!(side_spec(side_ref(&spec)), spec);
    }

    #[test]
    fn a_remote_side_round_trips() {
        let spec = SideSpec::GitUrl {
            url: "https://github.com/owner/repo".into(),
            rev: "main".into(),
        };
        assert_eq!(side_spec(side_ref(&spec)), spec);
    }
}
