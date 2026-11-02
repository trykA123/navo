use super::*;
use crate::ProviderOps;

fn sample() -> FakeFs {
    let fs = FakeFs::new();
    fs.add_folder("a");
    fs.add_folder("b");
    fs.add_file("a/f.txt", 10_000);
    fs
}

fn exists(fs: &FakeFs, path: &str) -> bool {
    fs.metadata(&fs.location(path)).is_ok()
}

fn size(fs: &FakeFs, path: &str) -> Option<u64> {
    fs.metadata(&fs.location(path)).ok().map(|e| e.size)
}

fn add_link(fs: &FakeFs, folder: &str, name: &str) {
    fs.add_entry(
        folder,
        FsEntry {
            kind: EntryKind::Link,
            size: 0,
            ..file_entry(ProviderPath::from_text(name))
        },
    );
}

fn copy_with(
    fs: &FakeFs,
    (from, to): (&str, &str),
    cancel: &CancelToken,
    on_progress: &mut dyn FnMut(u64),
) -> Result<u64, FsError> {
    fs.copy_file(&fs.location(from), &fs.location(to), on_progress, cancel)
}

fn copy(fs: &FakeFs, from: &str, to: &str) -> Result<u64, FsError> {
    copy_with(fs, (from, to), &CancelToken::new(), &mut |_| {})
}

fn progress_of(fs: &FakeFs, from: &str, to: &str) -> Vec<u64> {
    let mut seen = Vec::new();
    let _ = copy_with(fs, (from, to), &CancelToken::new(), &mut |bytes| {
        seen.push(bytes);
    });
    seen
}

#[test]
fn create_folder_makes_an_empty_folder() {
    let fs = sample();
    let made = fs.create_folder(&fs.location("b/n"));
    assert_eq!(
        (made, names(&list_all(&fs, "b/n").1)),
        (Ok(()), Vec::<String>::new())
    );
}

#[test]
fn create_folder_fails_when_a_file_is_there() {
    let fs = sample();
    let made = fs.create_folder(&fs.location("a/f.txt"));
    assert_eq!((made.is_err(), size(&fs, "a/f.txt")), (true, Some(10_000)));
}

#[test]
fn create_file_makes_an_empty_file() {
    let fs = sample();
    let made = fs.create_file(&fs.location("b/notes.txt"));
    assert_eq!((made, size(&fs, "b/notes.txt")), (Ok(()), Some(0)));
}

#[test]
fn create_file_keeps_an_existing_file() {
    let fs = sample();
    let made = fs.create_file(&fs.location("a/f.txt"));
    assert_eq!((made.is_err(), size(&fs, "a/f.txt")), (true, Some(10_000)));
}

#[test]
fn create_folder_fails_when_a_folder_is_there() {
    let fs = sample();
    assert!(fs.create_folder(&fs.location("a")).is_err());
}

#[test]
fn create_folder_without_a_parent_is_not_found() {
    let fs = sample();
    assert_eq!(
        kind_of(fs.create_folder(&fs.location("x/n"))),
        Some(FsErrorKind::NotFound)
    );
}

#[test]
fn copy_file_writes_the_source_size() {
    let fs = sample();
    let copied = copy(&fs, "a/f.txt", "b/f.txt");
    assert_eq!((copied, size(&fs, "b/f.txt")), (Ok(10_000), Some(10_000)));
}

#[test]
fn copy_file_keeps_the_source() {
    let fs = sample();
    let _ = copy(&fs, "a/f.txt", "b/f.txt");
    assert_eq!(size(&fs, "a/f.txt"), Some(10_000));
}

#[test]
fn copy_file_reports_progress_per_4_kib_chunk() {
    let fs = sample();
    assert_eq!(progress_of(&fs, "a/f.txt", "b/f.txt"), [4096, 8192, 10_000]);
}

#[test]
fn copy_file_fails_when_the_target_exists() {
    let fs = sample();
    fs.add_file("b/f.txt", 3);
    let copied = copy(&fs, "a/f.txt", "b/f.txt");
    assert_eq!((copied.is_err(), size(&fs, "b/f.txt")), (true, Some(3)));
}

#[test]
fn copy_file_of_a_missing_source_is_not_found() {
    let fs = sample();
    assert_eq!(
        kind_of(copy(&fs, "a/missing", "b/f.txt")),
        Some(FsErrorKind::NotFound)
    );
}

#[test]
fn copy_file_into_a_missing_folder_is_not_found() {
    let fs = sample();
    assert_eq!(
        kind_of(copy(&fs, "a/f.txt", "x/f.txt")),
        Some(FsErrorKind::NotFound)
    );
}

#[test]
fn copy_file_cancelled_before_the_start_writes_nothing() {
    let fs = sample();
    let cancel = CancelToken::new();
    cancel.cancel();
    let copied = copy_with(&fs, ("a/f.txt", "b/f.txt"), &cancel, &mut |_| {});
    assert_eq!(
        (kind_of(copied), exists(&fs, "b/f.txt")),
        (Some(FsErrorKind::Cancelled), false)
    );
}

#[test]
fn copy_file_cancelled_mid_copy_leaves_a_partial_target() {
    let fs = sample();
    let cancel = CancelToken::new();
    let copied = copy_with(&fs, ("a/f.txt", "b/f.txt"), &cancel, &mut |_| {
        cancel.cancel();
    });
    assert_eq!(
        (kind_of(copied), size(&fs, "b/f.txt")),
        (Some(FsErrorKind::Cancelled), Some(4096))
    );
}

#[test]
fn rename_moves_a_file() {
    let fs = sample();
    let renamed = fs.rename(&fs.location("a/f.txt"), &fs.location("b/g.txt"));
    assert_eq!(
        (renamed, exists(&fs, "a/f.txt"), size(&fs, "b/g.txt")),
        (Ok(()), false, Some(10_000))
    );
}

#[test]
fn rename_moves_a_folder_with_its_children() {
    let fs = sample();
    let _ = fs.rename(&fs.location("a"), &fs.location("b/a2"));
    assert_eq!(
        (exists(&fs, "a"), size(&fs, "b/a2/f.txt")),
        (false, Some(10_000))
    );
}

#[test]
fn rename_fails_when_the_target_exists() {
    let fs = sample();
    fs.add_file("b/f.txt", 3);
    let renamed = fs.rename(&fs.location("a/f.txt"), &fs.location("b/f.txt"));
    assert_eq!(
        (renamed.is_err(), size(&fs, "a/f.txt"), size(&fs, "b/f.txt")),
        (true, Some(10_000), Some(3))
    );
}

#[test]
fn rename_into_itself_fails_and_keeps_the_folder() {
    let fs = sample();
    let renamed = fs.rename(&fs.location("a"), &fs.location("a/inner"));
    assert_eq!(
        (renamed.is_err(), size(&fs, "a/f.txt")),
        (true, Some(10_000))
    );
}

#[test]
fn rename_of_a_missing_source_is_not_found() {
    let fs = sample();
    assert_eq!(
        kind_of(fs.rename(&fs.location("a/missing"), &fs.location("b/x"))),
        Some(FsErrorKind::NotFound)
    );
}

#[test]
fn remove_file_removes_a_file() {
    let fs = sample();
    let removed = fs.remove_file(&fs.location("a/f.txt"));
    assert_eq!((removed, exists(&fs, "a/f.txt")), (Ok(()), false));
}

#[test]
fn remove_file_fails_on_a_folder() {
    let fs = sample();
    fs.add_folder("b/empty");
    let removed = fs.remove_file(&fs.location("b/empty"));
    assert_eq!((removed.is_err(), exists(&fs, "b/empty")), (true, true));
}

#[test]
fn remove_file_fails_on_a_link() {
    let fs = sample();
    add_link(&fs, "a", "l");
    assert!(fs.remove_file(&fs.location("a/l")).is_err());
}

#[test]
fn remove_of_a_missing_entry_is_not_found() {
    let fs = sample();
    assert_eq!(
        kind_of(fs.remove_file(&fs.location("a/missing"))),
        Some(FsErrorKind::NotFound)
    );
}

#[test]
fn remove_link_removes_the_link() {
    let fs = sample();
    add_link(&fs, "a", "l");
    let removed = fs.remove_link(&fs.location("a/l"));
    assert_eq!((removed, exists(&fs, "a/l")), (Ok(()), false));
}

#[test]
fn remove_link_does_not_touch_the_target() {
    let fs = sample();
    fs.add_file("b/target.txt", 5);
    add_link(&fs, "a", "l");
    let _ = fs.remove_link(&fs.location("a/l"));
    assert_eq!(size(&fs, "b/target.txt"), Some(5));
}

#[test]
fn remove_link_fails_on_a_file() {
    let fs = sample();
    let removed = fs.remove_link(&fs.location("a/f.txt"));
    assert_eq!((removed.is_err(), exists(&fs, "a/f.txt")), (true, true));
}

#[test]
fn remove_empty_folder_removes_an_empty_folder() {
    let fs = sample();
    let removed = fs.remove_empty_folder(&fs.location("b"));
    assert_eq!((removed, exists(&fs, "b")), (Ok(()), false));
}

#[test]
fn remove_empty_folder_fails_when_not_empty() {
    let fs = sample();
    let removed = fs.remove_empty_folder(&fs.location("a"));
    assert_eq!((removed.is_err(), exists(&fs, "a/f.txt")), (true, true));
}

#[test]
fn remove_empty_folder_fails_on_a_file() {
    let fs = sample();
    assert!(fs.remove_empty_folder(&fs.location("a/f.txt")).is_err());
}

#[test]
fn recycle_moves_the_entry_to_the_recycled_list() {
    let fs = sample();
    let recycled = fs.recycle(&fs.location("a/f.txt"));
    assert_eq!(
        (recycled, exists(&fs, "a/f.txt"), fs.recycled()),
        (Ok(()), false, vec!["a/f.txt".to_owned()])
    );
}

#[test]
fn recycle_of_a_missing_entry_is_not_found() {
    let fs = sample();
    assert_eq!(
        kind_of(fs.recycle(&fs.location("a/missing"))),
        Some(FsErrorKind::NotFound)
    );
}

#[test]
fn create_fault_fails_create_folder() {
    let fs = sample();
    fs.fault(
        "b/n",
        FakeOp::Create,
        1,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    let made = fs.create_folder(&fs.location("b/n"));
    assert_eq!(
        (kind_of(made), exists(&fs, "b/n")),
        (Some(FsErrorKind::AccessDenied), false)
    );
}

#[test]
fn copy_fault_on_the_source_fails_before_writing() {
    let fs = sample();
    fs.fault("a/f.txt", FakeOp::Copy, 1, Fault::Fail(FsErrorKind::Locked));
    let copied = copy(&fs, "a/f.txt", "b/f.txt");
    assert_eq!(
        (kind_of(copied), exists(&fs, "b/f.txt")),
        (Some(FsErrorKind::Locked), false)
    );
}

fn fail_after_5000(fs: &FakeFs) {
    let fault = Fault::FailAfterBytes {
        kind: FsErrorKind::DiskFull,
        bytes: 5000,
    };
    fs.fault("a/f.txt", FakeOp::Copy, 1, fault);
}

#[test]
fn fail_after_bytes_leaves_a_partial_target() {
    let fs = sample();
    fail_after_5000(&fs);
    let copied = copy(&fs, "a/f.txt", "b/f.txt");
    assert_eq!(
        (kind_of(copied), size(&fs, "b/f.txt")),
        (Some(FsErrorKind::DiskFull), Some(5000))
    );
}

#[test]
fn fail_after_bytes_reports_progress_up_to_the_bytes() {
    let fs = sample();
    fail_after_5000(&fs);
    assert_eq!(progress_of(&fs, "a/f.txt", "b/f.txt"), [4096, 5000]);
}

#[test]
fn rename_fault_keeps_the_source() {
    let fs = sample();
    fs.fault(
        "a/f.txt",
        FakeOp::Rename,
        1,
        Fault::Fail(FsErrorKind::Locked),
    );
    let renamed = fs.rename(&fs.location("a/f.txt"), &fs.location("b/f.txt"));
    assert_eq!(
        (kind_of(renamed), exists(&fs, "a/f.txt")),
        (Some(FsErrorKind::Locked), true)
    );
}

#[test]
fn remove_fault_fails_every_remove() {
    let fs = sample();
    add_link(&fs, "a", "l");
    for (path, call) in [("a/f.txt", 1), ("a/l", 1), ("b", 1)] {
        fs.fault(path, FakeOp::Remove, call, Fault::Fail(FsErrorKind::Locked));
    }
    let kinds = [
        kind_of(fs.remove_file(&fs.location("a/f.txt"))),
        kind_of(fs.remove_link(&fs.location("a/l"))),
        kind_of(fs.remove_empty_folder(&fs.location("b"))),
    ];
    assert_eq!(kinds, [Some(FsErrorKind::Locked); 3]);
}

#[test]
fn recycle_fault_keeps_the_entry() {
    let fs = sample();
    fs.fault(
        "a/f.txt",
        FakeOp::Recycle,
        1,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    let recycled = fs.recycle(&fs.location("a/f.txt"));
    assert_eq!(
        (kind_of(recycled), exists(&fs, "a/f.txt"), fs.recycled()),
        (Some(FsErrorKind::AccessDenied), true, Vec::<String>::new())
    );
}

#[test]
fn copy_calls_are_counted_on_the_source() {
    let fs = sample();
    let _ = copy(&fs, "a/f.txt", "b/one");
    let _ = copy(&fs, "a/f.txt", "b/two");
    assert_eq!(fs.calls("a/f.txt", FakeOp::Copy), 2);
}

#[test]
fn write_fault_fires_on_its_call_only() {
    let fs = sample();
    fs.fault("a/f.txt", FakeOp::Copy, 2, Fault::Fail(FsErrorKind::Locked));
    let kinds = [
        kind_of(copy(&fs, "a/f.txt", "b/one")),
        kind_of(copy(&fs, "a/f.txt", "b/two")),
        kind_of(copy(&fs, "a/f.txt", "b/three")),
    ];
    assert_eq!(kinds, [None, Some(FsErrorKind::Locked), None]);
}
