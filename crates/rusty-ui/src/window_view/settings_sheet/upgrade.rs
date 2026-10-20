use crate::prelude::*;
use crate::settings::{Change, DefaultShift, Field, Moved};
use crate::status_bar::StatusNotice;
use crate::window_view::FerriteWindow;

#[derive(Default)]
pub(super) struct Upgrade {
    items: Vec<Moved>,
    seen: Vec<DefaultShift>,
}

impl Upgrade {
    pub(super) fn items(&self) -> Vec<Moved> {
        self.items.clone()
    }

    fn take(&mut self, shifts: Vec<DefaultShift>) {
        for shift in shifts {
            if self.seen.contains(&shift) {
                continue;
            }
            self.items.extend(Moved::of(&shift));
            self.seen.push(shift);
        }
    }

    fn keep(&mut self, field: Field) -> Option<Change> {
        let at = self.items.iter().position(|item| item.field == field)?;
        self.items.remove(at).keep_change()
    }
}

impl FerriteWindow {
    pub(in crate::window_view) fn take_defaults(&mut self, shifts: Vec<DefaultShift>) {
        self.settings.upgrade.take(shifts);
    }

    pub(super) fn keep_old(&mut self, field: Field, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(change) = self.settings.upgrade.keep(field) {
            self.change_setting(change, window, cx);
        }
    }

    pub(super) fn dismiss_upgrade(&mut self) {
        self.settings.upgrade.items.clear();
    }

    pub(in crate::window_view) fn upgrade_notice(&self) -> Option<StatusNotice> {
        let count = self.settings.upgrade.items.len();
        (count > 0 && !self.settings.is_open()).then(|| StatusNotice {
            text: format!(
                "Defaults changed for {count} setting{}. Review in Settings.",
                if count == 1 { "" } else { "s" }
            )
            .into(),
            is_error: false,
        })
    }
}
