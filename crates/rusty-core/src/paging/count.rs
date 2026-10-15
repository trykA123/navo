use std::sync::atomic::AtomicBool;

use super::{ByteSource, Needle};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Counted {
    pub total: u64,
    pub starts: Vec<u64>,
    pub done: bool,
}

#[expect(
    clippy::too_many_arguments,
    reason = "signature fixed by the P15 A5 contract"
)]
pub fn count<S: ByteSource + ?Sized>(
    _src: &S,
    _needle: &Needle,
    _keep: usize,
    _cancel: &AtomicBool,
    _progress: &mut dyn FnMut(u64, u64),
) -> Result<Counted, String> {
    Ok(Counted::default())
}

#[must_use]
pub fn ordinal(_counted: &Counted, _at: u64) -> Option<u64> {
    None
}
