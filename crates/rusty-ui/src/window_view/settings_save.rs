use std::sync::Arc;

use super::FerriteWindow;
use crate::app_menu::Setting;
use crate::browser::Browser;
use crate::prelude::*;
use crate::settings::{Change, Field, Values};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveReport {
    pub values: Values,
    pub failed: Vec<Field>,
    pub error: Option<String>,
    pub was_absent: Vec<Field>,
}

impl SaveReport {
    pub fn saved(values: Values) -> Self {
        Self {
            values,
            failed: Vec::new(),
            error: None,
            was_absent: Vec::new(),
        }
    }
}

pub type SaveSettings = Arc<dyn Fn(&[Change]) -> SaveReport + Send + Sync>;

pub(super) struct Saver {
    save: SaveSettings,
    pending: Vec<Change>,
    in_flight: Vec<Field>,
    failed: Vec<Field>,
    is_saving: bool,
    pub(super) error: Option<SharedString>,
    pub(super) problem: Option<SharedString>,
    #[cfg(test)]
    pub(super) disk_applies: usize,
}

impl Saver {
    pub(super) fn new(save: SaveSettings, error: Option<SharedString>) -> Self {
        Self {
            save,
            pending: Vec::new(),
            in_flight: Vec::new(),
            failed: Vec::new(),
            is_saving: false,
            error,
            problem: None,
            #[cfg(test)]
            disk_applies: 0,
        }
    }

    pub(super) fn disk_valid(&mut self) {
        self.problem = None;
        self.error = None;
        self.failed.clear();
    }

    pub(super) fn disk_invalid(&mut self, message: SharedString) {
        self.error = Some(message.clone());
        self.problem = Some(message);
    }

    pub(super) fn over_pending(&self, values: Values) -> Values {
        self.pending
            .iter()
            .fold(values, |values, change| values.with(change))
    }

    #[allow(clippy::unused_self, reason = "counts only in tests")]
    pub(super) fn count_disk_apply(&mut self) {
        #[cfg(test)]
        {
            self.disk_applies += 1;
        }
    }

    pub(super) fn is_busy(&self) -> bool {
        self.is_saving || !self.pending.is_empty()
    }

    fn queue(&mut self, change: Change) {
        self.pending.retain(|older| older.field != change.field);
        self.pending.push(change);
    }

    fn settle(&mut self, report: &SaveReport, in_flight: &[Field]) {
        self.failed
            .retain(|field| !in_flight.contains(field) || report.failed.contains(field));
        for field in &report.failed {
            if !self.failed.contains(field) {
                self.failed.push(*field);
            }
        }
        match &report.error {
            Some(message) => self.error = Some(message.clone().into()),
            None if self.failed.is_empty() => self.error = None,
            None => {}
        }
    }
}

pub(super) fn steer_browser(values: &Values, browser: &mut Browser) {
    browser.set_show_hidden(values.show_hidden());
    browser.set_show_system(values.show_system());
    browser.set_default_sort(values.sort_key(), values.folders_first());
    browser.set_space_measures_folders(values.space_measures_folders());
}

impl FerriteWindow {
    pub(super) fn set_values(&mut self, values: Values, window: &Window, cx: &mut App) {
        let old = std::mem::replace(&mut self.values, values);
        if old.theme() != self.values.theme() || old.density() != self.values.density() {
            self.apply_theme(window, cx);
        }
        steer_browser(&self.values, &mut self.browser);
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
        self.saver.in_flight = changes.iter().map(|change| change.field).collect();
        self.settings.undo_sent(&self.saver.in_flight);
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

    fn saved(&mut self, report: SaveReport, window: &mut Window, cx: &mut Context<Self>) {
        self.saver.is_saving = false;
        let sent = std::mem::take(&mut self.saver.in_flight);
        self.saver.settle(&report, &sent);
        self.settings.undo_learned(&report, &sent);
        let fresh = self
            .saver
            .pending
            .iter()
            .fold(report.values, |values, change| values.with(change));
        self.set_values(fresh, window, cx);
        self.start_save(window, cx);
        self.locate_after_save(window, cx);
        cx.notify();
    }
}
