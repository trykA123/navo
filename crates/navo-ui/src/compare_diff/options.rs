//! The stored `[compare]` table as the options a compare runs with.

use navo_store::Compare;

/// The compare settings a diff or folder compare runs with (NC53).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompareOptions {
    /// The stored `[compare]` table.
    pub compare: Compare,
}
