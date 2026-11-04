//! File kinds from the lowercase extension or the first bytes; never from names.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Kind {
    Archive,
    Image,
    Pdf,
    Elf,
    Map,
    A2l,
    Dbc,
}

pub(super) const SNIFF_BYTES: usize = 8 * 1024;
pub(super) const SNIFF_FILES: u32 = 5_000;

pub(super) fn from_extension(extension: &str) -> Option<Kind> {
    Some(match extension {
        "zip" | "7z" | "rar" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "zst" | "cab" => Kind::Archive,
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "tif" | "tiff" | "ico" | "svg" => {
            Kind::Image
        }
        "pdf" => Kind::Pdf,
        "elf" => Kind::Elf,
        "map" => Kind::Map,
        "a2l" => Kind::A2l,
        "dbc" => Kind::Dbc,
        _ => return None,
    })
}

pub(super) fn from_magic(head: &[u8]) -> Option<Kind> {
    const SIGNATURES: [(&[u8], Kind); 9] = [
        (b"\x7fELF", Kind::Elf),
        (b"%PDF", Kind::Pdf),
        (b"PK\x03\x04", Kind::Archive),
        (b"7z\xbc\xaf\x27\x1c", Kind::Archive),
        (b"\x1f\x8b", Kind::Archive),
        (b"\x89PNG", Kind::Image),
        (b"\xff\xd8\xff", Kind::Image),
        (b"GIF8", Kind::Image),
        (b"BM", Kind::Image),
    ];
    SIGNATURES
        .iter()
        .find(|(prefix, _)| head.starts_with(prefix))
        .map(|(_, kind)| *kind)
}

/// Text means no NUL byte in a non-empty sample.
pub(super) fn is_text(head: &[u8]) -> bool {
    !head.is_empty() && !head.contains(&0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_and_magic_kinds() {
        assert_eq!(from_extension("a2l"), Some(Kind::A2l));
        assert_eq!(from_extension("txt"), None);
        assert_eq!(from_magic(b"\x7fELF\x02"), Some(Kind::Elf));
        assert_eq!(from_magic(b"plain"), None);
    }

    #[test]
    fn text_needs_no_nul() {
        assert!(is_text(b"hello"));
        assert!(!is_text(b"he\0llo"));
        assert!(!is_text(b""));
    }
}
