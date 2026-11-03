use super::{Change, NavoWindow};
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
        self.select_setting(change.field.row(), window, cx);
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
        self.select_setting(old.field.row(), window, cx);
        self.change_setting(old, window, cx);
    }
}
