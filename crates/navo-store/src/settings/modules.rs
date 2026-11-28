//! First-party module switches stored under `[modules]` in `navo.toml`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Per-module enable flags from the `[modules]` table in `navo.toml`.
///
/// Known v1.0 defaults when an id is absent: `automotive` is on (`true`) and
/// `terminal` is off (`false`). Unknown ids default to `false` when absent and
/// are preserved across parse and save round-trips.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Modules(BTreeMap<String, bool>);

const KNOWN_DEFAULTS: &[(&str, bool)] = &[("automotive", true), ("terminal", false)];

impl Modules {
    /// Returns whether the module `id` is enabled, falling back to its known
    /// default when absent from `[modules]` (`false` for unknown ids).
    #[must_use]
    pub fn is_on(&self, id: &str) -> bool {
        self.0
            .get(id)
            .copied()
            .unwrap_or_else(|| default_enabled(id))
    }

    /// Sets the explicit enabled flag for module `id`.
    pub fn set(&mut self, id: impl Into<String>, enabled: bool) {
        self.0.insert(id.into(), enabled);
    }
}

fn default_enabled(id: &str) -> bool {
    KNOWN_DEFAULTS
        .iter()
        .find_map(|&(known, enabled)| (known == id).then_some(enabled))
        .unwrap_or(false)
}
