use std::ptr;

use windows_sys::Win32::System::Ole::CF_UNICODETEXT;

use super::ClipboardError;
use super::platform::{Clipboard, GlobalBlock, Owner, empty, sequence};
use super::text_buffer::Utf16Text;
use super::watch::{BUSY_ATTEMPTS, BUSY_DELAY, register};

const PRIVATE_FORMATS: [&str; 3] = [
    "ExcludeClipboardContentFromMonitorProcessing",
    "CanIncludeInClipboardHistory",
    "CanUploadToCloudClipboard",
];

/// Replaces the clipboard with `text`. Call it off the UI thread: it waits while another
/// program holds the clipboard. An embedded NUL is `Malformed`.
pub fn write_text(text: &str) -> Result<(), ClipboardError> {
    let block = text_block(text)?;
    let owner = Owner::new()?;
    let _clipboard = open(&owner)?;
    empty()?;
    block.publish(u32::from(CF_UNICODETEXT))
}

/// Replaces the clipboard with `text` marked so clipboard history, monitors and cloud sync
/// skip it, and returns the clipboard sequence read while the clipboard was still open, so
/// it is ours. On `Err` the clipboard holds nothing from this call, and every copy of the
/// text it made is zeroed (the one exception: if Windows refuses every re-lock of a block it never took, the block is freed unwiped). The caller's own `&str` copy of the secret is the caller's to wipe.
///
/// Waits at most about 100 ms (5 attempts of 20 ms) for another program to release the
/// clipboard, so call it on a worker thread.
pub fn write_private_text(text: &str) -> Result<u32, ClipboardError> {
    let block = text_block(text)?;
    let markers = private_markers()?;
    let owner = Owner::new()?;
    publish_private(&owner, block, markers)
}

/// Empties the clipboard only when its sequence is still `expected`; returns whether it did.
/// A sequence of 0 never clears.
pub fn clear_if_sequence(expected: u32) -> Result<bool, ClipboardError> {
    if expected == 0 {
        return Ok(false);
    }
    let _clipboard = Clipboard::open_with(ptr::null_mut(), BUSY_ATTEMPTS, BUSY_DELAY)?;
    if sequence() != expected {
        return Ok(false);
    }
    empty()?;
    Ok(true)
}

fn text_block(text: &str) -> Result<GlobalBlock, ClipboardError> {
    let units = Utf16Text::new(text)?;
    GlobalBlock::new(units.bytes())
}

fn private_markers() -> Result<Vec<(u32, GlobalBlock)>, ClipboardError> {
    PRIVATE_FORMATS
        .iter()
        .map(|name| Ok((register(name)?, GlobalBlock::new(&0u32.to_le_bytes())?)))
        .collect()
}

fn publish_private(
    owner: &Owner,
    text: GlobalBlock,
    markers: Vec<(u32, GlobalBlock)>,
) -> Result<u32, ClipboardError> {
    let _clipboard = open(owner)?;
    empty()?;
    let published = markers
        .into_iter()
        .try_for_each(|(format, block)| block.publish(format))
        .and_then(|()| text.publish(u32::from(CF_UNICODETEXT)));
    if let Err(error) = published {
        let _ = empty();
        return Err(error);
    }
    let written = sequence();
    if written == 0 {
        empty()?;
        return Err(ClipboardError::Os(
            "The clipboard sequence is unavailable".to_owned(),
        ));
    }
    Ok(written)
}

fn open(owner: &Owner) -> Result<Clipboard, ClipboardError> {
    Clipboard::open_with(owner.0.0, BUSY_ATTEMPTS, BUSY_DELAY)
}
