use std::sync::Arc;

use super::FerriteWindow;
use crate::app_menu::{QuickChange, QuickSettings, Setting};
use crate::prelude::*;

pub type SaveSettings = Arc<dyn Fn(&QuickChange) -> Result<QuickSettings, String> + Send + Sync>;

pub(super) struct Saver {
    save: SaveSettings,
    pending: Option<QuickChange>,
    is_saving: bool,
    pub(super) error: Option<SharedString>,
}

impl Saver {
    pub(super) fn new(save: SaveSettings, error: Option<SharedString>) -> Self {
        Self {
            save,
            pending: None,
            is_saving: false,
            error,
        }
    }
}

impl FerriteWindow {
    pub(super) fn set_quick(&mut self, values: QuickSettings, window: &Window, cx: &mut App) {
        let old = std::mem::replace(&mut self.quick, values);
        if old.theme != values.theme || old.density != values.density {
            self.apply_theme(window, cx);
        }
        if old.show_hidden != values.show_hidden {
            self.browser.set_show_hidden(values.show_hidden);
        }
    }

    pub(super) fn toggle_setting(
        &mut self,
        setting: Setting,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_quick(self.quick.toggled(setting), window, cx);
        let change = QuickChange::of(setting, self.quick);
        self.saver.pending = Some(match self.saver.pending.take() {
            Some(older) => older.merged(change),
            None => change,
        });
        self.start_save(window, cx);
    }

    fn start_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.saver.is_saving {
            return;
        }
        let Some(change) = self.saver.pending.take() else {
            return;
        };
        self.saver.is_saving = true;
        let save = self.saver.save.clone();
        let task = cx.background_executor().spawn(async move { save(&change) });
        cx.spawn_in(window, async move |this, cx| {
            let outcome = task.await;
            let _ = this.update_in(cx, |view, window, cx| view.saved(outcome, window, cx));
        })
        .detach();
    }

    fn saved(
        &mut self,
        outcome: Result<QuickSettings, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.saver.is_saving = false;
        match outcome {
            Ok(stored) => {
                self.saver.error = None;
                let fresh = self
                    .saver
                    .pending
                    .map_or(stored, |pending| pending.applied_to(stored));
                self.set_quick(fresh, window, cx);
            }
            Err(message) => self.saver.error = Some(message.into()),
        }
        self.start_save(window, cx);
        cx.notify();
    }
}
