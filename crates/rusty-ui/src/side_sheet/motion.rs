use std::time::{Duration, Instant};

use gpui_kit::base::animation::{ease_in_cubic, ease_out_cubic};

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct Look {
    pub opacity: f32,
    pub is_settled: bool,
}

pub(super) fn elapsed(since: Instant, now: Instant, duration: Duration) -> f32 {
    let span = duration.as_secs_f32().max(f32::EPSILON);
    (now.saturating_duration_since(since).as_secs_f32() / span).clamp(0., 1.)
}

pub(super) fn has_left(since: Instant, now: Instant, duration: Duration) -> bool {
    elapsed(since, now, duration) >= 1.
}

pub(super) fn entering(done: f32) -> Look {
    let shown = ease_out_cubic(done);
    Look {
        opacity: shown,
        is_settled: done >= 1.,
    }
}

pub(super) fn leaving(done: f32) -> Look {
    let gone = ease_in_cubic(done);
    Look {
        opacity: 1. - gone,
        is_settled: done >= 1.,
    }
}
