//! The Stream's resync decision (N4f1): whether a change of the compare's rows rebuilds the
//! items at once, waits out the throttle, or skips a stamp the items were already built from.

use std::time::{Duration, Instant};

use super::stream::StreamScreen;
use crate::compare_diff::{PairRequest, StreamList};

/// The shortest time between two rebuilds of the items while the compare still runs.
const RESYNC: Duration = Duration::from_millis(500);

/// What a resync does with the compare's new rows stamp.
#[derive(Debug, PartialEq)]
pub(super) enum ResyncStep {
    /// The items were built from this stamp already; nothing to do.
    Skip,
    /// A rebuild would come too soon after the last one while the compare still runs.
    Wait(Duration),
    /// Rebuild the items now, under the next generation when `fresh`.
    Rebuild { fresh: bool },
}

/// The resync decision for the rows stamp `stamp`: `Skip` when the items were built from it
/// (`built`), `Wait` for the rest of the 500 ms after `last` while the compare is `checking`,
/// else `Rebuild`, `fresh` when the generation changed or nothing was built yet. `now` is the
/// executor clock the last rebuild was stamped with.
#[expect(
    clippy::too_many_arguments,
    reason = "the decision reads five facts (N4f1)"
)]
pub(super) fn resync_step(
    built: Option<(u64, u64)>,
    last: Option<Instant>,
    stamp: (u64, u64),
    checking: bool,
    now: Instant,
) -> ResyncStep {
    if built == Some(stamp) {
        return ResyncStep::Skip;
    }
    let fresh = built.is_none_or(|(generation, _)| generation != stamp.0);
    let wait = last.map_or(Duration::ZERO, |at| {
        (at + RESYNC).saturating_duration_since(now)
    });
    if !fresh && checking && !wait.is_zero() {
        return ResyncStep::Wait(wait);
    }
    ResyncStep::Rebuild { fresh }
}

impl StreamScreen {
    /// The resync decision for the compare's new rows `stamp`, from the stamp the items were
    /// built from and when they were last rebuilt.
    pub(super) fn resync_step(
        &self,
        stamp: (u64, u64),
        checking: bool,
        now: Instant,
    ) -> ResyncStep {
        resync_step(self.stamp, self.resynced, stamp, checking, now)
    }

    /// Records `stamp` and `now` as the items' build and replaces them with `items`, under the
    /// next generation when `fresh`, else the same one. False when nothing changed.
    #[expect(
        clippy::too_many_arguments,
        reason = "the build records four facts (N4f1)"
    )]
    pub(super) fn resync_apply(
        &mut self,
        stamp: (u64, u64),
        now: Instant,
        items: (StreamList, Vec<PairRequest>),
        fresh: bool,
    ) -> bool {
        self.stamp = Some(stamp);
        self.resynced = Some(now);
        self.replace_items(items, fresh)
    }
}
