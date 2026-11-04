use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use navo_core::{FsError, FsErrorKind, Location};
use tempfile::TempDir;
use zip::write::SimpleFileOptions;

use crate::{
    CancelToken, CopyControl, EntryBatch, EntryKind, FsEntry, OwnedCopy, Provider, ProviderOps,
    SourceOps, StreamWrite,
};

use super::*;
use crate::archive::{
    ArchiveFs, ArchiveKind, ArchiveProblem, READ_ONLY, UnsafeReason, location, provider_id,
    raw_location,
};

#[cfg(target_os = "linux")]
use std::{
    sync::{Arc, Mutex},
    time::Instant,
};

#[cfg(target_os = "linux")]
use crate::archive::provider::Clock;

fn at(archive: &Path, inner: &str) -> Location {
    location(archive, inner).unwrap()
}

fn listed(provider: &ArchiveFs, folder: &Location) -> Vec<FsEntry> {
    let mut entries = Vec::new();
    provider
        .list(
            folder,
            &mut |batch: EntryBatch| {
                entries.extend(batch.entries);
                ControlFlow::Continue(())
            },
            &CancelToken::new(),
        )
        .unwrap();
    entries
}

fn find<'a>(entries: &'a [FsEntry], name: &str) -> &'a FsEntry {
    entries
        .iter()
        .find(|entry| entry.name.to_text() == name)
        .unwrap_or_else(|| panic!("missing entry {name:?}"))
}

fn read(provider: &ArchiveFs, location: &Location) -> Vec<u8> {
    let mut bytes = Vec::new();
    provider
        .read(location)
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    bytes
}

fn zip_with(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default();
    for (name, data) in entries {
        writer.start_file(*name, options).unwrap();
        writer.write_all(data).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn tar_named(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    for (name, data) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.as_gnu_mut().unwrap().name[..name.len()].copy_from_slice(name.as_bytes());
        header.set_cksum();
        builder.append(&header, *data).unwrap();
    }
    builder.into_inner().unwrap()
}

fn write_archive(dir: &TempDir, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

fn check_tree(provider: &ArchiveFs, archive: &Path) {
    let root = at(archive, "");
    let top = listed(provider, &root);
    let docs = find(&top, "docs");
    assert_eq!(docs.kind, EntryKind::Folder);
    assert!(docs.attributes.readonly);
    let inside = listed(provider, &at(archive, "docs"));
    assert_eq!(find(&inside, "readme.txt").size, README.len() as u64);
    assert_eq!(find(&inside, "deep").kind, EntryKind::Folder);
    assert_eq!(
        read(provider, &at(archive, "docs/readme.txt")),
        README.to_vec()
    );
    assert_eq!(
        read(provider, &at(archive, "docs/deep/data.bin")),
        DATA.to_vec()
    );
    let root_entry = <ArchiveFs as Provider>::metadata(provider, &root).unwrap();
    assert_eq!(root_entry.kind, EntryKind::Folder);
    assert_eq!(
        root_entry.name.to_text(),
        archive.file_name().unwrap().to_string_lossy().to_string()
    );
    assert_eq!(
        root.join(&navo_core::ProviderPath::from_text("docs"))
            .unwrap(),
        at(archive, "docs")
    );
}

#[test]
fn zip_sevenz_and_tar_gz_list_nested_folders_and_read_files() {
    let dir = tempfile::tempdir().unwrap();
    let provider = ArchiveFs::new();
    assert_eq!(<ArchiveFs as Provider>::id(&provider), provider_id());

    let zip = write_archive(&dir, "a.zip", &zip_bytes());
    check_tree(&provider, &zip);
    let tar = write_archive(&dir, "a.tar.gz", &gz(&sample_tar()));
    check_tree(&provider, &tar);
    let seven = write_archive(&dir, "a.7z", &seven_bytes(false));
    check_tree(&provider, &seven);
}

#[test]
fn tar_xz_and_tar_zst_use_the_same_readers() {
    let dir = tempfile::tempdir().unwrap();
    let provider = ArchiveFs::new();
    for (name, bytes) in [
        ("a.tar.xz", xz(&sample_tar())),
        ("a.tar.zst", zst(&sample_tar())),
    ] {
        let archive = write_archive(&dir, name, &bytes);
        check_tree(&provider, &archive);
    }
}

#[test]
fn folders_implied_by_entry_paths_appear() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = tar_named(&[("a/b/c.txt", b"deep")]);
    let archive = write_archive(&dir, "implied.tar", &bytes);
    let provider = ArchiveFs::new();
    let top = listed(&provider, &at(&archive, ""));
    assert_eq!(find(&top, "a").kind, EntryKind::Folder);
    let a = listed(&provider, &at(&archive, "a"));
    assert_eq!(find(&a, "b").kind, EntryKind::Folder);
    let file = listed(&provider, &at(&archive, "a/b"));
    assert_eq!(find(&file, "c.txt").kind, EntryKind::File);
    assert_eq!(read(&provider, &at(&archive, "a/b/c.txt")), b"deep");
}

fn refused(provider: &ArchiveFs, at: &Location) -> ArchiveProblem {
    let error = provider.read(at).err().expect("the read was accepted");
    assert_eq!(error.kind, FsErrorKind::NameInvalid, "{}", at.as_str());
    provider.last_problem(at).expect("the problem was dropped")
}

#[test]
fn unsafe_names_are_listed_and_refused_for_copy() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = tar_named(&[
        ("../evil", b"parent"),
        ("/absolute", b"rooted"),
        ("C:drive", b"drive"),
        ("CON", b"device"),
    ]);
    let archive = write_archive(&dir, "unsafe.tar", &bytes);
    let provider = ArchiveFs::new();
    let top = listed(&provider, &at(&archive, ""));
    for (name, reason) in [
        ("../evil", UnsafeReason::Parent),
        ("/absolute", UnsafeReason::Absolute),
        ("C:drive", UnsafeReason::Drive),
        ("CON", UnsafeReason::WindowsName),
    ] {
        find(&top, name);
        let raw = raw_location(&archive, name).unwrap();
        match refused(&provider, &raw) {
            ArchiveProblem::UnsafeName { reason: found, .. } => assert_eq!(found, reason),
            other => panic!("unexpected problem {other:?}"),
        }
        let metadata = <ArchiveFs as Provider>::metadata(&provider, &raw).unwrap();
        assert_eq!(metadata.kind, EntryKind::File);
        let error = <ArchiveFs as SourceOps>::metadata(&provider, &raw)
            .err()
            .unwrap();
        assert_eq!(error.kind, FsErrorKind::NameInvalid);
    }
}

#[test]
fn a_nul_name_is_listed_and_refused() {
    let dir = tempfile::tempdir().unwrap();
    let archive = write_archive(&dir, "nul.zip", &zip_with(&[("nul\0name", b"x")]));
    let provider = ArchiveFs::new();
    let top = listed(&provider, &at(&archive, ""));
    assert_eq!(find(&top, "nul\0name").kind, EntryKind::File);
    let raw = raw_location(&archive, "nul\0name").unwrap();
    match refused(&provider, &raw) {
        ArchiveProblem::UnsafeName { reason, .. } => assert_eq!(reason, UnsafeReason::Nul),
        other => panic!("unexpected problem {other:?}"),
    }
}

#[test]
fn a_nested_archive_is_a_file_and_is_not_entered() {
    let inner = zip_with(&[("inside.txt", b"inner")]);
    let outer = zip_with(&[("nested.zip", &inner)]);
    let dir = tempfile::tempdir().unwrap();
    let archive = write_archive(&dir, "outer.zip", &outer);
    let provider = ArchiveFs::new();
    let top = listed(&provider, &at(&archive, ""));
    assert_eq!(find(&top, "nested.zip").kind, EntryKind::File);
    assert_eq!(read(&provider, &at(&archive, "nested.zip")), inner);
    let error = provider
        .list(
            &at(&archive, "nested.zip"),
            &mut |_: EntryBatch| ControlFlow::Continue(()),
            &CancelToken::new(),
        )
        .unwrap_err();
    assert_eq!(error.kind, FsErrorKind::NotSupportedYet);
}

#[test]
fn a_successful_parse_clears_the_last_problem() {
    let dir = tempfile::tempdir().unwrap();
    let archive = write_archive(&dir, "a.zip", &zip_bytes());
    let provider = ArchiveFs::new();
    let file = at(&archive, "docs/readme.txt");
    provider.restrict_to_owner(&file).unwrap_err();
    assert_eq!(provider.last_problem(&file), Some(ArchiveProblem::ReadOnly));
    <ArchiveFs as Provider>::metadata(&provider, &file).unwrap();
    assert_eq!(provider.last_problem(&file), None);
}

#[test]
fn cancelling_metadata_stops_the_archive_load() {
    let dir = tempfile::tempdir().unwrap();
    let archive = write_archive(&dir, "a.zip", &zip_bytes());
    let provider = ArchiveFs::new();
    let _ = listed(&provider, &at(&archive, ""));
    let file = at(&archive, "docs/readme.txt");
    let cancel = CancelToken::new();
    cancel.cancel();
    let error = <ArchiveFs as SourceOps>::metadata_with_cancel(&provider, &file, &cancel)
        .expect_err("metadata was accepted");
    assert_eq!(error.kind, FsErrorKind::Cancelled);
}

#[test]
fn an_entry_that_normalizes_to_the_root_is_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let archive = write_archive(&dir, "root.tar", &tar_named(&[("./", b"")]));
    let provider = ArchiveFs::new();
    assert!(listed(&provider, &at(&archive, "")).is_empty());
}

fn tar_special(entries: &[(tar::EntryType, &str)]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    for (kind, name) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(*kind);
        header.set_size(0);
        header.set_mode(0o644);
        header.as_gnu_mut().unwrap().name[..name.len()].copy_from_slice(name.as_bytes());
        if matches!(kind, tar::EntryType::Symlink | tar::EntryType::Link) {
            header.set_link_name("target.txt").unwrap();
        }
        header.set_cksum();
        builder.append(&header, std::io::empty()).unwrap();
    }
    builder.into_inner().unwrap()
}

#[test]
fn tar_special_entries_are_listed_and_refused() {
    let dir = tempfile::tempdir().unwrap();
    let archive = write_archive(
        &dir,
        "special.tar",
        &tar_special(&[
            (tar::EntryType::Symlink, "link"),
            (tar::EntryType::Link, "hard"),
            (tar::EntryType::Char, "char"),
            (tar::EntryType::Block, "block"),
            (tar::EntryType::Fifo, "fifo"),
        ]),
    );
    let provider = ArchiveFs::new();
    let top = listed(&provider, &at(&archive, ""));
    for (name, kind) in [
        ("link", ArchiveKind::Symlink),
        ("hard", ArchiveKind::Hardlink),
        ("char", ArchiveKind::CharDevice),
        ("block", ArchiveKind::BlockDevice),
        ("fifo", ArchiveKind::Fifo),
    ] {
        find(&top, name);
        let row = at(&archive, name);
        let error = provider.read(&row).err().expect("the read was accepted");
        assert_eq!(error.kind, FsErrorKind::NotSupportedYet, "{name}");
        assert_eq!(
            provider.last_problem(&row),
            Some(ArchiveProblem::Special {
                name: name.to_owned(),
                kind
            })
        );
    }
}

#[cfg(target_os = "linux")]
fn open_files(path: &Path) -> usize {
    std::fs::read_dir("/proc/self/fd")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| std::fs::read_link(entry.path()).ok())
        .filter(|target| target == path)
        .count()
}

#[cfg(target_os = "linux")]
#[test]
fn a_zip_keeps_one_open_reader_for_its_entries() {
    let dir = tempfile::tempdir().unwrap();
    let archive = write_archive(&dir, "a.zip", &zip_bytes());
    let provider = ArchiveFs::new();
    let _ = listed(&provider, &at(&archive, ""));
    assert_eq!(open_files(&archive), 1);
    assert_eq!(read(&provider, &at(&archive, "docs/readme.txt")), README);
    assert_eq!(read(&provider, &at(&archive, "docs/deep/data.bin")), DATA);
    assert_eq!(open_files(&archive), 1);
}

#[cfg(target_os = "linux")]
#[test]
fn an_idle_zip_reader_is_dropped_and_reopened_on_the_next_read() {
    let dir = tempfile::tempdir().unwrap();
    let archive = write_archive(&dir, "a.zip", &zip_bytes());
    let now = Arc::new(Mutex::new(Instant::now()));
    let clock_now = Arc::clone(&now);
    let clock: Clock = Arc::new(move || *clock_now.lock().unwrap());
    let provider = ArchiveFs::with_clock(clock);
    let root = at(&archive, "");
    let _ = listed(&provider, &root);
    assert_eq!(open_files(&archive), 1);

    {
        let mut at = now.lock().unwrap();
        *at += Duration::from_secs(19);
    }
    <ArchiveFs as Provider>::metadata(&provider, &root).unwrap();
    assert_eq!(open_files(&archive), 1);
    assert_eq!(read(&provider, &at(&archive, "docs/readme.txt")), README);
    assert_eq!(open_files(&archive), 1);

    {
        let mut at = now.lock().unwrap();
        *at += Duration::from_secs(21);
    }
    <ArchiveFs as Provider>::metadata(&provider, &root).unwrap();
    assert_eq!(open_files(&archive), 0);
    assert_eq!(read(&provider, &at(&archive, "docs/deep/data.bin")), DATA);
    assert_eq!(open_files(&archive), 1);
}

#[cfg(target_os = "linux")]
#[test]
fn refresh_drops_the_zip_reader() {
    let dir = tempfile::tempdir().unwrap();
    let archive = write_archive(&dir, "a.zip", &zip_bytes());
    let provider = ArchiveFs::new();
    let _ = listed(&provider, &at(&archive, ""));
    assert_eq!(open_files(&archive), 1);
    provider.refresh();
    assert_eq!(open_files(&archive), 0);
}

#[test]
fn a_plain_tar_entry_reads_from_its_header_offset() {
    let dir = tempfile::tempdir().unwrap();
    let archive = write_archive(
        &dir,
        "offset.tar",
        &tar_named(&[("first.txt", b"first"), ("second.txt", b"second")]),
    );
    let provider = ArchiveFs::new();
    assert_eq!(read(&provider, &at(&archive, "second.txt")), b"second");

    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&archive)
        .unwrap();
    let modified = file.metadata().unwrap().modified().unwrap();
    file.seek(SeekFrom::Start(0)).unwrap();
    file.write_all(&[0xFF; 512]).unwrap();
    file.set_modified(modified).unwrap();
    assert_eq!(read(&provider, &at(&archive, "second.txt")), b"second");
}

#[test]
fn duplicate_names_keep_the_last_entry_and_are_marked() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = tar_named(&[("dup.txt", b"first"), ("dup.txt", b"second")]);
    let archive = write_archive(&dir, "dup.tar", &bytes);
    let provider = ArchiveFs::new();
    let top = listed(&provider, &at(&archive, ""));
    assert_eq!(
        top.iter()
            .filter(|entry| entry.name.to_text() == "dup.txt")
            .count(),
        1
    );
    let row = at(&archive, "dup.txt");
    assert_eq!(find(&top, "dup.txt").size, 6);
    assert_eq!(
        provider.last_problem(&row),
        Some(ArchiveProblem::Duplicate {
            name: "dup.txt".to_owned()
        })
    );
    assert_eq!(read(&provider, &row), b"second");
}

#[test]
fn a_changed_archive_is_read_again() {
    let dir = tempfile::tempdir().unwrap();
    let archive = write_archive(&dir, "a.zip", &zip_with(&[("a.txt", b"one")]));
    let provider = ArchiveFs::new();
    assert_eq!(read(&provider, &at(&archive, "a.txt")), b"one");

    let rewritten = zip_with(&[("a.txt", b"two")]);
    std::fs::write(&archive, &rewritten).unwrap();
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(&archive)
        .unwrap();
    file.set_modified(SystemTime::now() + Duration::from_secs(60))
        .unwrap();
    assert_eq!(read(&provider, &at(&archive, "a.txt")), b"two");
}

#[test]
fn a_broken_archive_gives_an_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let provider = ArchiveFs::new();
    let mut truncated = zip_bytes();
    truncated.truncate(truncated.len() / 2);
    let archive = write_archive(&dir, "broken.zip", &truncated);
    let root = at(&archive, "");
    let listed = provider.list(
        &root,
        &mut |_: EntryBatch| ControlFlow::Continue(()),
        &CancelToken::new(),
    );
    let read = provider.read(&at(&archive, "docs/readme.txt"));
    assert!(listed.is_err() || read.is_err());

    let rar = write_archive(&dir, "a.rar", b"Rar!\x1a\x07\x00not-for-navo");
    let rar_root = at(&rar, "");
    let error = provider
        .list(
            &rar_root,
            &mut |_: EntryBatch| ControlFlow::Continue(()),
            &CancelToken::new(),
        )
        .unwrap_err();
    assert_eq!(error.kind, FsErrorKind::NotSupportedYet);
    assert_eq!(
        provider.last_problem(&rar_root),
        Some(ArchiveProblem::NotAnArchive)
    );
}

fn encrypted_zip() -> Vec<u8> {
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
    bytes
}

#[test]
fn an_encrypted_entry_is_listed_and_not_read() {
    let dir = tempfile::tempdir().unwrap();
    let archive = write_archive(&dir, "locked.zip", &encrypted_zip());
    let provider = ArchiveFs::new();
    let inside = listed(&provider, &at(&archive, "docs"));
    assert!(find(&inside, "readme.txt").size > 0);
    let row = at(&archive, "docs/readme.txt");
    assert_eq!(
        provider.read(&row).err().unwrap().kind,
        FsErrorKind::NotSupportedYet
    );
    assert_eq!(
        provider.last_problem(&row),
        Some(ArchiveProblem::Encrypted {
            name: "docs/readme.txt".to_owned()
        })
    );
    assert_eq!(
        <ArchiveFs as SourceOps>::metadata(&provider, &row)
            .err()
            .unwrap()
            .kind,
        FsErrorKind::NotSupportedYet
    );
}

fn write_calls(
    provider: &ArchiveFs,
    file: &Location,
    folder: &Location,
) -> [Result<(), FsError>; 14] {
    let mut progress = |_| {};
    let cancel = CancelToken::new();
    let mut created = |_| Ok(());
    let request = OwnedCopy {
        source: file,
        target: folder,
        created: &mut created,
    };
    let control = CopyControl {
        progress: &mut progress,
        cancel: &cancel,
    };
    [
        provider.create_file(file),
        provider.create_folder(folder),
        provider.create_private_folder(folder),
        provider.remove_file(file),
        provider.remove_link(file),
        provider.remove_empty_folder(folder),
        provider.recycle(file),
        provider.rename(file, folder),
        provider.replace(file, folder),
        provider.rename_atomic(file, folder),
        provider
            .copy_file(file, folder, &mut |_| {}, &CancelToken::new())
            .map(drop),
        provider.make_link(file, folder),
        provider.restrict_to_owner(file),
        provider.copy_file_owned(request, control).map(drop),
    ]
}

#[test]
fn every_write_is_refused_with_the_read_only_problem() {
    let dir = tempfile::tempdir().unwrap();
    let archive = write_archive(&dir, "a.zip", &zip_bytes());
    let provider = ArchiveFs::new();
    let file = at(&archive, "docs/readme.txt");
    let folder = at(&archive, "");
    for write in write_calls(&provider, &file, &folder) {
        let error: FsError = write.expect_err("the write was accepted");
        assert_eq!(error.kind, FsErrorKind::NotSupportedYet);
    }
    let mut bytes = std::io::empty();
    let mut no_created = |_| Ok(());
    let error = provider
        .write_new(
            StreamWrite {
                bytes: &mut bytes,
                source: &file,
                target: &folder,
                created: Some(&mut no_created),
                modified: None,
            },
            CopyControl {
                progress: &mut |_| {},
                cancel: &CancelToken::new(),
            },
        )
        .expect_err("the write was accepted");
    assert_eq!(error.kind, FsErrorKind::NotSupportedYet);
    assert_eq!(READ_ONLY, "Archives are read-only");
    assert_eq!(provider.last_problem(&file), Some(ArchiveProblem::ReadOnly));
}

#[test]
fn a_cancelled_listing_stops_with_cancelled() {
    let dir = tempfile::tempdir().unwrap();
    let archive = write_archive(&dir, "a.zip", &zip_bytes());
    let provider = ArchiveFs::new();
    let cancel = CancelToken::new();
    cancel.cancel();
    let error = provider
        .list(
            &at(&archive, ""),
            &mut |_: EntryBatch| ControlFlow::Continue(()),
            &cancel,
        )
        .unwrap_err();
    assert_eq!(error.kind, FsErrorKind::Cancelled);
}
