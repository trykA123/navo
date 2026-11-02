use super::*;
use crate::{ALREADY_EXISTS_CODES, ProviderOps};

fn insensitive() -> FakeFs {
    let fs = FakeFs::new().case_insensitive();
    fs.add_folder("d");
    fs.add_file("d/a.txt", 5);
    fs
}

fn copy_to(fs: &FakeFs, from: &str, to: &str) -> Result<u64, FsError> {
    fs.copy_file(
        &fs.location(from),
        &fs.location(to),
        &mut |_| {},
        &CancelToken::new(),
    )
}

fn listed(fs: &FakeFs, path: &str) -> Vec<String> {
    names(&list_all(fs, path).1)
}

#[test]
fn create_refuses_a_name_that_differs_by_case() {
    let fs = insensitive();
    let made = fs.create_file(&fs.location("d/A.txt"));
    assert_eq!(
        (made.is_err(), listed(&fs, "d")),
        (true, vec!["a.txt".to_owned()])
    );
}

#[test]
fn create_folder_refuses_a_name_that_differs_by_case() {
    let fs = insensitive();
    assert!(fs.create_folder(&fs.location("d/A.TXT")).is_err());
}

#[test]
fn copy_refuses_a_name_that_differs_by_case() {
    let fs = insensitive();
    fs.add_file("d/b.txt", 1);
    assert!(copy_to(&fs, "d/b.txt", "d/A.txt").is_err());
    assert_eq!(listed(&fs, "d").len(), 2);
}

#[test]
fn rename_onto_a_name_that_differs_by_case_is_refused_as_existing() {
    let fs = insensitive();
    fs.add_file("d/b.txt", 1);
    let error = fs
        .rename(&fs.location("d/b.txt"), &fs.location("d/A.txt"))
        .unwrap_err();
    assert_eq!(error.os_code, Some(ALREADY_EXISTS_CODES[0]));
}

#[test]
fn lookups_fold_names_and_folders() {
    let fs = insensitive();
    let entry = fs.metadata(&fs.location("D/A.TXT")).unwrap();
    assert_eq!((entry.size, entry.name.to_text()), (5, "a.txt".to_owned()));
}

#[test]
fn case_only_rename_keeps_one_entry_with_the_new_spelling() {
    let fs = insensitive();
    fs.rename(&fs.location("d/a.txt"), &fs.location("d/A.txt"))
        .unwrap();
    assert_eq!(listed(&fs, "d"), vec!["A.txt".to_owned()]);
}

#[test]
fn case_only_rename_keeps_the_size() {
    let fs = insensitive();
    fs.rename(&fs.location("d/a.txt"), &fs.location("d/A.txt"))
        .unwrap();
    assert_eq!(fs.metadata(&fs.location("d/a.txt")).unwrap().size, 5);
}

#[test]
fn the_default_fake_keeps_two_names_that_differ_by_case() {
    let fs = FakeFs::new();
    fs.add_folder("d");
    fs.add_file("d/a.txt", 1);
    fs.create_file(&fs.location("d/A.txt")).unwrap();
    assert_eq!(listed(&fs, "d").len(), 2);
}

#[test]
fn the_default_fake_does_not_fold_lookups() {
    let fs = FakeFs::new();
    fs.add_file("a.txt", 1);
    assert_eq!(
        kind_of(fs.metadata(&fs.location("A.txt"))),
        Some(FsErrorKind::NotFound)
    );
}
