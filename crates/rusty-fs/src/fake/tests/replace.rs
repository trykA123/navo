use super::*;
use crate::ProviderOps;

fn pair() -> FakeFs {
    let fs = FakeFs::new();
    fs.add_folder("d");
    fs.add_file("d/new", 10);
    fs.add_file("d/old", 3);
    fs
}

fn size(fs: &FakeFs, path: &str) -> Option<u64> {
    fs.metadata(&fs.location(path)).ok().map(|e| e.size)
}

fn replace(fs: &FakeFs, from: &str, to: &str) -> Result<(), FsError> {
    fs.replace(&fs.location(from), &fs.location(to))
}

#[test]
fn replace_puts_the_source_over_an_existing_file() {
    let fs = pair();
    assert_eq!(
        (
            replace(&fs, "d/new", "d/old"),
            size(&fs, "d/new"),
            size(&fs, "d/old")
        ),
        (Ok(()), None, Some(10))
    );
}

#[test]
fn replace_onto_a_free_name_moves_the_source() {
    let fs = pair();
    assert_eq!(
        (replace(&fs, "d/new", "d/free"), size(&fs, "d/free")),
        (Ok(()), Some(10))
    );
}

#[test]
fn replace_never_overwrites_a_folder() {
    let fs = pair();
    fs.add_folder("d/sub");
    let replaced = replace(&fs, "d/new", "d/sub");
    assert_eq!(
        (
            replaced.map_err(|e| e.kind),
            size(&fs, "d/new"),
            fs.metadata(&fs.location("d/sub")).map(|e| e.kind).ok()
        ),
        (
            Err(FsErrorKind::NameInvalid),
            Some(10),
            Some(EntryKind::Folder)
        )
    );
}

#[test]
fn replace_of_a_missing_source_keeps_the_target() {
    let fs = pair();
    assert_eq!(
        (
            replace(&fs, "d/missing", "d/old").map_err(|e| e.kind),
            size(&fs, "d/old")
        ),
        (Err(FsErrorKind::NotFound), Some(3))
    );
}

#[test]
fn a_failed_replace_leaves_both_files_untouched() {
    let fs = pair();
    fs.fault(
        "d/new",
        FakeOp::Replace,
        1,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    assert_eq!(
        (
            replace(&fs, "d/new", "d/old").map_err(|e| e.kind),
            size(&fs, "d/new"),
            size(&fs, "d/old")
        ),
        (Err(FsErrorKind::AccessDenied), Some(10), Some(3))
    );
}
