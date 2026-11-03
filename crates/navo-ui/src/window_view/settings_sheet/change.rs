use super::{Change, FIELDS, NavoWindow};
use crate::prelude::*;

impl NavoWindow {
    pub(super) fn set_from_sheet(
        &mut self,
        change: Change,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let field = change.field;
        if self.apply_from_sheet(change, window, cx) {
            self.settings.upgrade.forget(field);
        }
    }

    pub(super) fn apply_from_sheet(
        &mut self,
        change: Change,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(row) = FIELDS.iter().position(|spec| spec.field == change.field) {
            self.select_setting(row, window, cx);
        }
        let old = self.values.change_of(change.field);
        if old == change {
            return false;
        }
        self.settings.undo.push(old);
        self.change_setting(change, window, cx);
        true
    }

    pub(super) fn undo_setting(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(old) = self.settings.undo.pop() else {
            return;
        };
        if let Some(row) = FIELDS.iter().position(|spec| spec.field == old.field) {
            self.select_setting(row, window, cx);
        }
        self.change_setting(old, window, cx);
    }
}
