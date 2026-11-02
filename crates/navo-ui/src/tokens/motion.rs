use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Motion {
    pub fade: Duration,
}

pub(super) fn motion() -> Motion {
    Motion {
        fade: Duration::from_millis(80),
    }
}
