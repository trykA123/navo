use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub(crate) struct ProcessClock {
    start: Instant,
}

impl ProcessClock {
    pub(super) fn starting_at(start: Instant) -> Self {
        Self { start }
    }

    pub(crate) fn detect(
        main_started: Instant,
        read_age: impl FnOnce() -> Option<Duration>,
    ) -> Self {
        let now = Instant::now();
        let start = read_age()
            .and_then(|age| now.checked_sub(age))
            .map_or(main_started, |start| start.min(main_started));
        Self::starting_at(start)
    }

    #[cfg(test)]
    pub(super) fn start(&self) -> Instant {
        self.start
    }

    pub(super) fn ms(&self, at: Instant) -> f64 {
        let ms = at.saturating_duration_since(self.start).as_secs_f64() * 1000.0;
        (ms * 10.0).round() / 10.0
    }
}
