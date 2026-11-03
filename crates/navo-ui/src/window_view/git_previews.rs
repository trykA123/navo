use futures::channel::oneshot;

use super::NavoWindow;
use super::git_menu::Cached;
use crate::git::Previewed;
use crate::prelude::*;

impl NavoWindow {
    /// Asks the hooks for the confirmations the open menu waits for; the reply arrives later.
    pub(super) fn request_git_previews(&mut self, cx: &mut Context<Self>) {
        let Some(hooks) = self.git_badges.hooks() else {
            return;
        };
        let Some(open) = self.git_menu.open.as_ref().filter(|o| o.parts.is_pending()) else {
            return;
        };
        let repo = open.repo.clone();
        self.git_menu.requests += 1;
        let request = self.git_menu.requests;
        let (sender, receiver) = oneshot::channel();
        hooks.previews(
            &repo,
            Box::new(move |replies| {
                let _ = sender.send(replies);
            }),
        );
        cx.spawn(async move |this, cx| {
            if let Ok(replies) = receiver.await {
                let _ = this.update(cx, |view, cx| view.git_previewed(request, replies, cx));
            }
        })
        .detach();
    }

    /// Applies a reply to the menu that asked for it; a reply for a closed or newer menu is dropped.
    fn git_previewed(&mut self, request: u64, replies: Previewed, cx: &mut Context<Self>) {
        if request != self.git_menu.requests || !self.git_menu.is_open(cx) {
            return;
        }
        let Some(open) = self.git_menu.open.as_mut() else {
            return;
        };
        open.model = open.parts.model(Some(&replies));
        let last = open.model.rows.len().saturating_sub(1);
        self.git_menu.cache = Some(Cached {
            repo: open.repo.clone(),
            replies,
        });
        self.git_menu.popup().update(cx, |popup, _| {
            popup.selected = popup.selected.min(last);
        });
        cx.notify();
    }
}
