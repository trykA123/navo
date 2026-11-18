//! Fill-to-paint timing of one paged fill (D12); logged once, at the fill's first paint.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

/// When a fill was requested, how long its parts took, and whether it has painted yet.
#[derive(Clone)]
pub(in crate::window_view::viewer::paged) struct FillTiming {
    requested: Instant,
    fill_us: u64,
    lex_us: u64,
    rows: usize,
    painted: Rc<Cell<bool>>,
}

impl FillTiming {
    /// Timing of a fill requested at `requested`, with `(fill_us, lex_us)` and `rows` read.
    pub(in crate::window_view::viewer::paged) fn new(
        requested: Instant,
        parts: (u64, u64),
        rows: usize,
    ) -> Self {
        Self {
            requested,
            fill_us: parts.0,
            lex_us: parts.1,
            rows,
            painted: Rc::default(),
        }
    }

    /// The microseconds since the request on the first call; `None` on every later call.
    pub(in crate::window_view::viewer::paged) fn paint_once(&self) -> Option<u64> {
        if self.painted.replace(true) {
            return None;
        }
        Some(u64::try_from(self.requested.elapsed().as_micros()).unwrap_or(u64::MAX))
    }

    /// Logs the fill's debug line at its first paint; later paints log nothing.
    pub(in crate::window_view::viewer::paged) fn log_first_paint(&self) {
        if let Some(paint_us) = self.paint_once() {
            tracing::debug!(
                target: "navo::paged",
                fill_us = self.fill_us,
                lex_us = self.lex_us,
                paint_us,
                rows = self.rows,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::FillTiming;

    #[test]
    fn paint_once_returns_the_time_on_the_first_call_only() {
        let timing = FillTiming::new(Instant::now(), (1, 2), 3);
        let copy = timing.clone();
        assert!(timing.paint_once().is_some());
        assert_eq!(copy.paint_once(), None);
        assert_eq!(timing.paint_once(), None);
    }
}
