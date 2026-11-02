use std::sync::atomic::{Ordering, compiler_fence};

use super::ClipboardError;

/// `CF_UNICODETEXT` bytes (UTF-16LE plus the NUL terminator), zeroed on drop.
pub(super) struct Utf16Text(Vec<u8>);

impl Utf16Text {
    /// Fails with `Malformed` on an embedded NUL, which would cut the text short.
    pub(super) fn new(text: &str) -> Result<Self, ClipboardError> {
        if text.contains('\0') {
            return Err(ClipboardError::Malformed);
        }
        // A UTF-16 unit never needs more than one UTF-8 byte, so the buffer never reallocates.
        let capacity = text
            .len()
            .checked_add(1)
            .and_then(|units| units.checked_mul(2))
            .ok_or(ClipboardError::Malformed)?;
        let mut bytes = Vec::with_capacity(capacity);
        for unit in text.encode_utf16().chain(std::iter::once(0)) {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        Ok(Self(bytes))
    }

    pub(super) fn bytes(&self) -> &[u8] {
        &self.0
    }
}

impl Drop for Utf16Text {
    fn drop(&mut self) {
        wipe(&mut self.0);
    }
}

pub(super) fn wipe(bytes: &mut [u8]) {
    for byte in bytes.iter_mut() {
        // SAFETY: byte is a valid, exclusive reference; the volatile write keeps the zeroing.
        unsafe { std::ptr::write_volatile(byte, 0) };
    }
    compiler_fence(Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::{ClipboardError, Utf16Text, wipe};

    #[test]
    fn text_becomes_nul_terminated_utf16le() {
        let text = Utf16Text::new("a🦀").unwrap();
        assert_eq!(text.bytes(), [0x61, 0, 0x3E, 0xD8, 0x80, 0xDD, 0, 0]);
    }

    #[test]
    fn empty_text_is_only_the_terminator() {
        assert_eq!(Utf16Text::new("").unwrap().bytes(), [0, 0]);
    }

    #[test]
    fn an_embedded_nul_is_malformed() {
        assert!(matches!(
            Utf16Text::new("before\0after"),
            Err(ClipboardError::Malformed)
        ));
    }

    #[test]
    fn the_buffer_fits_its_first_allocation() {
        for sample in ["", "ascii only", "ăîșț", "資料", "🦀🎵🦀"] {
            let text = Utf16Text::new(sample).unwrap();
            assert!(text.0.len() <= (sample.len() + 1) * 2);
        }
    }

    #[test]
    fn wipe_zeroes_every_byte() {
        let mut bytes = *b"secret value";
        wipe(&mut bytes);
        assert_eq!(bytes, [0; 12]);
    }
}
