use std::io::Read;

use super::ArchiveFormat;
use super::tarball::decompress;

const SEVEN_ZIP: &[u8] = b"7z\xBC\xAF\x27\x1C";
const GZIP: &[u8] = b"\x1F\x8B";
const XZ: &[u8] = b"\xFD7zXZ\x00";
const ZSTD: &[u8] = b"\x28\xB5\x2F\xFD";
const USTAR_AT: usize = 257;
const BLOCK: usize = 512;

const TAR_NAMES: [(ArchiveFormat, &[&str]); 3] = [
    (ArchiveFormat::TarGz, &[".tar.gz", ".tgz"]),
    (ArchiveFormat::TarXz, &[".tar.xz", ".txz"]),
    (ArchiveFormat::TarZst, &[".tar.zst", ".tar.zstd", ".tzst"]),
];

fn is_zip(head: &[u8]) -> bool {
    head.starts_with(b"PK\x03\x04") || head.starts_with(b"PK\x05\x06")
}

fn is_ustar(block: &[u8]) -> bool {
    block
        .get(USTAR_AT..USTAR_AT + 5)
        .is_some_and(|magic| magic == b"ustar")
}

fn compressed(head: &[u8]) -> Option<ArchiveFormat> {
    if head.starts_with(GZIP) {
        Some(ArchiveFormat::TarGz)
    } else if head.starts_with(XZ) {
        Some(ArchiveFormat::TarXz)
    } else if head.starts_with(ZSTD) {
        Some(ArchiveFormat::TarZst)
    } else {
        None
    }
}

fn holds_tar(format: ArchiveFormat, head: &[u8]) -> bool {
    let mut block = [0; BLOCK];
    decompress(format, head)
        .is_ok_and(|mut reader| reader.read_exact(&mut block).is_ok() && is_ustar(&block))
}

fn has_tar_name(format: ArchiveFormat, name: &str) -> bool {
    TAR_NAMES
        .iter()
        .filter(|(known, _)| *known == format)
        .flat_map(|(_, endings)| endings.iter())
        .any(|ending| name.ends_with(ending))
}

#[must_use]
pub fn detect(head: &[u8], name: &str) -> Option<ArchiveFormat> {
    let name = name.to_ascii_lowercase();
    if is_zip(head) {
        return Some(ArchiveFormat::Zip);
    }
    if head.starts_with(SEVEN_ZIP) {
        return Some(ArchiveFormat::SevenZip);
    }
    if let Some(format) = compressed(head) {
        return (holds_tar(format, head) || has_tar_name(format, &name)).then_some(format);
    }
    (is_ustar(head) || name.ends_with(".tar")).then_some(ArchiveFormat::Tar)
}
