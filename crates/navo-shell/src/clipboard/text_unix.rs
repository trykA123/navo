use super::ClipboardError;

pub fn write_text(_text: &str) -> Result<(), ClipboardError> {
    Err(ClipboardError::Unsupported)
}

pub fn write_private_text(_text: &str) -> Result<u32, ClipboardError> {
    Err(ClipboardError::Unsupported)
}

pub fn clear_if_sequence(_expected: u32) -> Result<bool, ClipboardError> {
    Err(ClipboardError::Unsupported)
}
