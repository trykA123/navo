use std::time::{Duration, Instant};

use gpui_kit::base::animation::ease_out_cubic;

pub(super) fn progress(started: Instant, now: Instant, duration: Duration) -> f32 {
    let elapsed = now.saturating_duration_since(started).as_secs_f32();
    let span = duration.as_secs_f32().max(f32::EPSILON);
    ease_out_cubic((elapsed / span).clamp(0., 1.))
}
