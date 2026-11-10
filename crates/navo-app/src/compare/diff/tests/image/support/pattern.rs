//! A synthetic image source for the large-side hash tests; it streams zeros, not a buffer.

use std::io::{self, Read};
use std::ops::ControlFlow;
use std::sync::Arc;

use navo_compare::model::{
    CancelToken, EntryKind, ObjectId, ReadHint, Reason, RelPath, SideEntry, SideSource, Stamp,
};
use navo_compare::text::Options;
use navo_core::{FileIdentity, ProviderId, ProviderPath};

use crate::compare::diff::load::{LoadInput, Side};

/// A source of `size` zero bytes whose last byte is `last`.
pub(crate) struct PatternSource {
    size: u64,
    last: u8,
    provider: ProviderId,
}

impl PatternSource {
    /// A source with the given size and final byte.
    pub(crate) fn new(size: u64, last: u8) -> Arc<Self> {
        Arc::new(Self {
            size,
            last,
            provider: ProviderId::local(),
        })
    }
}

impl SideSource for PatternSource {
    fn provider(&self) -> &ProviderId {
        &self.provider
    }

    fn list(
        &self,
        _folder: &RelPath,
        _sink: &mut dyn FnMut(SideEntry) -> ControlFlow<()>,
        _cancel: &CancelToken,
    ) -> Result<(), Reason> {
        Ok(())
    }

    fn open(&self, _rel: &RelPath, _hint: ReadHint) -> Result<Box<dyn Read + Send>, Reason> {
        Ok(Box::new(PatternReader {
            size: self.size,
            last: self.last,
            position: 0,
        }))
    }

    fn stamp(&self, _rel: &RelPath) -> Result<Stamp, Reason> {
        Ok(Stamp {
            size: self.size,
            modified: None,
            file_id: None,
        })
    }

    fn object_id(&self, _rel: &RelPath) -> Option<ObjectId> {
        None
    }

    fn file_id(&self, _rel: &RelPath) -> Option<FileIdentity> {
        None
    }

    fn cache_path(&self, rel: &RelPath) -> ProviderPath {
        rel.as_path().clone()
    }

    fn case_sensitive(&self) -> bool {
        true
    }

    fn read_only(&self) -> bool {
        false
    }
}

/// A pair over two pattern sources.
pub(crate) fn pattern_input(left: &Arc<PatternSource>, right: &Arc<PatternSource>) -> LoadInput {
    LoadInput {
        left: Some(pattern_side(left, "left.png")),
        right: Some(pattern_side(right, "right.png")),
        options: Options::default(),
        fallback: encoding_rs::UTF_8,
    }
}

/// One side over a pattern source with its listed size.
fn pattern_side(source: &Arc<PatternSource>, name: &str) -> Side {
    let entry = SideEntry::new(
        RelPath::new(ProviderPath::from_text(name)),
        EntryKind::File,
        source.size,
        None,
    );
    let side: Arc<dyn SideSource> = source.clone();
    (side, entry)
}

/// A reader that emits zeros and sets the final byte.
struct PatternReader {
    size: u64,
    last: u8,
    position: u64,
}

impl Read for PatternReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.position >= self.size {
            return Ok(0);
        }
        let left = self.size - self.position;
        let count = usize::try_from(left)
            .unwrap_or(usize::MAX)
            .min(buffer.len());
        buffer[..count].fill(0);
        if self
            .position
            .saturating_add(u64::try_from(count).unwrap_or(u64::MAX))
            == self.size
        {
            buffer[count - 1] = self.last;
        }
        self.position = self
            .position
            .saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
        Ok(count)
    }
}
