use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};

use super::settings_sheet::SettingsHooks;
use super::{FerriteWindow, KeymapHooks};
use crate::prelude::*;
use crate::settings::{DefaultShift, Values};
use crate::status_bar::StatusNotice;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsNews {
    Applied { values: Values, generation: u64 },
    Invalid(String),
    Defaults(Vec<DefaultShift>),
}

#[derive(Clone, Debug)]
pub struct SettingsSender(UnboundedSender<SettingsNews>);

impl SettingsSender {
    pub fn send(&self, news: SettingsNews) -> bool {
        self.0.unbounded_send(news).is_ok()
    }
}

pub struct SettingsInbox(UnboundedReceiver<SettingsNews>);

impl SettingsHooks {
    #[must_use]
    pub fn channel(self) -> (Self, SettingsSender) {
        let (sender, receiver) = unbounded();
        let hooks = Self {
            news: Some(SettingsInbox(receiver)),
            ..self
        };
        (hooks, SettingsSender(sender))
    }
}

impl FerriteWindow {
    pub(super) fn listen_files(
        &mut self,
        keymap: KeymapHooks,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.listen_keymap(keymap, cx);
        let Some(SettingsInbox(mut receiver)) = self.settings.news.take() else {
            return;
        };
        cx.spawn_in(window, async move |this, cx| {
            while let Some(news) = receiver.next().await
                && this
                    .update_in(cx, |view, window, cx| view.take_settings(news, window, cx))
                    .is_ok()
            {}
        })
        .detach();
    }

    pub(super) fn take_settings(
        &mut self,
        news: SettingsNews,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match news {
            SettingsNews::Applied { values, generation } => {
                if self.saver.is_stale(generation) {
                    return;
                }
                self.saver.disk_valid();
                let fresh = self.saver.on_disk(values, generation);
                if fresh != self.values {
                    self.set_values(fresh, window, cx);
                    self.follow_defaults();
                    self.saver.count_disk_apply();
                }
            }
            SettingsNews::Invalid(message) => self.saver.disk_invalid(message.into()),
            SettingsNews::Defaults(shifts) => self.take_defaults(shifts),
        }
        cx.notify();
    }

    pub(super) fn status_notice(&self) -> Option<StatusNotice> {
        let problem = self.saver.problem.clone().map(|text| StatusNotice {
            text,
            is_error: true,
        });
        self.clipboard_notice()
            .or(problem)
            .or_else(|| self.keymap_notice())
            .or_else(|| self.upgrade_notice())
    }
}
