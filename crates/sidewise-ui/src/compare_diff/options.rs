//! The stored `[compare]` table as the options a compare runs with.

use navo_compare::settings::Compare;

/// The compare settings a diff or folder compare runs with (NC53).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompareOptions {
    /// The stored `[compare]` table.
    pub compare: Compare,
    /// Not stored: true for one start (F5, Recheck all) that ignores remembered hashes and stores fresh ones (NC209).
    pub fresh_hashes: bool,
}
