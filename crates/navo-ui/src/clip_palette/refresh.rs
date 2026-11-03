//! Asking the hooks: a search per change, the counts of the other modes, and the answers.

use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, unbounded};
use gpui_kit::{AppContext as _, Context};

use super::notes::lead_with_folder;
use super::view::ClipPalette;
use super::{ClipMode, ClipResults};

/// Waits for a change and swallows the ones that piled up behind it; false when the hooks are gone.
async fn next_change(receiver: &mut UnboundedReceiver<()>) -> bool {
    if receiver.next().await.is_none() {
        return false;
    }
    while receiver.try_recv().is_ok() {}
    true
}

pub(super) fn system_now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

impl ClipPalette {
    pub(super) fn listen(&self, cx: &mut Context<Self>) {
        let (sender, mut receiver) = unbounded::<()>();
        self.hooks.subscribe(Box::new(move || {
            let _ = sender.unbounded_send(());
        }));
        cx.spawn(async move |this, cx| {
            while next_change(&mut receiver).await
                && this
                    .update(cx, |palette, cx| palette.refresh(true, cx))
                    .is_ok()
            {}
        })
        .detach();
    }

    /// Asks the hooks again. A refresh keeps the selected row; a new search starts at the top.
    pub(super) fn refresh(&mut self, is_refresh: bool, cx: &mut Context<Self>) {
        if self.dormant {
            return;
        }
        self.now_ms = self.fixed_now.unwrap_or_else(system_now);
        if self.manage.is_some() {
            return self.refresh_manage(is_refresh, cx);
        }
        let request = self.state.begin_request(is_refresh);
        let query = self.query(cx).to_string();
        let (hooks, folder) = (self.hooks.clone(), self.folder.clone());
        let (shown, modes) = (self.state.mode(), self.state.modes().to_vec());
        let search = {
            let (hooks, query, folder) = (hooks.clone(), query.clone(), folder.clone());
            cx.background_spawn(async move { hooks.search(&query, shown, folder.as_ref()) })
        };
        cx.spawn(async move |this, cx| {
            let results = search.await;
            let _ = this.update(cx, |palette, cx| palette.take_results(request, results, cx));
        })
        .detach();
        if modes.len() < 2 {
            return;
        }
        let tally = cx.background_spawn(async move {
            modes
                .into_iter()
                .map(|mode| (mode, hooks.search(&query, mode, folder.as_ref()).rows.len()))
                .collect::<Vec<_>>()
        });
        cx.spawn(async move |this, cx| {
            let counts = tally.await;
            let _ = this.update(cx, |palette, cx| palette.take_counts(request, counts, cx));
        })
        .detach();
    }

    fn take_counts(
        &mut self,
        request: u64,
        counts: Vec<(ClipMode, usize)>,
        cx: &mut Context<Self>,
    ) {
        if self.state.accept_counts(request, counts) {
            cx.notify();
        }
    }

    pub(super) fn take_results(
        &mut self,
        request: u64,
        mut results: ClipResults,
        cx: &mut Context<Self>,
    ) {
        if let Some(folder) = self
            .state
            .folder()
            .filter(|_| self.state.asked() == ClipMode::Notes)
        {
            let is_blank = self.query(cx).is_empty();
            lead_with_folder(&mut results.rows, folder, is_blank);
        }
        if self.state.accept(request, results) {
            self.reveal_selection();
            self.probe_folders(cx);
            cx.notify();
        }
    }
}
