use std::time::{Duration, Instant};

use gpui_kit::base::animation::ease_out_cubic;
use gpui_kit::{Pixels, px};
use rusty_core::shell_menu::Row;

use crate::tokens::Motion;

const RISE: f32 = 4.;
pub(super) const GROW: Duration = Duration::from_millis(150);

pub(super) fn progress(started: Instant, now: Instant, duration: Duration) -> f32 {
    let elapsed = now.saturating_duration_since(started).as_secs_f32();
    let span = duration.as_secs_f32().max(f32::EPSILON);
    ease_out_cubic((elapsed / span).clamp(0., 1.))
}

pub(super) fn rise(entered: f32, reduce_motion: bool) -> Pixels {
    Motion::travel(px(RISE), reduce_motion) * (1. - entered)
}

pub(super) struct Grow {
    pub from: Pixels,
    pub rows: Vec<Row>,
    pub started: Instant,
}

impl Grow {
    pub fn height(&self, to: Pixels, done: f32) -> Pixels {
        self.from + (to - self.from) * done
    }
}

pub(super) struct RootMotion<'a> {
    pub rise: Pixels,
    pub opacity: f32,
    pub fresh: Option<(&'a [Row], f32)>,
}
