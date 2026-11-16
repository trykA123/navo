//! What the compare screen reads from the host's file layer: whole bytes and paged sources.

use std::sync::Arc;

use navo_core::Location;
use navo_core::paging::ByteSource;
use navo_l10n::tr;

/// Reads up to a byte limit from a location.
pub type ReadBytes = Arc<dyn Fn(&Location, u64) -> Result<Vec<u8>, String> + Send + Sync>;
/// Opens a location as a paged byte source.
pub type OpenSource =
    Arc<dyn Fn(&Location) -> Result<Arc<dyn ByteSource + Send + Sync>, String> + Send + Sync>;

/// The two file reads the compare screen makes.
#[derive(Clone)]
pub struct CompareFiles {
    pub bytes: ReadBytes,
    pub source: OpenSource,
}

impl CompareFiles {
    /// A layer that opens nothing; every read fails as the viewer's empty hooks do.
    pub fn none() -> Self {
        Self {
            bytes: Arc::new(|_, _| Err(tr("compare-files-cannot-open"))),
            source: Arc::new(|_| Err(tr("compare-files-cannot-open"))),
        }
    }
}
