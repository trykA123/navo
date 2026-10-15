use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

const FRESH_WINDOW_MS: i64 = 5_000;

fn summary(batches: &[EntryBatch]) -> Vec<(String, EntryKind, u64)> {
    entries(batches)
        .into_iter()
        .map(|e| (e.name.to_text(), e.kind, e.size))
        .collect()
}

fn now_ms() -> i64 {
    let elapsed = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    i64::try_from(elapsed.as_millis()).unwrap()
}

#[test]
fn listing_a_folder_succeeds() {
    let (_dir, folder) = with_files(2);
    assert_eq!(list_all(&folder).0, Ok(()));
}

#[test]
fn files_and_folders_have_their_kinds_and_sizes() {
    let (dir, folder) = temp();
    write_file(dir.path(), "a.txt", 5);
    write_file(dir.path(), "b.bin", 0);
    fs::create_dir(dir.path().join("sub")).unwrap();
    assert_eq!(
        summary(&list_all(&folder).1),
        [
            ("a.txt".to_owned(), EntryKind::File, 5),
            ("b.bin".to_owned(), EntryKind::File, 0),
            ("sub".to_owned(), EntryKind::Folder, 0),
        ]
    );
}

#[test]
fn a_folder_with_content_still_has_size_zero() {
    let (dir, folder) = temp();
    fs::create_dir(dir.path().join("sub")).unwrap();
    write_file(&dir.path().join("sub"), "inner", 100);
    assert_eq!(entry_named(&folder, "sub").size, 0);
}

#[cfg(unix)]
#[test]
fn a_symlink_to_a_file_is_a_link_with_size_zero() {
    let (dir, folder) = temp();
    write_file(dir.path(), "target", 7);
    std::os::unix::fs::symlink(dir.path().join("target"), dir.path().join("link")).unwrap();
    let link = entry_named(&folder, "link");
    assert_eq!((link.kind, link.size), (EntryKind::Link, 0));
}

#[cfg(unix)]
#[test]
fn a_symlink_to_a_folder_is_a_link() {
    let (dir, folder) = temp();
    fs::create_dir(dir.path().join("target")).unwrap();
    std::os::unix::fs::symlink(dir.path().join("target"), dir.path().join("link")).unwrap();
    assert_eq!(entry_named(&folder, "link").kind, EntryKind::Link);
}

#[cfg(unix)]
#[test]
fn a_dangling_symlink_is_still_listed() {
    let (dir, folder) = temp();
    std::os::unix::fs::symlink(dir.path().join("gone"), dir.path().join("link")).unwrap();
    assert_eq!(sorted_names(&list_all(&folder).1), ["link"]);
}

#[test]
fn dot_and_dot_dot_are_never_listed() {
    let (dir, folder) = temp();
    write_file(dir.path(), ".hidden", 1);
    write_file(dir.path(), "...", 1);
    assert_eq!(sorted_names(&list_all(&folder).1), ["...", ".hidden"]);
}

#[test]
fn ten_entries_in_batches_of_three_make_four_batches() {
    let (_dir, folder) = with_files(10);
    let fs = LocalFs::new().with_batch_size(3);
    assert_eq!(
        shape(&list_with(&fs, &folder).1),
        [(3, false), (3, false), (3, false), (1, true)]
    );
}

#[test]
fn batches_carry_every_entry_once() {
    let (_dir, folder) = with_files(10);
    let fs = LocalFs::new().with_batch_size(3);
    let expected: Vec<String> = (0..10).map(|i| format!("f{i:02}")).collect();
    assert_eq!(sorted_names(&list_with(&fs, &folder).1), expected);
}

#[test]
fn default_batch_size_is_512() {
    let (_dir, folder) = with_files(513);
    assert_eq!(shape(&list_all(&folder).1), [(512, false), (1, true)]);
}

#[test]
fn batch_size_zero_acts_as_one() {
    let (_dir, folder) = with_files(2);
    let fs = LocalFs::new().with_batch_size(0);
    assert_eq!(shape(&list_with(&fs, &folder).1), [(1, false), (1, true)]);
}

#[test]
fn empty_folder_sends_one_empty_last_batch() {
    let (_dir, folder) = temp();
    assert_eq!(shape(&list_all(&folder).1), [(0, true)]);
}

#[test]
fn break_from_the_sink_stops_with_ok() {
    let (_dir, folder) = with_files(10);
    let fs = LocalFs::new().with_batch_size(3);
    let mut seen = 0;
    let mut sink = |_: EntryBatch| {
        seen += 1;
        ControlFlow::Break(())
    };
    let result = fs.list(&folder, &mut sink, &CancelToken::new());
    assert_eq!((result, seen), (Ok(()), 1));
}

#[test]
fn cancel_before_the_call_gives_cancelled() {
    let (_dir, folder) = with_files(3);
    let cancel = CancelToken::new();
    cancel.cancel();
    let mut sink = |_: EntryBatch| ControlFlow::Continue(());
    let result = LocalFs::new().list(&folder, &mut sink, &cancel);
    assert_eq!(kind_of(result), Some(FsErrorKind::Cancelled));
}

#[test]
fn cancel_before_the_call_sends_no_batch() {
    let (_dir, folder) = with_files(3);
    let cancel = CancelToken::new();
    cancel.cancel();
    let mut seen = 0;
    let mut sink = |_: EntryBatch| {
        seen += 1;
        ControlFlow::Continue(())
    };
    let _ = LocalFs::new().list(&folder, &mut sink, &cancel);
    assert_eq!(seen, 0);
}

#[test]
fn cancel_inside_the_sink_gives_cancelled() {
    let (_dir, folder) = with_files(10);
    let fs = LocalFs::new().with_batch_size(3);
    let cancel = CancelToken::new();
    let mut sink = |_: EntryBatch| {
        cancel.cancel();
        ControlFlow::Continue(())
    };
    let result = fs.list(&folder, &mut sink, &cancel);
    assert_eq!(kind_of(result), Some(FsErrorKind::Cancelled));
}

#[test]
fn cancel_inside_the_sink_sends_no_second_batch() {
    let (_dir, folder) = with_files(10);
    let fs = LocalFs::new().with_batch_size(3);
    let cancel = CancelToken::new();
    let mut seen = 0;
    let mut sink = |_: EntryBatch| {
        seen += 1;
        cancel.cancel();
        ControlFlow::Continue(())
    };
    let _ = fs.list(&folder, &mut sink, &cancel);
    assert_eq!(seen, 1);
}

#[test]
fn a_fresh_file_was_modified_just_now() {
    let (dir, folder) = temp();
    write_file(dir.path(), "new", 1);
    let modified = entry_named(&folder, "new").modified_ms.unwrap();
    assert!((now_ms() - modified).abs() < FRESH_WINDOW_MS);
}

#[cfg(unix)]
#[test]
fn a_dot_file_is_hidden_on_unix() {
    let (dir, folder) = temp();
    write_file(dir.path(), ".rc", 1);
    assert!(entry_named(&folder, ".rc").attributes.hidden);
}

#[cfg(unix)]
#[test]
fn a_plain_file_has_no_attributes_on_unix() {
    let (dir, folder) = temp();
    write_file(dir.path(), "plain", 1);
    assert_eq!(
        entry_named(&folder, "plain").attributes,
        crate::Attributes::default()
    );
}

#[test]
fn a_read_only_file_is_readonly() {
    let (dir, folder) = temp();
    write_file(dir.path(), "ro", 1);
    let path = dir.path().join("ro");
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions).unwrap();
    assert!(entry_named(&folder, "ro").attributes.readonly);
}
