use std::sync::Arc;

use super::FerriteWindow;
use crate::app_menu::Setting;
use crate::prelude::*;
use crate::settings::{Change, Values};

pub type SaveSettings = Arc<dyn Fn(&[Change]) -> Result<Values, String> + Send + Sync>;

pub(super) struct Saver {
    save: SaveSettings,
    pending: Vec<Change>,
    is_saving: bool,
    pub(super) error: Option<SharedString>,
}

impl Saver {
    pub(super) fn new(save: SaveSettings, error: Option<SharedString>) -> Self {
        Self {
            save,
            pending: Vec::new(),
            is_saving: false,
            error,
        }
    }

    fn queue(&mut self, change: Change) {
        self.pending.retain(|older| older.field != change.field);
        self.pending.push(change);
    }
}

impl FerriteWindow {
    pub(super) fn set_values(&mut self, values: Values, window: &Window, cx: &mut App) {
        let old = std::mem::replace(&mut self.values, values);
        if old.theme() != self.values.theme() || old.density() != self.values.density() {
            self.apply_theme(window, cx);
        }
        if old.show_hidden() != self.values.show_hidden() {
            self.browser.set_show_hidden(self.values.show_hidden());
        }
    }

    pub(super) fn toggle_setting(
        &mut self,
        setting: Setting,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let change = self.values.toggled(setting);
        self.change_setting(change, window, cx);
    }

    pub(super) fn change_setting(
        &mut self,
        change: Change,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_values(self.values.with(&change), window, cx);
        self.saver.queue(change);
        self.start_save(window, cx);
    }

    fn start_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.saver.is_saving || self.saver.pending.is_empty() {
            return;
        }
        let changes = std::mem::take(&mut self.saver.pending);
        self.saver.is_saving = true;
        let save = self.saver.save.clone();
        let task = cx
            .background_executor()
            .spawn(async move { save(&changes) });
        cx.spawn_in(window, async move |this, cx| {
            let outcome = task.await;
            let _ = this.update_in(cx, |view, window, cx| view.saved(outcome, window, cx));
        })
        .detach();
    }

    fn saved(
        &mut self,
        outcome: Result<Values, String>,
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
                    .iter()
                    .fold(stored, |values, change| values.with(change));
                self.set_values(fresh, window, cx);
            }
            Err(message) => self.saver.error = Some(message.into()),
        }
        self.start_save(window, cx);
        cx.notify();
    }
}
