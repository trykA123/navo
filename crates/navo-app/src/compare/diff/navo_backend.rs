//! Navo's `DiffBackend` constructor: it builds Navo's parts (`LocalSide` and Git sides, Viewed
//! marks in `cache.db`) and adds the copy service as its host.
//!
//! It sits beside the hooks so the glue can build its backend without reaching into `hooks`.

use std::path::PathBuf;
use std::sync::Arc;

use navo_fs::ProviderOps;
use navo_store::StoreDirs;
use navo_ui::browser::LogFactory;

use super::copy_hook::CopyService;
use super::hooks::{BackendParts, DiffBackend};
use super::sides::navo_side_of;
use super::sources::SPOOL_DIR;
use super::viewed::opener;
use crate::compare::dirs::CompareDirs;

/// The diff hooks; with `copy`, copies and saves keep their old files through that job factory.
/// `dirs` are Navo's compare folders, built once from the profile by `navo_compare_dirs`;
/// `cache_db` is the profile's `cache.db`, which holds the Viewed marks (memory if `None`).
pub fn diff_backend(
    copy: Option<LogFactory>,
    local: &Arc<dyn ProviderOps>,
    dirs: CompareDirs,
    cache_db: Option<PathBuf>,
) -> Arc<DiffBackend> {
    let backend = DiffBackend::new(navo_parts(dirs, cache_db));
    Arc::new(match copy {
        Some(factory) => backend.with_copy_host(Arc::new(CopyService::new(factory, local.clone()))),
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

/// Navo's sides over `dirs`, with Viewed marks in the cache file `cache_db` (memory if `None`).
pub(super) fn navo_parts(dirs: CompareDirs, cache_db: Option<PathBuf>) -> BackendParts {
    BackendParts {
        dirs,
        side: Arc::new(navo_side_of),
        viewed: opener(cache_db),
    }
}
