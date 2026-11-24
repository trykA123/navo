use super::controls::Status;
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
        let status = Status::of_change(change.field, &change.value);
        self.change_setting(change, window, cx);
        self.settings.status = Some(status);
        true
    }

    pub(super) fn undo_setting(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(old) = self.settings.undo.pop() else {
            return;
        };
        let field = old.field;
        self.select_setting(field.row(), window, cx);
        self.change_setting(old, window, cx);
        self.settings.status = Some(Status::Undid(field));
    }
}
