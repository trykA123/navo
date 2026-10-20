use std::time::Instant;

use rusty_ui::activity::{Activity, ActivityItem};
use rusty_ui::activity_strip::{ActivityDrawer, ActivityStrip, drawer_stops};
use rusty_ui::prelude::*;
use rusty_ui::tokens::Theme;

use super::{JobsLook, WindowPage};
use crate::job_sample;

impl WindowPage {
    fn activity_items(&self) -> Vec<ActivityItem> {
        let Some(shown) = self.jobs else {
            return Vec::new();
        };
        let mut activity = Activity::default();
        activity.sync(&job_sample::jobs(shown.count), Instant::now());
        activity
            .primary()
            .into_iter()
            .chain(activity.rest())
            .cloned()
            .collect()
    }

    pub(super) fn activity_row(&self) -> Option<ActivityStrip> {
        let items = self.activity_items();
        (!items.is_empty()).then(|| ActivityStrip::new(items).all_key(Some("alt-j")))
    }

    pub(super) fn activity_drawer(&self, t: &Theme) -> Option<impl IntoElement + use<>> {
        self.jobs.filter(|shown| shown.look == JobsLook::Drawer)?;
        let items = self.activity_items();
        let first = drawer_stops(&items).first().copied();
        Some(
            div()
                .absolute()
                .left(t.spacing.md)
                .right(t.spacing.md)
                .bottom(t.spacing.xs)
                .child(ActivityDrawer::new(items).selected(first).focused(true)),
        )
    }
}
