use std::sync::Arc;
use std::time::Instant;

use rusty_ui::settings::{SummonKey, Values, parse_summon_key};

#[cfg_attr(
    not(any(windows, test)),
    expect(dead_code, reason = "only the Windows resident sends events")
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResidentEvent {
    Summon(Instant),
    Quit,
}

pub(crate) type Sink = Arc<dyn Fn(ResidentEvent) + Send + Sync>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ResidentConfig {
    pub(crate) keep_in_tray: bool,
    pub(crate) summon_key: Option<SummonKey>,
}

impl ResidentConfig {
    pub(crate) fn from_values(values: &Values) -> Self {
        let summon_key = parse_summon_key(values.summon_key()).unwrap_or_else(|error| {
            tracing::warn!(?error, "the summon key setting is not valid; no summon key");
            None
        });
        Self {
            keep_in_tray: values.keeps_in_tray(),
            summon_key,
        }
    }

    #[cfg(any(windows, test))]
    pub(crate) fn is_idle(self) -> bool {
        !self.keep_in_tray && self.summon_key.is_none()
    }
}

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
