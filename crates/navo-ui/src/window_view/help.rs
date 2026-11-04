use super::NavoWindow;
use super::keys::{OpenKeyMap, OpenUserGuide};
use crate::prelude::*;
use crate::status_bar::StatusNotice;

impl NavoWindow {
    pub(super) fn help_on(root: Div, cx: &mut Context<Self>) -> Div {
        root.on_action(cx.listener(|view, _: &OpenKeyMap, window, cx| {
            view.open_key_map(window, cx);
        }))
        .on_action(cx.listener(|view, _: &OpenUserGuide, window, cx| {
            view.open_user_guide(window, cx);
        }))
    }

    pub(super) fn show_guide_hint(&mut self, cx: &mut Context<Self>) {
        let take = self.settings.take_guide_hint.clone();
        cx.spawn(async move |this, cx| {
            if !cx.background_spawn(async move { take() }).await {
                return;
            }
            let _ = this.update(cx, |view, cx| {
                let text = match crate::commands::active(cx).shown_in("help.user_guide", "global") {
                    Some(key) => navo_l10n::t!("guide-hint", key = key),
                    None => navo_l10n::t!("guide-hint-unbound"),
                };
                view.show_notice(StatusNotice {
                    text: text.into(),
                    is_error: false,
                });
                cx.notify();
            });
        })
        .detach();
    }
}
