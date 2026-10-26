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
    pub generation: u64,
}

impl SaveReport {
    pub fn saved(values: Values) -> Self {
        Self {
            values,
            failed: Vec::new(),
            error: None,
            was_absent: Vec::new(),
            generation: 0,
        }
    }
}

pub type SaveSettings = Arc<dyn Fn(&[Change]) -> SaveReport + Send + Sync>;

pub(super) struct Saver {
    save: SaveSettings,
    pending: Vec<Change>,
    in_flight: Vec<Change>,
    failed: Vec<Field>,
    disk: Values,
    generation: u64,
    is_saving: bool,
    pub(super) error: Option<SharedString>,
    pub(super) problem: Option<SharedString>,
    pub(super) font_notice: Option<crate::status_bar::StatusNotice>,
    #[cfg(test)]
    pub(super) disk_applies: usize,
}

impl Saver {
    pub(super) fn new(save: SaveSettings, error: Option<SharedString>, disk: Values) -> Self {
        Self {
            save,
            pending: Vec::new(),
            in_flight: Vec::new(),
            failed: Vec::new(),
            disk,
            generation: 0,
            is_saving: false,
            error,
            problem: None,
            font_notice: None,
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

    pub(super) fn is_stale(&self, generation: u64) -> bool {
        generation < self.generation
    }

    pub(super) fn on_disk(&mut self, values: Values, generation: u64) -> Values {
        if !self.is_stale(generation) {
            self.disk = values;
            self.generation = generation;
        }
        self.over_pending(self.disk.clone())
    }

    fn over_pending(&self, values: Values) -> Values {
        self.in_flight
            .iter()
            .chain(&self.pending)
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

fn fields_of(changes: &[Change]) -> Vec<Field> {
    changes.iter().map(|change| change.field).collect()
}

pub(super) fn steer(values: &Values, browser: &mut Browser) {
    browser.set_show_hidden(values.show_hidden());
    browser.set_show_system(values.show_system());
    browser.set_default_sort(values.sort_key(), values.folders_first());
    browser.set_space_measures_folders(values.space_measures_folders());
    browser.set_copy_mode(values.copy_mode());
    browser.set_rename_logging(values.flag(Field::RenameLog));
}

impl FerriteWindow {
    pub(super) fn set_values(&mut self, values: Values, window: &Window, cx: &mut App) {
        let old = std::mem::replace(&mut self.values, values);
        let looks = |values: &Values| {
            (
                values.theme(),
                values.color_theme(),
                values.density(),
                values.syntax_theme(),
            )
        };
        if looks(&old) != looks(&self.values)
            || old.get(Field::Font) != self.values.get(Field::Font)
            || old.get(Field::FontSize) != self.values.get(Field::FontSize)
            || old.get(Field::UiScale) != self.values.get(Field::UiScale)
        {
            self.apply_theme(window, cx);
        }
        steer(&self.values, &mut self.browser);
        self.show_parts(cx);
        self.apply_preview_parts();
        self.refresh_window_menu(cx);
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
        self.saver.in_flight.clone_from(&changes);
        self.settings.undo_sent(&fields_of(&changes));
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
        let flight = std::mem::take(&mut self.saver.in_flight);
        let sent = fields_of(&flight);
        self.saver.settle(&report, &sent);
        self.settings.undo_learned(&report, &sent);
        self.settings.upgrade_settled(&report, &sent);
        self.keep_settled(&report, &flight, cx);
        let fresh = self.saver.on_disk(report.values, report.generation);
        self.set_values(fresh, window, cx);
        self.start_save(window, cx);
        self.locate_after_save(window, cx);
        cx.notify();
    }
}
