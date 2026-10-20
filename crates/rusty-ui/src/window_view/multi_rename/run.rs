use std::sync::Arc;
use std::time::Duration;

use rusty_l10n::tr;

use super::{FerriteWindow, first_new_name, gather, inputs};
use crate::multi_rename::view::fields::FieldErrors;
use crate::prelude::*;
use crate::status_bar::StatusNotice;

impl FerriteWindow {
    pub(super) fn run_multi_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.multi_rename.open.as_ref() else {
            return;
        };
        let mut typed = open.form.rules.clone();
        inputs::read(&open.inputs, &mut typed, cx);
        if typed != open.form.rules {
            self.rules_changed(window, cx);
        }
        let Some(open) = self.multi_rename.open.as_mut() else {
            return;
        };
        if open.errors != FieldErrors::default() {
            return;
        }
        if !open.is_current() {
            open.run_waiting_for = Some(open.form.clone());
            return;
        }
        if self.refresh_changed_folder(window, cx) {
            return;
        }
        self.start_rename(window, cx);
    }

    fn start_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.multi_rename.open.as_ref() else {
            return;
        };
        let Some(preview) = open.preview.as_ref() else {
            return;
        };
        let Some(plan) = preview.plan.clone().filter(|_| preview.can_run()) else {
            if preview.summary.problems == 0 {
                return self.notify_rename("rename-nothing", cx);
            }
            return self.step_problem(true, window, cx);
        };
        let first_new = first_new_name(preview);
        let rules = open.form.rules();
        if self.browser.renaming() {
            return self.notify_rename("rename-busy", cx);
        }
        if !self.browser.rename_batch(plan, first_new.as_deref()) {
            return self.notify_rename("rename-nothing", cx);
        }
        self.save_last_used(&rules, cx);
        self.refresh_activity(cx);
        self.close_multi_rename(window, cx);
    }

    pub(super) fn notify_rename(&mut self, id: &str, cx: &mut Context<Self>) {
        self.show_notice(StatusNotice {
            text: tr(id).into(),
            is_error: true,
        });
        cx.notify();
    }

    fn refresh_changed_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(open) = self.multi_rename.open.as_mut() else {
            return false;
        };
        let gather::Gathered::Ready(batch) = gather::gather(&self.browser) else {
            self.notify_rename("rename-folder-changed", cx);
            return true;
        };
        let has_items = batch.has_items(&open.source.items);
        if has_items && batch.existing == *open.source.existing {
            return false;
        }
        if !has_items {
            open.form.clear_excluded();
        }
        open.source.items = Arc::new(batch.items);
        open.source.existing = Arc::new(batch.existing);
        open.source.fields = Arc::default();
        self.schedule_rename_preview(Duration::ZERO, window, cx);
        self.notify_rename("rename-folder-changed", cx);
        true
    }

    pub(super) fn step_problem(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.multi_rename.open.as_ref() else {
            return;
        };
        let Some(preview) = open.preview.as_ref() else {
            return;
        };
        let cursor = open.view.read(cx).cursor();
        let row = if forward {
            preview.next_problem(cursor)
        } else {
            preview.previous_problem(cursor)
        };
        let Some(row) = row else {
            return;
        };
        open.view
            .update(cx, |view, cx| view.set_cursor(Some(row), cx));
        self.focus_rename_preview(window, cx);
    }
}
