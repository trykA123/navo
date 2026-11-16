//! Navo's `DiffBackend` constructor: it builds Navo's parts and adds the copy service.
//!
//! It sits beside the hooks so the glue can build its backend without reaching into `hooks`.

use std::sync::Arc;

use navo_fs::ProviderOps;
use navo_store::StoreDirs;
use navo_ui::browser::LogFactory;

use super::hooks::{DiffBackend, navo_parts};
use super::sources::SPOOL_DIR;
use crate::compare::dirs::CompareDirs;

/// The diff hooks; with `copy`, copies and saves keep their old files through that job factory.
/// `dirs` are Navo's compare folders, built once from the profile by `navo_compare_dirs`.
pub fn diff_backend(
    copy: Option<LogFactory>,
    local: &Arc<dyn ProviderOps>,
    dirs: CompareDirs,
) -> Arc<DiffBackend> {
    let backend = DiffBackend::new(navo_parts(dirs));
    Arc::new(match copy {
        Some(factory) => backend.copying(factory, local.clone()),
        None => backend,
    })
}

/// Navo's compare folders for profile `dirs`: the spool sits in the profile data folder, beside
/// `cache.db`. Without a profile there is no spool.
pub fn navo_compare_dirs(dirs: Option<&StoreDirs>) -> CompareDirs {
    CompareDirs {
        spool: dirs.map(|dirs| dirs.data.join(SPOOL_DIR)),
    }
}
