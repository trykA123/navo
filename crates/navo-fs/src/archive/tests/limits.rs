use std::io::{self, Cursor, Read, Seek, SeekFrom, Write as _};
use std::sync::atomic::AtomicBool;

use tar::{EntryType, Header};
use zip::write::SimpleFileOptions;

use super::{NEVER, gz, index_of, read_archive_entry, sample_tar, seven_bytes, tar_bytes, write};
use crate::archive::{
    ArchiveFormat, CANCELLED, ENTRY_LIMIT, EntryAt, Limits, OVER_BUDGET, UNPACK_BUDGET,
    UNSUPPORTED_METHOD, list_with, read_with, sevenz, tarball,
};

const ZEROS: usize = 1024 * 1024;
const COPIES: usize = 4096;

fn limits(entries: usize, budget: u64, cancel: &AtomicBool) -> Limits<'_> {
    Limits {
        entries,
        budget,
        cancel,
    }
}

fn header_declaring(kind: EntryType, size: u64) -> Vec<u8> {
    let mut header = Header::new_gnu();
    header.set_entry_type(kind);
    header.set_size(size);
    header.as_gnu_mut().unwrap().name[..13].copy_from_slice(b"././@LongLink");
    header.set_cksum();
    header.as_bytes().to_vec()
}

fn gzip_bomb_after(head: &[u8]) -> Vec<u8> {
    let mut bomb = gz(head);
    let zeros = gz(&vec![0; ZEROS]);
    for _ in 0..COPIES {
        bomb.extend_from_slice(&zeros);
    }
    bomb
}

#[test]
fn a_huge_long_name_link_or_pax_body_is_refused_before_it_is_read() {
    let dir = tempfile::tempdir().unwrap();
    for kind in [
        EntryType::GNULongName,
        EntryType::GNULongLink,
        EntryType::XHeader,
    ] {
        let declared = (ZEROS * COPIES) as u64;
        let path = write(
            &dir,
            "bomb.tgz",
            &gzip_bomb_after(&header_declaring(kind, declared)),
        );
        let error = super::list_archive(&path).unwrap_err();
        assert!(error.contains("over 64 KB"), "{kind:?}: {error}");
    }
}

#[test]
fn a_long_name_and_a_pax_path_name_the_next_entry() {
    let long = format!("docs/{}/notes.txt", "deep".repeat(40));
    let mut builder = tar::Builder::new(Vec::new());
    let mut header = Header::new_gnu();
    header.set_size(4);
    builder
        .append_data(&mut header, &long, &b"note"[..])
        .unwrap();
    let mut header = Header::new_ustar();
    header.set_size(3);
    builder
        .append_pax_extensions([("path", b"pax/named.txt".as_slice())])
        .unwrap();
    builder
        .append_data(&mut header, "short", &b"pax"[..])
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = write(&dir, "names.tar.gz", &gz(&builder.into_inner().unwrap()));
    let listing = super::list_archive(&path).unwrap();
    let names: Vec<_> = listing.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, [long.as_str(), "pax/named.txt"]);
    assert_eq!(read_archive_entry(&path, 1, ENTRY_LIMIT).unwrap(), b"pax");
}

#[test]
fn a_listing_stops_at_the_entry_cap_and_says_so() {
    let dir = tempfile::tempdir().unwrap();
    for (_, path) in super::every_format(&dir) {
        let capped = list_with(&path, limits(2, u64::MAX, &NEVER)).unwrap();
        assert_eq!(capped.entries.len(), 2, "{}", path.display());
        assert!(capped.truncated, "{}", path.display());
        let whole = list_with(&path, limits(3, u64::MAX, &NEVER)).unwrap();
        assert_eq!(whole.entries.len(), 3, "{}", path.display());
        assert!(!whole.truncated, "{}", path.display());
    }
}

#[test]
fn a_listing_over_the_unpack_budget_is_partial_not_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(&dir, "a.tgz", &gz(&sample_tar()));
    let partial = list_with(&path, limits(10, 1024, &NEVER)).unwrap();
    assert!(partial.truncated);
    assert!(!partial.entries.is_empty() && partial.entries.len() < 3);
}

#[test]
fn a_cancelled_listing_or_read_stops() {
    let cancel = AtomicBool::new(true);
    let dir = tempfile::tempdir().unwrap();
    for (_, path) in super::every_format(&dir) {
        let listing = list_with(&path, limits(10, u64::MAX, &cancel)).unwrap();
        assert!(
            listing.truncated && listing.entries.is_empty(),
            "{}",
            path.display()
        );
    }
    let at = EntryAt {
        index: 1,
        limit: ENTRY_LIMIT,
    };
    for (name, bytes) in [("a.tgz", gz(&sample_tar())), ("a.tar", sample_tar())] {
        let path = write(&dir, name, &bytes);
        let read = read_with(&path, at, limits(10, u64::MAX, &cancel));
        assert_eq!(read.unwrap_err(), CANCELLED, "{name}");
    }
}

#[test]
fn reading_past_other_entries_in_a_solid_7z_block_keeps_to_the_budget() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(&dir, "solid.7z", &seven_bytes(true));
    let errors: Vec<_> = ["docs/readme.txt", "docs/deep/data.bin"]
        .into_iter()
        .map(|name| {
            let at = EntryAt {
                index: index_of(&path, name),
                limit: ENTRY_LIMIT,
            };
            read_with(&path, at, limits(10, 4, &NEVER)).err()
        })
        .collect();
    assert!(
        errors
            .iter()
            .any(|error| error.as_deref() == Some(OVER_BUDGET)),
        "{errors:?}"
    );
}

struct Counting<R> {
    inner: R,
    read: u64,
}

impl<R: Read> Read for Counting<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let read = self.inner.read(buf)?;
        self.read += read as u64;
        Ok(read)
    }
}

impl<R: Seek> Seek for Counting<R> {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        self.inner.seek(to)
    }
}

#[test]
fn a_plain_tar_listing_seeks_past_entry_data() {
    let big = vec![7; 8 * ZEROS];
    let bytes = tar_bytes(&[("big.bin", &big), ("after.txt", b"after")]);
    let mut counting = Counting {
        inner: Cursor::new(bytes),
        read: 0,
    };
    let listed = tarball::list(
        ArchiveFormat::Tar,
        &mut counting,
        limits(10, u64::MAX, &NEVER),
    )
    .unwrap();
    assert_eq!(listed.entries.len(), 3);
    assert!(counting.read < 64 * 1024, "read {} bytes", counting.read);
}

fn deflated_zip(data: &[u8]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file("big.txt", SimpleFileOptions::default())
        .unwrap();
    writer.write_all(data).unwrap();
    writer.finish().unwrap().into_inner()
}

fn patch_all(bytes: &mut [u8], signature: &[u8], offset: usize, value: &[u8]) {
    let starts: Vec<usize> = bytes
        .windows(4)
        .enumerate()
        .filter(|(_, window)| *window == signature)
        .map(|(at, _)| at + offset)
        .collect();
    for at in starts {
        bytes[at..at + value.len()].copy_from_slice(value);
    }
}

#[test]
fn a_zip_that_understates_an_entry_size_is_read_no_further_than_the_cap() {
    let mut bytes = deflated_zip(&vec![b'a'; 4096]);
    patch_all(&mut bytes, b"PK\x01\x02", 24, &16u32.to_le_bytes());
    patch_all(&mut bytes, b"PK\x03\x04", 22, &16u32.to_le_bytes());
    let dir = tempfile::tempdir().unwrap();
    let path = write(&dir, "liar.zip", &bytes);
    assert_eq!(super::list_archive(&path).unwrap().entries[0].size, 16);
    match read_archive_entry(&path, 0, 64) {
        Ok(read) => assert!(read.len() <= 64, "read {} bytes", read.len()),
        Err(error) => assert!(error.contains("too large") || error.contains("damaged")),
    }
}

#[test]
fn unsupported_compression_methods_say_so() {
    let mut bytes = deflated_zip(b"some words to pack");
    patch_all(&mut bytes, b"PK\x03\x04", 8, &12u16.to_le_bytes());
    patch_all(&mut bytes, b"PK\x01\x02", 10, &12u16.to_le_bytes());
    let dir = tempfile::tempdir().unwrap();
    let path = write(&dir, "bzip2.zip", &bytes);
    assert_eq!(
        read_archive_entry(&path, 0, ENTRY_LIMIT).unwrap_err(),
        UNSUPPORTED_METHOD
    );
    let bcj2 = sevenz_rust2::Error::UnsupportedCompressionMethod("BCJ2".to_owned());
    assert_eq!(sevenz::failed(bcj2), UNSUPPORTED_METHOD);
}

fn sparse_header(chunks: &[(u64, u64)], stored: u64, real: u64, is_extended: bool) -> Vec<u8> {
    let mut header = Header::new_gnu();
    header.set_entry_type(EntryType::GNUSparse);
    header.set_size(stored);
    header.set_mode(0o644);
    header.as_gnu_mut().unwrap().name[..10].copy_from_slice(b"sparse.bin");
    let gnu = header.as_gnu_mut().unwrap();
    for (slot, (offset, length)) in gnu.sparse.iter_mut().zip(chunks) {
        slot.set_offset(*offset);
        slot.set_length(*length);
    }
    gnu.set_real_size(real);
    gnu.set_is_extended(is_extended);
    header.set_cksum();
    header.as_bytes().to_vec()
}

fn extension(chunks: &[(u64, u64)], is_extended: bool) -> Vec<u8> {
    let mut block = tar::GnuExtSparseHeader::new();
    for (slot, (offset, length)) in block.sparse_mut().iter_mut().zip(chunks) {
        slot.set_offset(*offset);
        slot.set_length(*length);
    }
    block.set_is_extended(is_extended);
    block.as_bytes().to_vec()
}

#[test]
fn a_gnu_sparse_entry_with_an_extended_block_is_listed_and_read() {
    let mut bytes = sparse_header(&[(0, 512), (1024, 512)], 1536, 2560, true);
    bytes.extend(extension(&[(2048, 512)], false));
    for fill in *b"abc" {
        bytes.extend([fill; 512]);
    }
    bytes.extend([0; 1024]);
    let dir = tempfile::tempdir().unwrap();
    let path = write(&dir, "sparse.tar", &bytes);
    let listing = super::list_archive(&path).unwrap();
    assert_eq!(listing.entries.len(), 1);
    assert_eq!(
        (listing.entries[0].name.as_str(), listing.entries[0].size),
        ("sparse.bin", 2560)
    );
    let read = read_archive_entry(&path, 0, ENTRY_LIMIT).unwrap();
    let mut expected = vec![0; 2560];
    expected[..512].fill(b'a');
    expected[1024..1536].fill(b'b');
    expected[2048..2560].fill(b'c');
    assert_eq!(read, expected);
}

#[test]
fn an_endless_sparse_header_chain_is_refused() {
    let mut bytes = sparse_header(&[(0, 1)], 1, 1, true);
    for _ in 0..3_000 {
        bytes.extend(extension(&[], true));
    }
    let dir = tempfile::tempdir().unwrap();
    for (name, archive) in [("chain.tar", bytes.clone()), ("chain.tgz", gz(&bytes))] {
        let path = write(&dir, name, &archive);
        let error = super::list_archive(&path).unwrap_err();
        assert!(
            error.contains("sparse header chain is too long"),
            "{name}: {error}"
        );
    }
}

#[test]
fn a_7z_dictionary_over_the_budget_is_refused() {
    let lzma2 = sevenz_rust2::EncoderMethod::ID_LZMA2;
    let lzma = sevenz_rust2::EncoderMethod::ID_LZMA;
    assert_eq!(sevenz::dictionary_size(lzma2, &[0]), 4096);
    assert_eq!(sevenz::dictionary_size(lzma2, &[40]), u64::from(u32::MAX));
    assert!(sevenz::dictionary_size(lzma2, &[39]) > UNPACK_BUDGET);
    assert_eq!(sevenz::dictionary_size(lzma, &[0x5D, 0, 0, 0, 1]), 1 << 24);
    assert_eq!(sevenz::dictionary_size(b"\x04\x01\x08", &[]), 0);
}
