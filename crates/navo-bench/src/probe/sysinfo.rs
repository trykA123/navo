//! Reads the threads of one process from a `SystemProcessInformation` buffer by byte
//! offsets, so the walk is bounds-checked and testable without Windows.

use super::ProbeError;
use super::math::ThreadSwitches;

/// Field offsets in bytes; on Windows they come from the windows-sys structs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Layout {
    pub(super) pointer: usize,
    pub(super) process_size: usize,
    pub(super) process_id: usize,
    pub(super) thread_size: usize,
    pub(super) thread_id: usize,
    pub(super) switches: usize,
}

/// The 64-bit layout of `SYSTEM_PROCESS_INFORMATION` and `SYSTEM_THREAD_INFORMATION`.
#[cfg(test)]
pub(super) const X64: Layout = Layout {
    pointer: 8,
    process_size: 0x100,
    process_id: 0x50,
    thread_size: 0x50,
    thread_id: 0x30,
    switches: 0x40,
};

const NEXT_OFFSET: usize = 0;
const THREAD_COUNT: usize = 4;

/// Context-switch counters of every thread of `pid`; empty when the process is not listed.
pub(super) fn threads_of(
    buffer: &[u8],
    layout: &Layout,
    pid: u32,
) -> Result<Vec<ThreadSwitches>, ProbeError> {
    let mut at = 0_usize;
    loop {
        let entry = buffer.get(at..).ok_or(ProbeError::Malformed)?;
        if usize_at(entry, layout.process_id, layout.pointer)? == pid as usize {
            return threads(entry, layout);
        }
        let next = u32_at(entry, NEXT_OFFSET)? as usize;
        if next == 0 {
            return Ok(Vec::new());
        }
        at = at.checked_add(next).ok_or(ProbeError::Malformed)?;
    }
}

fn threads(entry: &[u8], layout: &Layout) -> Result<Vec<ThreadSwitches>, ProbeError> {
    let count = u32_at(entry, THREAD_COUNT)? as usize;
    let end = count
        .checked_mul(layout.thread_size)
        .and_then(|bytes| bytes.checked_add(layout.process_size))
        .ok_or(ProbeError::Malformed)?;
    if end > entry.len() {
        return Err(ProbeError::Malformed);
    }
    (0..count)
        .map(|index| {
            let base = layout.process_size + index * layout.thread_size;
            let thread = &entry[base..base + layout.thread_size];
            let id = usize_at(thread, layout.thread_id, layout.pointer)?;
            Ok(ThreadSwitches {
                thread_id: u32::try_from(id).map_err(|_| ProbeError::Malformed)?,
                switches: u32_at(thread, layout.switches)?,
            })
        })
        .collect()
}

fn bytes<const N: usize>(data: &[u8], at: usize) -> Result<[u8; N], ProbeError> {
    data.get(at..at.checked_add(N).ok_or(ProbeError::Malformed)?)
        .and_then(|slice| slice.try_into().ok())
        .ok_or(ProbeError::Malformed)
}

fn u32_at(data: &[u8], at: usize) -> Result<u32, ProbeError> {
    bytes::<4>(data, at).map(u32::from_ne_bytes)
}

fn usize_at(data: &[u8], at: usize, pointer: usize) -> Result<usize, ProbeError> {
    match pointer {
        4 => u32_at(data, at).map(|value| value as usize),
        8 => bytes::<8>(data, at)
            .map(u64::from_ne_bytes)
            .and_then(|value| usize::try_from(value).map_err(|_| ProbeError::Malformed)),
        _ => Err(ProbeError::Malformed),
    }
}
