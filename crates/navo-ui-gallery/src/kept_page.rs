//! Job cards for moves that kept files in place (none, one, five) and the drawer that lists them.

use std::time::Instant;

use navo_ui::activity::Activity;
use navo_ui::activity_strip::ActivityDrawer;
use navo_ui::job_card::JobCard;
use navo_ui::prelude::*;
use navo_ui::tokens;

use crate::job_sample;
use crate::sections::section;

const PHONE: f32 = 390.;

fn card(count: usize) -> JobCard {
    let id = u64::try_from(count).unwrap_or_default();
    JobCard::new(vec![job_sample::kept(id, count)])
        .on_shrink(|_, _| {})
        .on_dismiss(|_, _, _| {})
        .on_more(|_, _, _| {})
}

fn drawer() -> ActivityDrawer {
    let mut activity = Activity::default();
    activity.sync(&[job_sample::kept(5, 5)], Instant::now());
    ActivityDrawer::new(activity.primary().into_iter().cloned().collect())
}

/// The three cards and the drawer side by side, or stacked in a 390 px column when `is_narrow`.
pub fn page(is_narrow: bool, cx: &App) -> Div {
    let t = tokens::current(cx).clone();
    let cards = [(0, "NONE LISTED"), (1, "ONE KEPT"), (5, "FIVE KEPT")]
        .map(|(count, title)| section(&t, title, card(count)));
    let content = div()
        .flex()
        .flex_wrap()
        .items_start()
        .gap(t.spacing.xxl)
        .children(cards)
        .child(div().w(px(PHONE)).child(section(&t, "DRAWER", drawer())));
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .p(t.spacing.xxl)
        .bg(t.colors.background)
        .when(is_narrow, |page| page.w(px(PHONE)).p(t.spacing.sm))
        .child(content)
}
