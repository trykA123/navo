//! Red-first unit tests for sparse hex patch overlay, undo/redo, streaming durable save,
//! file-stamp conflict refusal, and crash/replace fault recovery.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime};

use navo_core::paging::ByteSource;

use super::{
    HEX_SAVE_CHUNK, HexFileStamp, HexPatchOverlay, HexSaveError, PatchedByteSource, SaveFaultStage,
    save_patched_file, save_patched_file_with_hook,
};

struct WindowedSource {
    bytes: Vec<u8>,
    max_single_read: AtomicUsize,
}

impl WindowedSource {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            max_single_read: AtomicUsize::new(0),
        }
    }

    fn max_read(&self) -> usize {
        self.max_single_read.load(Ordering::Relaxed)
    }
}

impl ByteSource for WindowedSource {
    fn len(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        self.max_single_read.fetch_max(buf.len(), Ordering::Relaxed);
        self.bytes.as_slice().read_at(offset, buf)
    }
}

fn dir_entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("read dir")
        .map(|entry| {
            entry
                .expect("dir entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

fn read_boundary_window(patched: &PatchedByteSource<WindowedSource>, boundary: u64) -> [u8; 4] {
    let mut window = [0u8; 4];
    let read = patched
        .read_range(boundary - 2, &mut window)
        .expect("read_range");
    assert_eq!(read, 4);
    window
}

#[test]
fn hex_patch_overlay_overwrites_across_chunks_and_undo_redo_restores_clean_state() {
    let total = HEX_SAVE_CHUNK * 2 + 512;
    let base_bytes: Vec<u8> = (0..total).map(|i| (i % 251) as u8).collect();
    let base = Arc::new(WindowedSource::new(base_bytes.clone()));
    let patched = PatchedByteSource::new(base.clone());

    let boundary = HEX_SAVE_CHUNK as u64;
    let (orig_0, orig_prev, orig_curr, orig_tail) = (
        base_bytes[0],
        base_bytes[HEX_SAVE_CHUNK - 1],
        base_bytes[HEX_SAVE_CHUNK],
        base_bytes[total - 1],
    );

    {
        let mut overlay = patched.overlay().write().expect("overlay lock");
        assert!(!overlay.is_dirty());
        assert_eq!(overlay.patched_count(), 0);
        assert!(overlay.overwrite_byte(0, orig_0, orig_0 ^ 0xFF));
        assert!(overlay.overwrite_range(boundary - 1, &[orig_prev, orig_curr], &[0xDE, 0xAD]));
        assert!(overlay.overwrite_byte((total - 1) as u64, orig_tail, 0x5A));
        assert!(overlay.is_dirty());
        assert_eq!(overlay.patched_count(), 4);
        assert!(overlay.is_offset_patched(0) && overlay.is_offset_patched(boundary));
        assert!(!overlay.is_offset_patched(1));
    }

    let left = base_bytes[HEX_SAVE_CHUNK - 2];
    let right = base_bytes[HEX_SAVE_CHUNK + 1];
    assert_eq!(
        read_boundary_window(&patched, boundary),
        [left, 0xDE, 0xAD, right]
    );
    assert!(base.max_read() <= 4);

    // Undo all 3 edits -> restores original bytes and clears is_dirty().
    {
        let mut overlay = patched.overlay().write().expect("overlay lock");
        assert_eq!(overlay.undo(), Some((total - 1) as u64));
        assert_eq!(overlay.undo(), Some(boundary - 1));
        assert_eq!(overlay.undo(), Some(0));
        assert!(!overlay.is_dirty());
        assert_eq!(overlay.patched_count(), 0);
        assert_eq!(overlay.undo(), None);
    }
    assert_eq!(
        read_boundary_window(&patched, boundary),
        [left, orig_prev, orig_curr, right]
    );

    // Redo reapplies edits in order.
    {
        let mut overlay = patched.overlay().write().expect("overlay lock");
        assert_eq!(overlay.redo(), Some(0));
        assert_eq!(overlay.redo(), Some(boundary - 1));
        assert!(overlay.is_dirty());
        assert_eq!(overlay.patched_count(), 3);
    }
    assert_eq!(
        read_boundary_window(&patched, boundary),
        [left, 0xDE, 0xAD, right]
    );
}

#[test]
fn hex_patch_streaming_durable_save_commits_multi_chunk_file_and_updates_stamp() {
    let dir = tempfile::tempdir().expect("tempdir");
    let target = dir.path().join("firmware.bin");
    let total = HEX_SAVE_CHUNK * 2 + 1024;
    let mut expected_bytes: Vec<u8> = (0..total).map(|i| ((i * 7 + 3) % 256) as u8).collect();
    fs::write(&target, &expected_bytes).expect("write fixture");

    let initial_stamp = HexFileStamp::from_path(&target).expect("capture initial stamp");
    let mut overlay = HexPatchOverlay::new();
    let edits = [
        (0u64, 0xAAu8),
        ((HEX_SAVE_CHUNK - 1) as u64, 0xBBu8),
        (HEX_SAVE_CHUNK as u64, 0xCCu8),
        ((total - 1) as u64, 0xDDu8),
    ];
    for (offset, value) in edits {
        let orig = expected_bytes[offset as usize];
        overlay.overwrite_byte(offset, orig, value);
        expected_bytes[offset as usize] = value;
    }

    let patches = overlay.snapshot_patches();
    let new_stamp = save_patched_file(&target, &initial_stamp, &patches).expect("durable save");
    overlay.clear();

    assert!(!overlay.is_dirty());
    assert_eq!(new_stamp.len, total as u64);
    assert_eq!(fs::read(&target).expect("read saved"), expected_bytes);
    assert_eq!(dir_entries(dir.path()), vec!["firmware.bin".to_owned()]);
}

#[test]
fn hex_patch_save_refuses_when_file_changed_on_disk_and_leaves_newer_file_untouched() {
    let dir = tempfile::tempdir().expect("tempdir");
    let target = dir.path().join("data.bin");
    fs::write(&target, b"0123456789abcdef").expect("write original");
    let initial_stamp = HexFileStamp::from_path(&target).expect("initial stamp");

    // Case 1: External modification changes file size and contents.
    let newer_resized = b"EXTERNAL_NEWER_CONTENT_LONGER";
    fs::write(&target, newer_resized).expect("external rewrite");

    let mut patches = BTreeMap::new();
    patches.insert(0, 0xFF);
    let err = save_patched_file(&target, &initial_stamp, &patches)
        .expect_err("must refuse save when size changed on disk");
    assert!(
        matches!(err, HexSaveError::FileChangedOnDisk),
        "expected FileChangedOnDisk, got {err:?}"
    );
    assert_eq!(
        fs::read(&target).expect("read after refusal"),
        newer_resized
    );
    assert_eq!(dir_entries(dir.path()), vec!["data.bin".to_owned()]);

    // Case 2: External modification keeps exact same size but changes mtime/content.
    let same_len_target = dir.path().join("same_len.bin");
    fs::write(&same_len_target, b"aaaaaaaaaaaaaaaa").expect("write original");
    let stamp2 = HexFileStamp::from_path(&same_len_target).expect("stamp2");
    fs::write(&same_len_target, b"bbbbbbbbbbbbbbbb").expect("same-len rewrite");
    let later = SystemTime::now() + Duration::from_secs(120);
    File::options()
        .write(true)
        .open(&same_len_target)
        .expect("open for mtime")
        .set_modified(later)
        .expect("advance mtime");

    let err2 = save_patched_file(&same_len_target, &stamp2, &patches)
        .expect_err("must refuse save when mtime changed on disk");
    assert!(
        matches!(err2, HexSaveError::FileChangedOnDisk),
        "expected FileChangedOnDisk, got {err2:?}"
    );
    assert_eq!(
        fs::read(&same_len_target).expect("read same_len after refusal"),
        b"bbbbbbbbbbbbbbbb"
    );
    assert_eq!(
        dir_entries(dir.path()),
        vec!["data.bin".to_owned(), "same_len.bin".to_owned()]
    );
}

#[test]
fn hex_patch_crash_or_replace_fault_leaves_original_intact_with_zero_temp_files() {
    let dir = tempfile::tempdir().expect("tempdir");
    let target = dir.path().join("precious.bin");
    let original: Vec<u8> = (0..(HEX_SAVE_CHUNK + 128))
        .map(|i| (i % 199) as u8)
        .collect();
    fs::write(&target, &original).expect("write precious");
    let stamp = HexFileStamp::from_path(&target).expect("stamp");
    let mut patches = BTreeMap::new();
    patches.insert(0, original[0] ^ 0xFF);
    patches.insert(HEX_SAVE_CHUNK as u64, 0x42);

    // Fault 1: I/O error mid-stream after first chunk.
    let err = save_patched_file_with_hook(&target, &stamp, &patches, &mut |stage| {
        if stage == SaveFaultStage::AfterFirstChunk {
            return Err(io::Error::other("simulated disk full mid-stream"));
        }
        Ok(())
    })
    .expect_err("simulated mid-stream fault must fail");
    assert!(matches!(err, HexSaveError::Io(_)), "got {err:?}");
    assert_eq!(fs::read(&target).expect("original intact"), original);
    assert_eq!(dir_entries(dir.path()), vec!["precious.bin".to_owned()]);

    // Fault 2: Crash right before durable replace.
    let err = save_patched_file_with_hook(&target, &stamp, &patches, &mut |stage| {
        if stage == SaveFaultStage::BeforeReplace {
            return Err(io::Error::other("simulated crash before replace"));
        }
        Ok(())
    })
    .expect_err("simulated pre-replace fault must fail");
    assert!(matches!(err, HexSaveError::Io(_)), "got {err:?}");
    assert_eq!(fs::read(&target).expect("original intact"), original);
    assert_eq!(dir_entries(dir.path()), vec!["precious.bin".to_owned()]);
}

#[test]
fn hex_patch_save_supports_255_byte_filename_without_enametoolong() {
    let dir = tempfile::tempdir().expect("tempdir");
    let long_name = format!("{}.bin", "a".repeat(251));
    assert_eq!(long_name.len(), 255);
    let target = dir.path().join(&long_name);
    fs::write(&target, b"0123456789abcdef").expect("write 255-byte name fixture");
    let stamp = HexFileStamp::from_path(&target).expect("initial stamp");

    let mut patches = BTreeMap::new();
    patches.insert(0, 0xEE);
    save_patched_file(&target, &stamp, &patches).expect("save 255-byte filename");

    let saved = fs::read(&target).expect("read saved");
    assert_eq!(saved[0], 0xEE);
    assert_eq!(&saved[1..], b"123456789abcdef");
    assert_eq!(dir_entries(dir.path()), vec![long_name]);
}
