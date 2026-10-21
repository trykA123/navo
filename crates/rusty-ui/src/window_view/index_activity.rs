use std::time::Duration;

use gpui_kit::{AsyncApp, WeakEntity};

use super::FerriteWindow;
use crate::command_center::{IndexMode, IndexOptions, Indexing};
use crate::prelude::*;

const POLL: Duration = Duration::from_millis(500);
const INDEX_ACTIVITY: u64 = 0;

impl FerriteWindow {
    pub(super) fn poll_index_activity(cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let mut previous = None;
            cx.background_executor().timer(POLL).await;
            while Self::refresh_index_activity(&this, cx, &mut previous).await {
                cx.background_executor().timer(POLL).await;
            }
        })
        .detach();
    }

    async fn refresh_index_activity(
        this: &WeakEntity<Self>,
        cx: &mut AsyncApp,
        previous: &mut Option<Indexing>,
    ) -> bool {
        let Ok(status) = this.read_with(cx, |view, _| {
            view.index_activity_enabled()
                .then(|| view.command_center.hooks.index.status.clone())
        }) else {
            return false;
        };
        let indexing = match status {
            Some(status) => cx.background_spawn(async move { status() }).await,
            None => None,
        };
        this.update(cx, |view, cx| {
            view.set_index_activity(indexing, previous, cx);
        })
        .is_ok()
    }

    fn set_index_activity(
        &mut self,
        indexing: Option<Indexing>,
        previous: &mut Option<Indexing>,
        cx: &mut Context<Self>,
    ) {
        let indexing = indexing.filter(|_| self.index_activity_enabled());
        if *previous == indexing {
            return;
        }
        match &indexing {
            Some(indexing) => {
                self.job_ui
                    .activity
                    .set_background(INDEX_ACTIVITY, indexing.text(), None);
            }
            None => self.job_ui.activity.end_background(INDEX_ACTIVITY),
        }
        *previous = indexing;
        self.refresh_activity(cx);
    }

    fn index_activity_enabled(&self) -> bool {
        IndexOptions::of(&self.values).mode != IndexMode::Off
    }
}
