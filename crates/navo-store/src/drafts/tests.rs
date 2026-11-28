use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tempfile::tempdir;

use super::{
    DRAFT_DEBOUNCE_DELAY, DRAFT_MAGIC, DRAFT_VERSION, DraftDebounce, DraftError, DraftFileStamp,
    DraftSaveOutcome, DraftStore, MAX_DRAFT_BYTES, WriteStage,
};
use crate::StoreDirs;

fn sample_stamp() -> DraftFileStamp {
    DraftFileStamp {
        size: 4096,
        modified_unix_secs: 1_760_000_000,
        modified_subsec_nanos: 123_456_789,
    }
}

#[test]
fn store_dirs_drafts_dir_is_under_data_folder() {
    let dir = tempdir().unwrap();
    let dirs = StoreDirs::in_profile(dir.path());
    assert_eq!(dirs.drafts_dir(), dir.path().join("data").join("drafts"));
    let store = DraftStore::from_dirs(&dirs);
    assert_eq!(store.root(), dirs.drafts_dir());
}

#[test]
fn save_and_load_round_trip_preserves_path_stamp_and_bytes() {
    let dir = tempdir().unwrap();
    let store = DraftStore::new(dir.path().join("drafts"));
    let target = Path::new("/workspace/project/notes.md");
    let content = b"hello world\nline two \x00\xff raw bytes";
    let stamp = sample_stamp();

    let outcome = store.save(target, content, Some(stamp)).unwrap();
    let key = match outcome {
        DraftSaveOutcome::Saved { key, bytes_written } => {
            assert!(bytes_written > content.len());
            key
        }
        other => panic!("expected Saved, got {other:?}"),
    };

    assert!(store.has_draft(target).unwrap());
    let loaded = store.load(target).unwrap().expect("draft should exist");
    assert_eq!(loaded.key, key);
    assert_eq!(loaded.target_path, target);
    assert_eq!(loaded.file_stamp, Some(stamp));
    assert_eq!(loaded.content, content);
    assert_eq!(loaded.content_blake3, *blake3::hash(content).as_bytes());
    assert!(loaded.saved_at_unix_secs > 0);
}

#[test]
fn remove_deletes_draft_and_is_idempotent() {
    let dir = tempdir().unwrap();
    let store = DraftStore::new(dir.path().join("drafts"));
    let target = Path::new("/workspace/project/lib.rs");

    store.save(target, b"fn main() {}", None).unwrap();
    assert!(store.has_draft(target).unwrap());

    assert!(store.remove(target).unwrap());
    assert!(!store.has_draft(target).unwrap());
    assert_eq!(store.load(target).unwrap(), None);

    // Second remove is an idempotent no-op.
    assert!(!store.remove(target).unwrap());
}

#[test]
fn payload_over_32_mib_is_skipped_without_writing_file() {
    let dir = tempdir().unwrap();
    let store = DraftStore::new(dir.path().join("drafts"));
    let target = Path::new("/workspace/huge.log");
    let oversized = vec![b'a'; MAX_DRAFT_BYTES + 1];

    let outcome = store.save(target, &oversized, None).unwrap();
    assert_eq!(
        outcome,
        DraftSaveOutcome::SkippedTooLarge {
            size: MAX_DRAFT_BYTES + 1,
            limit: MAX_DRAFT_BYTES,
        }
    );
    assert!(!store.has_draft(target).unwrap());
    assert!(!store.draft_path(target).exists());
    assert_eq!(store.load(target).unwrap(), None);

    // If a smaller draft existed earlier, an oversized save clears the stale draft.
    store.save(target, b"small", None).unwrap();
    assert!(store.has_draft(target).unwrap());
    let outcome = store.save(target, &oversized, None).unwrap();
    assert!(matches!(outcome, DraftSaveOutcome::SkippedTooLarge { .. }));
    assert!(!store.has_draft(target).unwrap());
}

#[test]
fn payload_at_exact_32_mib_limit_is_saved_and_loaded() {
    let dir = tempdir().unwrap();
    let store = DraftStore::new(dir.path().join("drafts"));
    let target = Path::new("/workspace/exact_32m.bin");
    let exact = vec![0x5A; MAX_DRAFT_BYTES];

    let outcome = store.save(target, &exact, None).unwrap();
    assert!(matches!(outcome, DraftSaveOutcome::Saved { .. }));
    let loaded = store.load(target).unwrap().expect("exact 32 MiB draft");
    assert_eq!(loaded.content.len(), MAX_DRAFT_BYTES);
    assert_eq!(loaded.content_blake3, *blake3::hash(&exact).as_bytes());
}

#[test]
fn fault_mid_write_and_replace_failure_preserves_existing_draft() {
    let dir = tempdir().unwrap();
    let store = DraftStore::new(dir.path().join("drafts"));
    let target = Path::new("/workspace/important.rs");
    let original = b"original durable content v1";
    let updated = b"replacement content v2 that must not tear the old draft";

    store.save(target, original, Some(sample_stamp())).unwrap();
    let draft_path = store.draft_path(target);
    let original_raw = fs::read(&draft_path).unwrap();

    for fail_stage in [
        WriteStage::AfterPartialTempWrite,
        WriteStage::AfterTempSync,
        WriteStage::BeforeReplace,
    ] {
        let mut hook = |stage: WriteStage| {
            if stage == fail_stage {
                Err(io::Error::other(format!("injected fault at {stage:?}")))
            } else {
                Ok(())
            }
        };
        let err = store
            .save_with_hook(target, updated, None, 1_760_000_100, &mut hook)
            .expect_err("injected fault must fail save");
        assert!(matches!(err, DraftError::Io(_)));

        // Existing draft on disk is 100% intact, never torn or partial.
        assert_eq!(fs::read(&draft_path).unwrap(), original_raw);
        let loaded = store.load(target).unwrap().expect("old draft intact");
        assert_eq!(loaded.content, original);

        // No temp files left behind after the failed write.
        let entries: Vec<_> = fs::read_dir(store.root())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            entries.len(),
            1,
            "unexpected leftover files after {fail_stage:?}: {entries:?}"
        );
    }
}

#[test]
fn corrupt_and_truncated_draft_files_return_typed_errors() {
    let dir = tempdir().unwrap();
    let store = DraftStore::new(dir.path().join("drafts"));
    let target = Path::new("/workspace/corrupt.txt");
    store.save(target, b"valid draft body", None).unwrap();
    let path = store.draft_path(target);
    let valid_bytes = fs::read(&path).unwrap();

    // 1. Bad magic.
    fs::write(&path, b"NOT-A-NAVO-DRAFT-HEADER").unwrap();
    assert!(matches!(store.load(target), Err(DraftError::BadMagic)));

    // 2. Unsupported future version.
    let mut future = valid_bytes.clone();
    let next_ver = (DRAFT_VERSION + 1).to_le_bytes();
    future[DRAFT_MAGIC.len()..DRAFT_MAGIC.len() + 2].copy_from_slice(&next_ver);
    fs::write(&path, &future).unwrap();
    assert!(matches!(
        store.load(target),
        Err(DraftError::UnsupportedVersion { .. })
    ));

    // 3. Truncated body.
    fs::write(&path, &valid_bytes[..valid_bytes.len() - 4]).unwrap();
    assert!(matches!(store.load(target), Err(DraftError::Truncated(_))));

    // 4. Bit-flipped content byte -> checksum mismatch.
    let mut flipped = valid_bytes.clone();
    let last = flipped.len() - 1;
    flipped[last] ^= 0x01;
    fs::write(&path, &flipped).unwrap();
    assert!(matches!(store.load(target), Err(DraftError::BadChecksum)));

    // 5. Trailing garbage byte -> length mismatch.
    let mut trailing = valid_bytes;
    trailing.push(0xFF);
    fs::write(&path, &trailing).unwrap();
    assert!(matches!(store.load(target), Err(DraftError::Truncated(_))));

    // 6. Oversized file on disk -> bounded read refuses without OOM.
    let oversized_len = (MAX_DRAFT_BYTES as u64) + 128 * 1024;
    let file = fs::File::create(&path).unwrap();
    file.set_len(oversized_len).unwrap();
    drop(file);
    assert!(matches!(
        store.load(target),
        Err(DraftError::TooLarge { .. })
    ));
}

#[test]
fn file_stamp_from_path_reads_existing_and_returns_none_for_missing() {
    let dir = tempdir().unwrap();
    let missing = dir.path().join("absent.txt");
    assert_eq!(DraftFileStamp::from_path(&missing).unwrap(), None);

    let file_path = dir.path().join("present.txt");
    fs::write(&file_path, b"twelve bytes").unwrap();
    let stamp = DraftFileStamp::from_path(&file_path)
        .unwrap()
        .expect("stamp for existing file");
    assert_eq!(stamp.size, 12);
    assert!(stamp.modified_unix_secs > 0);
}

#[test]
fn hostile_long_unicode_and_windows_paths_stay_inside_drafts_dir() {
    let dir = tempdir().unwrap();
    let store = DraftStore::new(dir.path().join("drafts"));

    let long_segment = "a".repeat(120);
    let long_path = format!("\\\\?\\C:\\{long_segment}\\{long_segment}\\{long_segment}\\file.rs");
    let hostile_paths = [
        PathBuf::from("../../../../etc/passwd"),
        PathBuf::from("C:\\Users\\admin\\..\\CON.txt"),
        PathBuf::from("NUL"),
        PathBuf::from(long_path),
        PathBuf::from("/tmp/日本語/مجلد/🦀_draft_file.md"),
    ];

    for (idx, target) in hostile_paths.iter().enumerate() {
        let draft_path = store.draft_path(target);
        assert_eq!(draft_path.parent(), Some(store.root()));
        let file_name = draft_path
            .file_name()
            .and_then(|n| n.to_str())
            .expect("ASCII filename");
        assert!(file_name.ends_with(".draft"));
        let stem = file_name.trim_end_matches(".draft");
        assert_eq!(stem.len(), 64);
        assert!(stem.bytes().all(|b| b.is_ascii_hexdigit()));

        let payload = format!("payload-{idx}");
        store.save(target, payload.as_bytes(), None).unwrap();
        let loaded = store.load(target).unwrap().expect("round-trip hostile");
        assert_eq!(loaded.content, payload.as_bytes());
    }
}

#[test]
fn list_drafts_and_prune_ignore_temps_and_corrupt_files() {
    let dir = tempdir().unwrap();
    let store = DraftStore::new(dir.path().join("drafts"));

    let old_target = Path::new("/workspace/old.rs");
    let fresh_target = Path::new("/workspace/fresh.rs");

    store
        .save_with_hook(old_target, b"old", None, 1_000, &mut |_| Ok(()))
        .unwrap();
    store.save(fresh_target, b"fresh", None).unwrap();

    // Drop a stray temp file and a corrupt `.draft` file into `drafts/`.
    fs::write(store.root().join(".stray.draft.tmp-99999-0"), b"partial").unwrap();
    fs::write(store.root().join("corrupt.draft"), b"garbage").unwrap();

    let listed = store.list_drafts().unwrap();
    assert_eq!(listed.len(), 2);

    let pruned = store.prune_older_than(Duration::from_secs(3600)).unwrap();
    assert_eq!(pruned, 1);
    assert!(!store.has_draft(old_target).unwrap());
    assert!(store.has_draft(fresh_target).unwrap());
}

#[test]
fn debounce_policy_waits_one_second_and_resets_on_edits() {
    let mut debounce = DraftDebounce::default();
    assert_eq!(debounce.delay(), DRAFT_DEBOUNCE_DELAY);

    let t0 = Instant::now();
    assert!(!debounce.is_due(t0));
    assert_eq!(debounce.due_at(), None);

    debounce.mark_dirty(t0);
    assert_eq!(debounce.due_at(), Some(t0 + Duration::from_secs(1)));
    assert!(!debounce.is_due(t0 + Duration::from_millis(999)));

    // Another edit at +600 ms pushes the deadline to +1600 ms.
    debounce.mark_dirty(t0 + Duration::from_millis(600));
    assert!(!debounce.is_due(t0 + Duration::from_secs(1)));
    assert!(debounce.is_due(t0 + Duration::from_millis(1600)));

    debounce.clear();
    assert!(!debounce.is_due(t0 + Duration::from_secs(10)));
    assert_eq!(debounce.due_at(), None);
}
