use std::time::Duration;

use gpui_kit::{Pixels, px};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Motion {
    pub fast: Duration,
    pub base: Duration,
    pub slow: Duration,
}

impl Motion {
    pub fn travel(distance: Pixels, reduce_motion: bool) -> Pixels {
        if reduce_motion { px(0.) } else { distance }
    }
}

pub(super) fn motion() -> Motion {
    Motion {
        fast: Duration::from_millis(120),
        base: Duration::from_millis(180),
        slow: Duration::from_millis(280),
    }
}
