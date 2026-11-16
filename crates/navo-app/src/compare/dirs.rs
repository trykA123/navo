//! The folders the compare glue may write to, chosen by the host app (NC101, NC112).
//!
//! Navo fills this from its profile; a host without a profile passes the default, which spools
//! nothing and sweeps nothing.

use std::path::PathBuf;

/// Host-chosen compare folders. `spool` is the only folder the start-up sweep may delete in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompareDirs {
    /// The Git blob spool root; `None` means no spool and no sweep.
    pub spool: Option<PathBuf>,
}
