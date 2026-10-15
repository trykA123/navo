use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use flate2::Compression;
use flate2::write::GzEncoder;
use sevenz_rust2::{ArchiveEntry as SevenEntry, ArchiveWriter, SourceReader};
use tempfile::TempDir;
use zip::write::SimpleFileOptions;

use super::{
    ArchiveEntry, ArchiveFormat, ENCRYPTED, ENTRY_LIMIT, Stamp, detect, list_archive,
    read_archive_entry,
};

const README: &[u8] = b"Ferrite reads archives.\n";
const DATA: &[u8] = b"\x00\x01\x02 binary payload \xFF";

fn tar_bytes(names: &[(&str, &[u8])]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    let mut folder = tar::Header::new_gnu();
    folder.set_entry_type(tar::EntryType::Directory);
    folder.set_size(0);
    folder.set_mtime(1_791_920_400);
    builder
        .append_data(&mut folder, "docs/", std::io::empty())
        .unwrap();
    for (name, data) in names {
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mtime(1_791_920_400);
        header.set_mode(0o644);
        builder.append_data(&mut header, name, *data).unwrap();
    }
    builder.into_inner().unwrap()
}

fn sample_tar() -> Vec<u8> {
    tar_bytes(&[("docs/readme.txt", README), ("docs/deep/data.bin", DATA)])
}

fn zip_bytes() -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default();
    writer.add_directory("docs/", options).unwrap();
    writer.start_file("docs/readme.txt", options).unwrap();
    writer.write_all(README).unwrap();
    writer.start_file("docs/deep/data.bin", options).unwrap();
    writer.write_all(DATA).unwrap();
    writer.finish().unwrap().into_inner()
}

fn seven_bytes(solid: bool) -> Vec<u8> {
    let mut writer = ArchiveWriter::new(Cursor::new(Vec::new())).unwrap();
    writer
        .push_archive_entry::<&[u8]>(SevenEntry::new_directory("docs"), None)
        .unwrap();
    let files = [("docs/readme.txt", README), ("docs/deep/data.bin", DATA)];
    if solid {
        let entries = files.iter().map(|(name, _)| SevenEntry::new_file(name));
        let readers = files.iter().map(|(_, data)| SourceReader::from(*data));
        writer
            .push_archive_entries(entries.collect(), readers.collect())
            .unwrap();
    } else {
        for (name, data) in files {
            writer
                .push_archive_entry(SevenEntry::new_file(name), Some(data))
                .unwrap();
        }
    }
    writer.finish().unwrap().into_inner()
}

fn gz(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}

fn xz(bytes: &[u8]) -> Vec<u8> {
    let options = lzma_rust2::XzOptions::with_preset(1);
    let mut writer = lzma_rust2::XzWriter::new(Vec::new(), options).unwrap();
    writer.write_all(bytes).unwrap();
    writer.finish().unwrap()
}

fn zst(bytes: &[u8]) -> Vec<u8> {
    ruzstd::encoding::compress_to_vec(bytes, ruzstd::encoding::CompressionLevel::Fastest)
}

fn write(dir: &TempDir, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.path().join(name);
    fs::write(&path, bytes).unwrap();
    path
}

fn every_format(dir: &TempDir) -> Vec<(ArchiveFormat, PathBuf)> {
    vec![
        (ArchiveFormat::Zip, write(dir, "a.zip", &zip_bytes())),
        (ArchiveFormat::Tar, write(dir, "a.tar", &sample_tar())),
        (
            ArchiveFormat::TarGz,
            write(dir, "a.tgz", &gz(&sample_tar())),
        ),
        (
            ArchiveFormat::TarXz,
            write(dir, "a.tar.xz", &xz(&sample_tar())),
        ),
        (
            ArchiveFormat::TarZst,
            write(dir, "a.tar.zst", &zst(&sample_tar())),
        ),
        (
            ArchiveFormat::SevenZip,
            write(dir, "a.7z", &seven_bytes(false)),
        ),
        (
            ArchiveFormat::SevenZip,
            write(dir, "solid.7z", &seven_bytes(true)),
        ),
    ]
}

fn names(entries: &[ArchiveEntry]) -> Vec<(&str, bool)> {
    entries
        .iter()
        .map(|entry| (entry.name.trim_end_matches('/'), entry.is_folder))
        .collect()
}

fn index_of(path: &Path, name: &str) -> usize {
    list_archive(path)
        .unwrap()
        .entries
        .iter()
        .position(|entry| entry.name == name)
        .unwrap()
}

#[test]
fn every_format_lists_nested_folders_and_reads_an_entry() {
    let dir = tempfile::tempdir().unwrap();
    for (format, path) in every_format(&dir) {
        let listing = list_archive(&path).unwrap();
        assert_eq!(listing.format, format, "{}", path.display());
        let mut listed = names(&listing.entries);
        listed.sort_unstable();
        assert_eq!(
            listed,
            [
                ("docs", true),
                ("docs/deep/data.bin", false),
                ("docs/readme.txt", false)
            ],
            "{}",
            path.display()
        );
        let readme = index_of(&path, "docs/readme.txt");
        assert_eq!(listing.entries[readme].size, README.len() as u64);
        assert_eq!(
            read_archive_entry(&path, readme, ENTRY_LIMIT).unwrap(),
            README
        );
        let data = index_of(&path, "docs/deep/data.bin");
        assert_eq!(read_archive_entry(&path, data, ENTRY_LIMIT).unwrap(), DATA);
    }
}

#[test]
fn zip_entries_carry_packed_size_and_a_local_date() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(&dir, "a.zip", &zip_bytes());
    let listing = list_archive(&path).unwrap();
    let readme = &listing.entries[index_of(&path, "docs/readme.txt")];
    assert!(readme.packed.is_some());
    assert!(matches!(readme.modified, Some(Stamp::Local(_))));
    let tar = write(&dir, "a.tar", &sample_tar());
    let entry = &list_archive(&tar).unwrap().entries[1];
    assert_eq!(entry.modified, Some(Stamp::UnixMs(1_791_920_400_000)));
    assert_eq!(entry.packed, None);
}

#[test]
fn an_entry_over_the_cap_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    for (_, path) in every_format(&dir) {
        let readme = index_of(&path, "docs/readme.txt");
        let error = read_archive_entry(&path, readme, 4).unwrap_err();
        assert!(error.contains("too large"), "{}: {error}", path.display());
    }
}

#[test]
fn a_folder_or_a_missing_index_is_not_read() {
    let dir = tempfile::tempdir().unwrap();
    for (_, path) in every_format(&dir) {
        let entries = list_archive(&path).unwrap().entries;
        let folder = entries.iter().position(|entry| entry.is_folder).unwrap();
        assert_eq!(
            read_archive_entry(&path, folder, ENTRY_LIMIT).unwrap_err(),
            "This is a folder."
        );
        assert!(read_archive_entry(&path, 99, ENTRY_LIMIT).is_err());
    }
}

#[test]
fn corrupt_archives_error_and_never_panic() {
    let dir = tempfile::tempdir().unwrap();
    let mut zip = zip_bytes();
    let half = zip.len() / 2;
    zip.truncate(half);
    let mut seven = seven_bytes(false);
    seven.truncate(40);
    let mut tgz = gz(&sample_tar());
    tgz.truncate(30);
    let cases = [
        write(&dir, "cut.zip", &zip),
        write(&dir, "junk.zip", b"PK\x03\x04 then nothing useful at all"),
        write(&dir, "cut.7z", &seven),
        write(&dir, "cut.tar.gz", &tgz),
        write(&dir, "junk.tar.xz", b"\xFD7zXZ\x00 not really xz"),
        write(&dir, "junk.tar.zst", b"\x28\xB5\x2F\xFD garbage"),
        write(&dir, "plain.txt", b"just words"),
    ];
    for path in cases {
        let listed = list_archive(&path).map(|listing| listing.entries.len());
        let read = read_archive_entry(&path, 1, ENTRY_LIMIT);
        assert!(listed.is_err() || read.is_err(), "{}", path.display());
    }
}

#[test]
fn an_encrypted_zip_entry_is_flagged_and_not_read() {
    let mut bytes = zip_bytes();
    let flags = |signature: &[u8], offset: usize, bytes: &mut Vec<u8>| {
        let starts: Vec<usize> = bytes
            .windows(4)
            .enumerate()
            .filter(|(_, window)| *window == signature)
            .map(|(at, _)| at + offset)
            .collect();
        for at in starts {
            bytes[at] |= 1;
        }
    };
    flags(b"PK\x03\x04", 6, &mut bytes);
    flags(b"PK\x01\x02", 8, &mut bytes);
    let dir = tempfile::tempdir().unwrap();
    let path = write(&dir, "locked.zip", &bytes);
    let listing = list_archive(&path).unwrap();
    let readme = index_of(&path, "docs/readme.txt");
    assert!(listing.entries[readme].is_encrypted);
    assert_eq!(
        read_archive_entry(&path, readme, ENTRY_LIMIT).unwrap_err(),
        ENCRYPTED
    );
}

#[test]
fn a_path_traversal_name_is_listed_as_is_and_never_touches_the_disk() {
    let mut header = tar::Header::new_gnu();
    header.set_size(4);
    header.as_gnu_mut().unwrap().name[..7].copy_from_slice(b"../evil");
    header.set_cksum();
    let mut builder = tar::Builder::new(Vec::new());
    builder.append(&header, &b"boom"[..]).unwrap();
    let bytes = builder.into_inner().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let inner = dir.path().join("inner");
    fs::create_dir(&inner).unwrap();
    let path = inner.join("evil.tar");
    fs::write(&path, bytes).unwrap();
    let listing = list_archive(&path).unwrap();
    assert_eq!(listing.entries[0].name, "../evil");
    assert_eq!(read_archive_entry(&path, 0, ENTRY_LIMIT).unwrap(), b"boom");
    assert!(!dir.path().join("evil").exists());
    assert_eq!(fs::read_dir(&inner).unwrap().count(), 1);
}

#[test]
fn detection_reads_magic_and_tar_names() {
    let tar = sample_tar();
    assert_eq!(detect(&zip_bytes(), "x.jar"), Some(ArchiveFormat::Zip));
    assert_eq!(
        detect(&seven_bytes(false), "x"),
        Some(ArchiveFormat::SevenZip)
    );
    assert_eq!(detect(&tar, "x"), Some(ArchiveFormat::Tar));
    assert_eq!(detect(&gz(&tar), "x.gz"), Some(ArchiveFormat::TarGz));
    assert_eq!(detect(&xz(&tar), "x"), Some(ArchiveFormat::TarXz));
    assert_eq!(detect(&zst(&tar), "x"), Some(ArchiveFormat::TarZst));
    assert_eq!(detect(&gz(b"just a log"), "log.gz"), None);
    assert_eq!(
        detect(&gz(b"just a log"), "log.tar.gz"),
        Some(ArchiveFormat::TarGz)
    );
    assert_eq!(detect(b"plain words", "notes.txt"), None);
    assert_eq!(detect(b"MZ\x90\x00", "setup.exe"), None);
}
