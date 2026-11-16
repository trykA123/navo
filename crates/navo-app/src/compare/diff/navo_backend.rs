//! Navo's `DiffBackend` constructor: it picks the copy service or the default backend.
//!
//! It sits beside the hooks so the glue can build its backend without reaching into `hooks`.

use std::sync::Arc;

use navo_fs::ProviderOps;
use navo_ui::browser::LogFactory;

use super::hooks::DiffBackend;

/// The diff hooks; with `copy`, copies and saves keep their old files through that job factory.
pub fn diff_backend(copy: Option<LogFactory>, local: &Arc<dyn ProviderOps>) -> Arc<DiffBackend> {
    Arc::new(match copy {
        Some(factory) => DiffBackend::with_copy(factory, local.clone()),
        None => DiffBackend::default(),
    })
}
