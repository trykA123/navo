use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};

use super::FerriteWindow;
use crate::commands::Keymap;
use crate::prelude::*;
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

pub struct KeymapHooks {
    pub start: Option<KeymapNews>,
    news: Option<UnboundedReceiver<KeymapNews>>,
}

impl KeymapHooks {
    #[must_use]
    pub fn none() -> Self {
        Self {
            start: None,
            news: None,
        }
    }

    #[must_use]
    pub fn channel(start: Option<KeymapNews>) -> (Self, KeymapSender) {
        let (sender, receiver) = unbounded();
        let hooks = Self {
            start,
            news: Some(receiver),
        };
        (hooks, KeymapSender(sender))
    }
}

impl FerriteWindow {
    pub(super) fn listen_keymap(_hooks: KeymapHooks, _cx: &mut Context<Self>) {}

    pub(super) fn keymap_notice(&self) -> Option<StatusNotice> {
        None
    }
}
