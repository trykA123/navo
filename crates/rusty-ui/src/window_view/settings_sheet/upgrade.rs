use crate::prelude::*;
use crate::settings::{Change, DefaultShift, Field, Moved, Values};
use crate::status_bar::StatusNotice;
use crate::window_view::{FerriteWindow, SaveReport};

#[derive(Default)]
pub(super) struct Upgrade {
    items: Vec<Moved>,
    seen: Vec<DefaultShift>,
    keeping: Vec<Field>,
}

impl Upgrade {
    pub(super) fn items(&self) -> Vec<Moved> {
        self.items.clone()
    }

    fn take(&mut self, shifts: Vec<DefaultShift>, values: &Values) {
        for shift in shifts {
            if self.seen.contains(&shift) {
                continue;
            }
            self.items
                .extend(Moved::of(&shift).filter(|item| item.is_current(values)));
            self.seen.push(shift);
        }
    }

    fn keep(&mut self, field: Field, values: &Values) -> Option<Change> {
        let item = self.items.iter().find(|item| item.field == field)?;
        let change = item.keep_change().filter(|_| item.is_current(values))?;
        self.keeping.push(field);
        Some(change)
    }

    pub(super) fn forget(&mut self, field: Field) {
        self.items.retain(|item| item.field != field);
        self.keeping.retain(|kept| *kept != field);
    }

    fn follow(&mut self, values: &Values) {
        let keeping = &self.keeping;
        self.items
            .retain(|item| keeping.contains(&item.field) || item.is_current(values));
    }

    pub(super) fn settled(&mut self, report: &SaveReport, sent: &[Field]) {
        for field in sent {
            if self.keeping.contains(field) && !report.failed.contains(field) {
                self.forget(*field);
            }
        }
        self.keeping.retain(|kept| !sent.contains(kept));
    }
}

impl super::SettingsPanel {
    pub(in crate::window_view) fn upgrade_settled(&mut self, report: &SaveReport, sent: &[Field]) {
        self.upgrade.settled(report, sent);
    }
}

impl FerriteWindow {
    pub(in crate::window_view) fn take_defaults(&mut self, shifts: Vec<DefaultShift>) {
        self.settings.upgrade.take(shifts, &self.values);
    }

    pub(super) fn keep_old(&mut self, field: Field, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(change) = self.settings.upgrade.keep(field, &self.values)
            && !self.apply_from_sheet(change, window, cx)
        {
            self.settings.upgrade.forget(field);
        }
    }

    pub(in crate::window_view) fn follow_defaults(&mut self) {
        self.settings.upgrade.follow(&self.values);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keep_writes_nothing_once_the_value_is_no_longer_the_new_default() {
        let shift = DefaultShift {
            key: "files.sort".to_owned(),
            old: "\"date\"".to_owned(),
            new: "\"name\"".to_owned(),
        };
        let mut upgrade = Upgrade::default();
        upgrade.take(vec![shift], &Values::default());
        let picked = Values::default().with(&Change {
            field: Field::Sort,
            value: crate::settings::Value::Pick(1),
        });
        assert_eq!(upgrade.keep(Field::Sort, &picked), None);
        assert!(upgrade.keep(Field::Sort, &Values::default()).is_some());
    }
}
