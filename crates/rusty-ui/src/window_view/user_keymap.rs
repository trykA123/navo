use std::sync::Arc;

use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};

use rusty_core::Location;

use super::{FerriteWindow, rebind_app_keys};
use crate::commands::{Keymap, active};
use crate::prelude::*;
use crate::shortcuts::SaveKeymap;
use crate::status_bar::StatusNotice;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeymapNews {
    Applied {
        keymap: Keymap,
        warnings: Vec<String>,
    },
    Invalid(String),
}

#[derive(Clone, Debug)]
pub struct KeymapSender(UnboundedSender<KeymapNews>);

impl KeymapSender {
    pub fn send(&self, news: KeymapNews) -> bool {
        self.0.unbounded_send(news).is_ok()
    }
}

const NO_FILE: &str = "There is no keymap file in this session.";

pub struct KeymapHooks {
    pub start: Option<KeymapNews>,
    news: Option<UnboundedReceiver<KeymapNews>>,
    save: SaveKeymap,
    file: Option<Location>,
}

fn no_file() -> SaveKeymap {
    Arc::new(|_| Err(NO_FILE.to_owned()))
}

pub(super) struct KeymapState {
    notice: Option<StatusNotice>,
    pub(super) problem: Option<SharedString>,
    pub(super) save: SaveKeymap,
    pub(super) file: Option<Location>,
    #[cfg(test)]
    applies: usize,
}

impl Default for KeymapState {
    fn default() -> Self {
        Self {
            notice: None,
            problem: None,
            save: no_file(),
            file: None,
            #[cfg(test)]
            applies: 0,
        }
    }
}

impl KeymapHooks {
    #[must_use]
    pub fn none() -> Self {
        Self {
            start: None,
            news: None,
            save: no_file(),
            file: None,
        }
    }

    #[must_use]
    pub fn channel(start: Option<KeymapNews>) -> (Self, KeymapSender) {
        let (sender, receiver) = unbounded();
        let hooks = Self {
            start,
            news: Some(receiver),
            save: no_file(),
            file: None,
        };
        (hooks, KeymapSender(sender))
    }

    #[must_use]
    pub fn with_file(self, file: Option<Location>, save: SaveKeymap) -> Self {
        Self { save, file, ..self }
    }
}

impl FerriteWindow {
    pub(super) fn listen_keymap(&mut self, hooks: KeymapHooks, cx: &mut Context<Self>) {
        self.keymap.save = hooks.save;
        self.keymap.file = hooks.file;
        if let Some(news) = hooks.start {
            self.take_keymap(news, cx);
        }
        let Some(mut receiver) = hooks.news else {
            return;
        };
        cx.spawn(async move |this, cx| {
            while let Some(news) = receiver.next().await
                && this
                    .update(cx, |view, cx| view.take_keymap(news, cx))
                    .is_ok()
            {}
        })
        .detach();
    }

    pub(super) fn take_keymap(&mut self, news: KeymapNews, cx: &mut Context<Self>) {
        let (notice, problem) = match news {
            KeymapNews::Applied { keymap, warnings } => {
                self.apply_keymap(keymap, cx);
                let notice = (!warnings.is_empty()).then(|| StatusNotice {
                    text: warnings.join(" · ").into(),
                    is_error: false,
                });
                (notice, None)
            }
            KeymapNews::Invalid(message) => {
                let message = SharedString::from(message);
                let notice = StatusNotice {
                    text: message.clone(),
                    is_error: true,
                };
                (Some(notice), Some(message))
            }
        };
        self.keymap.notice = notice;
        self.keymap.problem.clone_from(&problem);
        self.tell_shortcuts(problem, cx);
        cx.notify();
    }

    fn apply_keymap(&mut self, keymap: Keymap, cx: &mut App) {
        if active(cx) == &keymap {
            return;
        }
        rebind_app_keys(keymap, cx);
        #[cfg(test)]
        {
            self.keymap.applies += 1;
        }
    }

    pub(super) fn keymap_notice(&self) -> Option<StatusNotice> {
        self.keymap.notice.clone()
    }
}

#[cfg(test)]
impl FerriteWindow {
    pub(super) fn keymap_applies(&self) -> usize {
        self.keymap.applies
    }
}
