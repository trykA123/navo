mod container;
mod frame;
mod header;
mod index;
mod stream;
mod window;

use std::collections::BTreeMap;
use std::io;

pub use header::{BlfHeader, SystemTime};
pub use index::index_blf;
pub(crate) use window::BinaryPlan;

const MAX_CONTAINER: usize = 64 * 1024 * 1024;
const MAX_HEADER: usize = 4096;
const MAX_SKIPPED_TYPES: usize = 65_536;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BlfPosition {
    pub container: u64,
    pub inner: u32,
}

#[derive(Debug, Clone, Default)]
pub struct BlfInfo {
    pub header: Option<BlfHeader>,
    pub skipped: BTreeMap<u32, u64>,
    pub problem_position: Option<BlfPosition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Checkpoint {
    pub row: u64,
    pub object: u64,
    pub position: BlfPosition,
}

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("{0}")]
    Invalid(&'static str),
    #[error("reading BLF: {0}")]
    Read(#[from] io::Error),
    #[error("indexing was cancelled")]
    Cancelled,
}

type Result<T> = std::result::Result<T, Error>;

fn read_exact(source: &mut impl io::Read, into: &mut [u8]) -> Result<()> {
    match source.read_exact(into) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
            Err(Error::Invalid("truncated BLF input"))
        }
        Err(error) => Err(error.into()),
    }
}

fn count_skipped(skipped: &mut BTreeMap<u32, u64>, kind: u32) -> Result<()> {
    if skipped.len() >= MAX_SKIPPED_TYPES && !skipped.contains_key(&kind) {
        return Err(Error::Invalid("too many distinct BLF object types"));
    }
    let count = skipped.entry(kind).or_default();
    *count = count
        .checked_add(1)
        .ok_or(Error::Invalid("BLF count overflow"))?;
    Ok(())
}

fn check(watch: &mut crate::Watch<'_>, at: u64) -> Result<()> {
    if watch.cancel.load(std::sync::atomic::Ordering::Relaxed) {
        return Err(Error::Cancelled);
    }
    (watch.progress)(at);
    Ok(())
}

#[cfg(test)]
mod tests;
